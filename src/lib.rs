pub mod devices;
use libc::{O_DIRECT, O_DSYNC, O_SYNC};
use lzma::reader::LzmaReader;
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::BufWriter;
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use tempfile::NamedTempFile;

/// Calculate the checksum of the data read from the given reader
fn calculate_checksum<R: Read>(reader: &mut R, size: usize) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer: Vec<u8> = Vec::with_capacity(size);

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        println!("still reading");
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub fn clone<F>(
    device_path: String,
    output_path: String,
    block_size: usize,
    silent: bool,
    callback_fn: Option<F>,
    channel: Option<tokio::sync::broadcast::Sender<String>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(f64),
{
    if !silent {
        println!(
            "device: {}, output: {}, block_size: {}",
            device_path, output_path, block_size
        );
    }

    // Open the device file with Direct IO
    let mut device_file = match OpenOptions::new()
        .read(true)
        .custom_flags(O_DIRECT)
        .custom_flags(O_SYNC)
        .custom_flags(O_DSYNC)
        .open(&device_path)
    {
        Ok(device_file) => device_file,
        Err(e) => {
            println!("Error opening device file: {}", device_path);
            return Err(Box::new(e));
        }
    };

    // Open the output file for writing
    let output_file = match File::create(&output_path) {
        Ok(output_file) => output_file,
        Err(e) => {
            println!("Error opening output file");
            return Err(Box::new(e));
        }
    };
    let mut writer = BufWriter::new(output_file);

    let mut buffer = vec![0u8; block_size];
    let mut total_bytes_read: usize = 0;

    // Read from the device and write to the file
    loop {
        let bytes_read = match device_file.read(&mut buffer) {
            Ok(bytes_read) => bytes_read,
            Err(e) => {
                println!("Error reading from device");
                return Err(Box::new(e));
            }
        };
        if bytes_read == 0 {
            break; // End of file
        }
        match writer.write_all(&buffer[..bytes_read]) {
            Ok(_) => {}
            Err(e) => {
                println!("Error writing to output file");
                return Err(Box::new(e));
            }
        };
        total_bytes_read += bytes_read;
        if !silent {
            println!("Read and written {total_bytes_read} bytes");
            // calculate percentage and call callback
            if let Some(callback) = &callback_fn {
                callback(total_bytes_read as f64 / 100.0);
            }
        }
        if let Some(channel) = channel.as_ref() {
            let _ = channel.send(format!("{total_bytes_read}")).unwrap();
        };
    }
    Ok(())
}

/// Flash the image at the given path to the device at the given path
pub fn flash<F>(
    img_path: String,
    device_path: String,
    block_size: usize,
    silent: bool,
    callback_fn: Option<F>,
    channel: Option<tokio::sync::broadcast::Sender<String>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(f64),
{
    // check if the img_path ends with .xz, if yes, call flash_xz
    if img_path.ends_with(".xz") {
        println!("calling flash_xz");
        flash_xz(
            img_path,
            device_path,
            block_size,
            silent,
            callback_fn,
            channel,
        )
    } else {
        let mut img_file = match File::open(&img_path) {
            Ok(file) => file,
            Err(e) => {
                println!("Image file not found");
                return Err(Box::new(e));
            }
        };
        let file_size = match img_file.metadata() {
            Ok(metadata) => metadata.len(),
            Err(e) => {
                println!("Error reading image file metadata");
                return Err(Box::new(e));
            }
        };
        let file_size_usize = match usize::try_from(file_size) {
            Ok(size) => size,
            Err(e) => {
                println!("File size too large");
                return Err(Box::new(e));
            }
        };
        let img_checksum = match calculate_checksum(&mut img_file, file_size_usize) {
            Ok(img_checksum) => img_checksum,
            Err(e) => {
                println!("Error calculating image checksum");
                return Err(Box::new(e));
            }
        };

        if !silent {
            println!("Source image checksum: {}", img_checksum);
        }

        // Write the image to the device
        let mut device_file = match OpenOptions::new()
            .write(true)
            .custom_flags(O_DIRECT)
            .custom_flags(O_SYNC)
            .custom_flags(O_DSYNC)
            .open(device_path.clone())
        {
            Ok(device_file) => device_file,
            Err(e) => {
                println!("Error opening device file");
                return Err(Box::new(e));
            }
        };

        let img_file = match File::open(&img_path) {
            Ok(img_file) => img_file,
            Err(e) => {
                println!("Error opening image file");
                return Err(Box::new(e));
            }
        };

        let mut reader = BufReader::new(img_file);
        let mut buffer = vec![0u8; block_size];

        if !silent {
            println!("Writing image to the device...: {file_size}");
        }

        let mut count: usize = 0;
        while let Ok(bytes_read) = reader.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            match device_file.write_all(&buffer[..bytes_read]) {
                Ok(_) => {}
                Err(e) => {
                    println!("Error while wriring to device");
                    return Err(Box::new(e));
                }
            };
            count += bytes_read;
            let percentage = (count * 100) / file_size as usize;
            if !silent {
                println!("written {count}/{file_size} : {percentage}%");

                // calculate percentage and call callback
                if let Some(callback) = &callback_fn {
                    callback(percentage as f64);
                }
            }
            if let Some(channel) = channel.as_ref() {
                let _ = channel.send(format!("{percentage}")).unwrap();
            };
        }

        // Calculate the checksum of the data on the SD card
        let device_file_ = match File::open(device_path) {
            Ok(dev_file) => dev_file,
            Err(e) => {
                println!("Error while opening the devicefile.");
                return Err(Box::new(e));
            }
        };
        let mut reader = BufReader::new(device_file_);
        let device_checksum = match calculate_checksum(&mut reader, file_size_usize) {
            Ok(dev_checksum) => dev_checksum,
            Err(e) => {
                println!("Error while calculating checksum");
                return Err(Box::new(e));
            }
        };
        if !silent {
            println!("Device checksum: {}", device_checksum);
        }

        // Compare the checksums
        if img_checksum == device_checksum {
            if !silent {
                println!("Checksums match. Write operation successful.");
            }
        } else {
            println!("Checksums do not match. Write operation may have failed.");
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Checksums do not match",
            )));
        }

        Ok(())
    }
}

/// Flash the compressed file (only xz compression is supported) at the given path to the device at the given path
pub fn flash_xz<F>(
    img_path: String,
    device_path: String,
    block_size: usize,
    silent: bool,
    callback_fn: Option<F>,
    channel: Option<tokio::sync::broadcast::Sender<String>>,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: Fn(f64),
{
    let temp_file = match NamedTempFile::new() {
        Ok(file) => file,
        Err(e) => {
            println!("Error creating temp file");
            return Err(Box::new(e));
        }
    };

    let temp_file_str = match temp_file.path().to_str() {
        Some(file_str) => file_str.to_string(),
        None => {
            println!("Error converting path to string");
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "Error converting path to string",
            )));
        }
    };
    let temp_file = temp_file_str;
    println!("temp_file: {:?}", temp_file);
    decompress_img(img_path.clone(), temp_file.clone())?;
    match flash(
        temp_file.clone(),
        device_path,
        block_size,
        silent,
        callback_fn,
        channel,
    ) {
        Ok(_) => {
            println!("Flash successful");

            println!("deleting temp file");
            if std::fs::remove_file(temp_file).is_err() {
                println!("Error removing temp file");
            }
            println!("done");
            Ok(())
        }
        Err(e) => {
            println!("Error flashing");
            println!("deleting temp file");
            if std::fs::remove_file(temp_file).is_err() {
                println!("Error removing temp file");
            }
            println!("done");
            Err(e)
        }
    }
}

/// decompress the input image and write to the output file
fn decompress_img(compressed_file: String, decompressed_file: String) -> std::io::Result<()> {
    let input_path = compressed_file;
    let output_path = decompressed_file;
    println!("input: {}\noutput: {}", input_path, output_path);

    // Open the input file
    let input_file = File::open(input_path)?;
    let buffered_reader = BufReader::new(input_file);

    // Create the LzmaReader
    let mut decoder = match LzmaReader::new_decompressor(buffered_reader) {
        Ok(decoder) => decoder,
        Err(e) => panic!("Error: {}", e),
    };

    // Open the output file
    let mut output_file = File::create(output_path)?;

    // Choose a buffer size (you can change this size as needed)
    let buffer_size = 33554432;
    let mut buffer = vec![0; buffer_size];

    // Read from the decoder and write to the output file
    loop {
        let bytes_read = decoder.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        output_file.write_all(&buffer[..bytes_read])?;
        println!("{} bytes read", bytes_read);
    }
    println!("\n done");
    Ok(())
}

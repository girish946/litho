use libc::{O_DIRECT, O_DSYNC, O_SYNC};
use std::fs::{File, OpenOptions};
use std::io::BufWriter;
use std::io::{BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;

pub fn clone(
    device_path: String,
    output_path: String,
    block_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "device: {}, output: {}, block_size: {}",
        device_path, output_path, block_size
    );
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
            println!("Error opening device file");
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
        println!("Read and written {total_bytes_read} bytes");
    }
    Ok(())
}

pub fn flash(
    img_path: String,
    device_path: String,
    block_size: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let img_file = match File::open(&img_path) {
        Ok(file) => file,
        Err(e) => {
            println!("Image file not found");
            return Err(Box::new(e));
        }
    };
    let file_size = img_file.metadata().unwrap().len();

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
    println!("Writing image to the device...: {file_size}");
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
        count = count + bytes_read;
        let percentage = (count * 100) / file_size as usize;
        println!("written {count}/{file_size} : {percentage}%");
    }

    Ok(())
}

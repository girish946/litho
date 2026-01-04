pub mod devices;
pub mod platform;

use anyhow::{Context, Result};
use log::{debug, info, warn};
use lzma::reader::LzmaReader;
use platform::PlatformDevice;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::BufWriter;
use std::io::{BufReader, Read, Write};
use tempfile::NamedTempFile;

/// Calculate the checksum of the data read from the given reader
fn calculate_checksum<R: Read>(reader: &mut R, size: usize) -> Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer: Vec<u8> = Vec::with_capacity(size);

    loop {
        let bytes_read = reader
            .read(&mut buffer)
            .context("Failed to read data for checksum calculation")?;
        if bytes_read == 0 {
            break;
        }
        debug!("Reading data for checksum calculation");
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
) -> Result<()>
where
    F: Fn(f64),
{
    if !silent {
        info!(
            "Cloning device: {} to output: {} with block_size: {}",
            device_path, output_path, block_size
        );
    }

    // Get platform-specific device reader
    let mut device_reader = PlatformDevice::new_reader(&device_path)?;

    // Open the output file for writing
    let output_file = File::create(&output_path)
        .context(format!("Failed to create output file: {}", output_path))?;
    let mut writer = BufWriter::new(output_file);

    let mut buffer = vec![0u8; block_size];
    let mut total_bytes_read: usize = 0;

    // Read from the device and write to the file
    loop {
        let bytes_read = device_reader
            .read(&mut buffer)
            .context("Failed to read from device")?;
        if bytes_read == 0 {
            break; // End of file
        }
        writer
            .write_all(&buffer[..bytes_read])
            .context("Failed to write to output file")?;
        total_bytes_read += bytes_read;
        if !silent {
            debug!("Read and written {} bytes", total_bytes_read);
            // calculate percentage and call callback
            if let Some(callback) = &callback_fn {
                callback(total_bytes_read as f64 / 100.0);
            }
        }
        if let Some(channel) = channel.as_ref() {
            let _ = channel.send(format!("{}", total_bytes_read));
        };
    }
    info!("Clone completed successfully");
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
) -> Result<()>
where
    F: Fn(f64),
{
    // check if the img_path ends with .xz, if yes, call flash_xz
    if img_path.ends_with(".xz") {
        info!("Detected compressed image, calling flash_xz");
        flash_xz(
            img_path,
            device_path,
            block_size,
            silent,
            callback_fn,
            channel,
        )
    } else {
        let mut img_file =
            File::open(&img_path).context(format!("Image file not found: {}", img_path))?;
        let file_size = img_file
            .metadata()
            .context("Failed to read image file metadata")?
            .len();
        let file_size_usize = usize::try_from(file_size).context("File size too large")?;
        let img_checksum = calculate_checksum(&mut img_file, file_size_usize)
            .context("Failed to calculate image checksum")?;

        if !silent {
            info!("Source image checksum: {}", img_checksum);
        }

        // Get platform-specific device writer
        let mut device_writer = PlatformDevice::new_writer(&device_path)?;

        let img_file =
            File::open(&img_path).context(format!("Failed to open image file: {}", img_path))?;

        let mut reader = BufReader::new(img_file);
        let mut buffer = vec![0u8; block_size];

        if !silent {
            info!("Writing image to the device... size: {}", file_size);
        }

        let mut count: usize = 0;
        while let Ok(bytes_read) = reader.read(&mut buffer) {
            if bytes_read == 0 {
                break;
            }
            device_writer
                .write_all(&buffer[..bytes_read])
                .context("Failed to write to device")?;
            count += bytes_read;
            let percentage = (count * 100) / file_size as usize;
            if !silent {
                debug!("Written {}/{} : {}%", count, file_size, percentage);

                // calculate percentage and call callback
                if let Some(callback) = &callback_fn {
                    callback(percentage as f64);
                }
            }
            if let Some(channel) = channel.as_ref() {
                let _ = channel.send(format!("{}", percentage));
            };
        }

        // Flush and sync the device writer
        device_writer
            .flush_and_sync()
            .context("Failed to flush and sync device")?;

        // Calculate the checksum of the data on the device
        let mut device_reader = PlatformDevice::new_reader(&device_path)?;
        let device_checksum = calculate_checksum(&mut device_reader, file_size_usize)
            .context("Failed to calculate device checksum")?;
        if !silent {
            info!("Device checksum: {}", device_checksum);
        }

        // Compare the checksums
        if img_checksum == device_checksum {
            if !silent {
                info!("Checksums match. Write operation successful.");
            }
        } else {
            log::error!("Checksums do not match. Write operation may have failed.");
            anyhow::bail!("Checksums do not match");
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
) -> Result<()>
where
    F: Fn(f64),
{
    let temp_file = NamedTempFile::new().context("Failed to create temporary file")?;

    let temp_file_str = temp_file
        .path()
        .to_str()
        .context("Failed to convert path to string")?
        .to_string();

    info!("Using temporary file: {}", temp_file_str);
    decompress_img(img_path.clone(), temp_file_str.clone())?;

    let result = flash(
        temp_file_str.clone(),
        device_path,
        block_size,
        silent,
        callback_fn,
        channel,
    );

    // Clean up temp file
    debug!("Deleting temporary file");
    if let Err(e) = std::fs::remove_file(&temp_file_str) {
        warn!("Failed to remove temporary file: {}", e);
    }

    result.context("Flash operation failed")?;
    info!("Flash successful");
    Ok(())
}

/// decompress the input image and write to the output file
fn decompress_img(compressed_file: String, decompressed_file: String) -> Result<()> {
    info!(
        "Decompressing: {} to {}",
        compressed_file, decompressed_file
    );

    // Open the input file
    let input_file = File::open(&compressed_file).context(format!(
        "Failed to open compressed file: {}",
        compressed_file
    ))?;
    let buffered_reader = BufReader::new(input_file);

    // Create the LzmaReader
    let mut decoder = LzmaReader::new_decompressor(buffered_reader)
        .context("Failed to create LZMA decompressor")?;

    // Open the output file
    let mut output_file = File::create(&decompressed_file).context(format!(
        "Failed to create output file: {}",
        decompressed_file
    ))?;

    // Choose a buffer size (you can change this size as needed)
    let buffer_size = 33554432;
    let mut buffer = vec![0; buffer_size];

    // Read from the decoder and write to the output file
    loop {
        let bytes_read = decoder
            .read(&mut buffer)
            .context("Failed to read from compressed stream")?;
        if bytes_read == 0 {
            break;
        }
        output_file
            .write_all(&buffer[..bytes_read])
            .context("Failed to write decompressed data")?;
        debug!("{} bytes decompressed", bytes_read);
    }
    info!("Decompression completed");
    Ok(())
}

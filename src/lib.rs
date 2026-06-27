pub mod cancel;
pub mod devices;
pub mod io_backend;
pub mod platform;
pub mod progress;

#[cfg(not(feature = "real-io"))]
pub mod cli_simulate;

use anyhow::{Context, Result};
use log::{debug, info, warn};
use lzma::reader::LzmaReader;
use platform::PlatformDevice;
use progress::{
    check_cancel, emit_progress, OperationCancelled, OperationPhase, OperationProgress,
};
use std::sync::atomic::AtomicBool;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::BufWriter;
use std::io::{BufReader, Read, Write};
use tempfile::NamedTempFile;

/// Calculate the checksum of the data read from the given reader
fn calculate_checksum<R: Read>(
    reader: &mut R,
    size: usize,
    cancel: Option<&AtomicBool>,
) -> Result<String> {
    let mut hasher = Sha256::new();
    let chunk = 65536usize;
    let mut buffer = vec![0u8; chunk];
    let mut remaining = size;

    while remaining > 0 {
        check_cancel(cancel)?;
        let to_read = remaining.min(buffer.len());
        let bytes_read = reader
            .read(&mut buffer[..to_read])
            .context("Failed to read data for checksum calculation")?;
        if bytes_read == 0 {
            break;
        }
        debug!("Reading data for checksum calculation");
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read;
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub fn clone<F>(
    device_path: String,
    output_path: String,
    block_size: usize,
    silent: bool,
    mut progress: Option<F>,
    cancel: Option<&AtomicBool>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    if !silent {
        info!(
            "Cloning device: {} to output: {} with block_size: {}",
            device_path, output_path, block_size
        );
    }

    emit_progress(
        silent,
        &mut progress,
        OperationProgress::new(OperationPhase::Preparing)
            .with_message(format!("Opening {}", device_path)),
    );

    let mut device_reader = PlatformDevice::new_clone_reader(&device_path)?;

    let total_bytes = device_reader
        .device_size()
        .ok()
        .filter(|s| *s > 0)
        .or_else(|| devices::device_size_bytes(&device_path));

    let output_file = File::create(&output_path)
        .context(format!("Failed to create output file: {}", output_path))?;
    let mut writer = BufWriter::new(output_file);

    let mut buffer = vec![0u8; block_size];
    let mut total_bytes_read: u64 = 0;

    let result = (|| -> Result<()> {
        loop {
            check_cancel(cancel)?;
            let bytes_read = device_reader
                .read(&mut buffer)
                .context("Failed to read from device")?;
            if bytes_read == 0 {
                break;
            }
            writer
                .write_all(&buffer[..bytes_read])
                .context("Failed to write to output file")?;
            total_bytes_read += bytes_read as u64;

            let mut event = OperationProgress::new(OperationPhase::Writing)
                .with_bytes(total_bytes_read, total_bytes);
            if total_bytes.is_none() {
                event = event.with_message(format!("{} bytes copied", total_bytes_read));
            }
            emit_progress(silent, &mut progress, event);

            if !silent {
                debug!("Read and written {} bytes", total_bytes_read);
            }
        }
        Ok(())
    })();

    if let Err(error) = result {
        drop(writer);
        if error.downcast_ref::<OperationCancelled>().is_some() {
            if let Err(remove_error) = std::fs::remove_file(&output_path) {
                warn!(
                    "Failed to remove incomplete clone output {}: {}",
                    output_path, remove_error
                );
            }
        }
        return Err(error);
    }

    writer
        .flush()
        .context("Failed to flush clone output file")?;

    emit_progress(
        silent,
        &mut progress,
        OperationProgress::new(OperationPhase::Complete)
            .with_bytes(total_bytes_read, total_bytes)
            .with_percentage(100.0)
            .with_message("Clone completed"),
    );

    info!("Clone completed successfully");
    Ok(())
}

/// Flash the image at the given path to the device at the given path.
///
/// When `verify` is false (default), the image is written and the operation
/// completes without a post-write checksum pass.
pub fn flash<F>(
    img_path: String,
    device_path: String,
    block_size: usize,
    silent: bool,
    verify: bool,
    progress: Option<F>,
    cancel: Option<&AtomicBool>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    if img_path.ends_with(".xz") {
        info!("Detected compressed image, calling flash_xz");
        flash_xz(
            img_path,
            device_path,
            block_size,
            silent,
            verify,
            progress,
            cancel,
        )
    } else {
        flash_image(
            img_path,
            device_path,
            block_size,
            silent,
            progress,
            false,
            verify,
            cancel,
        )
    }
}

fn flash_image<F>(
    img_path: String,
    device_path: String,
    block_size: usize,
    silent: bool,
    mut progress: Option<F>,
    skip_prepare: bool,
    verify: bool,
    cancel: Option<&AtomicBool>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    if !skip_prepare {
        emit_progress(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Preparing)
                .with_message(format!("Opening image {}", img_path)),
        );
    }

    check_cancel(cancel)?;

    let mut img_file =
        File::open(&img_path).context(format!("Image file not found: {}", img_path))?;
    let file_size = img_file
        .metadata()
        .context("Failed to read image file metadata")?
        .len();
    let file_size_usize = usize::try_from(file_size).context("File size too large")?;
    let img_checksum = if verify {
        let checksum = calculate_checksum(&mut img_file, file_size_usize, cancel)
            .context("Failed to calculate image checksum")?;
        if !silent {
            info!("Source image checksum: {}", checksum);
        }
        Some(checksum)
    } else {
        None
    };

    let mut device_writer = PlatformDevice::new_writer(&device_path)?;

    let img_file =
        File::open(&img_path).context(format!("Failed to open image file: {}", img_path))?;

    let mut reader = BufReader::new(img_file);
    let mut buffer = vec![0u8; block_size];

    if !silent {
        info!("Writing image to the device... size: {}", file_size);
    }

    let mut count: u64 = 0;
    loop {
        check_cancel(cancel)?;
        let bytes_read = reader
            .read(&mut buffer)
            .context("Failed to read image file")?;
        if bytes_read == 0 {
            break;
        }
        device_writer
            .write_all(&buffer[..bytes_read])
            .context("Failed to write to device")?;
        count += bytes_read as u64;
        let write_pct = if verify {
            (count as f64 / file_size as f64) * 90.0
        } else {
            (count as f64 / file_size as f64) * 100.0
        };
        emit_progress(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Writing)
                .with_bytes(count, Some(file_size))
                .with_percentage(write_pct),
        );

        if !silent {
            debug!(
                "Written {}/{} : {:.1}%",
                count,
                file_size,
                (count as f64 / file_size as f64) * 100.0
            );
        }
    }

    device_writer
        .flush_and_sync()
        .context("Failed to flush and sync device")?;

    if !verify {
        emit_progress(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Complete)
                .with_bytes(file_size, Some(file_size))
                .with_percentage(100.0)
                .with_message("Flash completed"),
        );
        if !silent {
            info!("Flash completed successfully");
        }
        return Ok(());
    }

    let img_checksum = img_checksum.context("Missing source checksum")?;

    emit_progress(
        silent,
        &mut progress,
        OperationProgress::new(OperationPhase::Verifying)
            .with_percentage(90.0)
            .with_message("Verifying checksum"),
    );

    let device_reader = PlatformDevice::new_verify_reader(&device_path)?;
    let mut buffered_reader = BufReader::with_capacity(1024 * 1024, device_reader);
    let mut verified: u64 = 0;
    let verify_hasher = verify_checksum_with_progress(
        &mut buffered_reader,
        file_size_usize,
        silent,
        &mut progress,
        &mut verified,
        file_size,
        cancel,
    )?;
    let device_checksum = format!("{:x}", verify_hasher.finalize());

    if !silent {
        info!("Device checksum: {}", device_checksum);
    }

    if img_checksum == device_checksum {
        emit_progress(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Complete)
                .with_percentage(100.0)
                .with_message("Checksums match"),
        );
        if !silent {
            info!("Checksums match. Write operation successful.");
        }
        Ok(())
    } else {
        emit_progress(
            silent,
            &mut progress,
            OperationProgress::new(OperationPhase::Failed).with_message("Checksums do not match"),
        );
        log::error!("Checksums do not match. Write operation may have failed.");
        anyhow::bail!("Checksums do not match");
    }
}

fn verify_checksum_with_progress<F>(
    reader: &mut dyn Read,
    size: usize,
    silent: bool,
    progress: &mut Option<F>,
    verified: &mut u64,
    file_size: u64,
    cancel: Option<&AtomicBool>,
) -> Result<Sha256>
where
    F: FnMut(OperationProgress),
{
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 65536];
    let mut remaining = size;

    while remaining > 0 {
        check_cancel(cancel)?;
        let to_read = remaining.min(buffer.len());
        let bytes_read = reader
            .read(&mut buffer[..to_read])
            .with_context(|| {
                format!(
                    "Failed to read from device during verification ({} bytes remaining)",
                    remaining
                )
            })?;
        if bytes_read == 0 {
            if remaining > 0 {
                anyhow::bail!(
                    "Unexpected end of device read during verification ({} bytes short)",
                    remaining
                );
            }
            break;
        }
        hasher.update(&buffer[..bytes_read]);
        remaining -= bytes_read;
        *verified += bytes_read as u64;
        let verify_pct = 90.0 + (*verified as f64 / file_size as f64) * 10.0;
        emit_progress(
            silent,
            progress,
            OperationProgress::new(OperationPhase::Verifying)
                .with_bytes(*verified, Some(file_size))
                .with_percentage(verify_pct.min(99.9)),
        );
    }

    Ok(hasher)
}

/// Flash the compressed file (only xz compression is supported) at the given path to the device at the given path
pub fn flash_xz<F>(
    img_path: String,
    device_path: String,
    block_size: usize,
    silent: bool,
    verify: bool,
    mut progress: Option<F>,
    cancel: Option<&AtomicBool>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    let temp_file = NamedTempFile::new().context("Failed to create temporary file")?;

    let temp_file_str = temp_file
        .path()
        .to_str()
        .context("Failed to convert path to string")?
        .to_string();

    info!("Using temporary file: {}", temp_file_str);
    decompress_img(
        img_path.clone(),
        temp_file_str.clone(),
        silent,
        &mut progress,
        cancel,
    )?;

    let result = flash_image(
        temp_file_str.clone(),
        device_path,
        block_size,
        silent,
        progress,
        true,
        verify,
        cancel,
    );

    debug!("Deleting temporary file");
    if let Err(e) = std::fs::remove_file(&temp_file_str) {
        warn!("Failed to remove temporary file: {}", e);
    }

    result.context("Flash operation failed")?;
    info!("Flash successful");
    Ok(())
}

/// decompress the input image and write to the output file
fn decompress_img<F>(
    compressed_file: String,
    decompressed_file: String,
    silent: bool,
    progress: &mut Option<F>,
    cancel: Option<&AtomicBool>,
) -> Result<()>
where
    F: FnMut(OperationProgress),
{
    info!(
        "Decompressing: {} to {}",
        compressed_file, decompressed_file
    );

    emit_progress(
        silent,
        progress,
        OperationProgress::new(OperationPhase::Decompressing)
            .with_message(format!("Decompressing {}", compressed_file)),
    );

    let input_file = File::open(&compressed_file).context(format!(
        "Failed to open compressed file: {}",
        compressed_file
    ))?;
    let buffered_reader = BufReader::new(input_file);

    let mut decoder = LzmaReader::new_decompressor(buffered_reader)
        .context("Failed to create LZMA decompressor")?;

    let mut output_file = File::create(&decompressed_file).context(format!(
        "Failed to create output file: {}",
        decompressed_file
    ))?;

    let buffer_size = 33554432;
    let mut buffer = vec![0; buffer_size];
    let mut decompressed: u64 = 0;

    loop {
        check_cancel(cancel)?;
        let bytes_read = decoder
            .read(&mut buffer)
            .context("Failed to read from compressed stream")?;
        if bytes_read == 0 {
            break;
        }
        output_file
            .write_all(&buffer[..bytes_read])
            .context("Failed to write decompressed data")?;
        decompressed += bytes_read as u64;
        emit_progress(
            silent,
            progress,
            OperationProgress::new(OperationPhase::Decompressing)
                .with_bytes(decompressed, None)
                .with_message(format!("{} bytes decompressed", decompressed)),
        );
        debug!("{} bytes decompressed", decompressed);
    }
    info!("Decompression completed");
    Ok(())
}

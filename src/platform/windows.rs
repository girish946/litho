use super::{DeviceReader, DeviceWriter};
use anyhow::{Context, Result};
use log::debug;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};

#[cfg(target_os = "windows")]
use std::os::windows::fs::OpenOptionsExt;
#[cfg(target_os = "windows")]
use winapi::um::winbase::{FILE_FLAG_NO_BUFFERING, FILE_FLAG_WRITE_THROUGH};

pub struct WindowsDeviceReader {
    file: File,
}

impl DeviceReader for WindowsDeviceReader {
    fn open(device_path: &str) -> Result<Self> {
        debug!("Opening Windows device for reading: {}", device_path);
        // On Windows, use physical drive format: \\.\PhysicalDrive0
        let windows_device = if !device_path.starts_with("\\\\.\\") {
            format!("\\\\.\\{}", device_path)
        } else {
            device_path.to_string()
        };

        #[cfg(target_os = "windows")]
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_NO_BUFFERING)
            .open(&windows_device)
            .context(format!(
                "Failed to open device for reading: {}",
                windows_device
            ))?;

        #[cfg(not(target_os = "windows"))]
        let file = File::open(&windows_device).context(format!(
            "Failed to open device for reading: {}",
            windows_device
        ))?;

        Ok(Self { file })
    }

    fn device_size(&self) -> Result<u64> {
        // On Windows, use IOCTL_DISK_GET_LENGTH_INFO to get device size
        // For now, use metadata as fallback
        let metadata = self
            .file
            .metadata()
            .context("Failed to get device metadata")?;
        Ok(metadata.len())
    }
}

impl Read for WindowsDeviceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

pub struct WindowsDeviceWriter {
    file: File,
}

impl DeviceWriter for WindowsDeviceWriter {
    fn open(device_path: &str) -> Result<Self> {
        debug!("Opening Windows device for writing: {}", device_path);
        // On Windows, use physical drive format: \\.\PhysicalDrive0
        let windows_device = if !device_path.starts_with("\\\\.\\") {
            format!("\\\\.\\{}", device_path)
        } else {
            device_path.to_string()
        };

        #[cfg(target_os = "windows")]
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH)
            .open(&windows_device)
            .context(format!(
                "Failed to open device for writing: {}",
                windows_device
            ))?;

        #[cfg(not(target_os = "windows"))]
        let file = OpenOptions::new()
            .write(true)
            .open(&windows_device)
            .context(format!(
                "Failed to open device for writing: {}",
                windows_device
            ))?;

        Ok(Self { file })
    }

    fn flush_and_sync(&mut self) -> Result<()> {
        self.file.flush().context("Failed to flush device")?;

        // On Windows, FILE_FLAG_WRITE_THROUGH ensures immediate write
        // sync_all() will also call FlushFileBuffers on Windows
        self.file.sync_all().context("Failed to sync device")?;

        debug!("Device flushed and synced");
        Ok(())
    }

    fn device_size(&self) -> Result<u64> {
        let metadata = self
            .file
            .metadata()
            .context("Failed to get device metadata")?;
        Ok(metadata.len())
    }
}

impl Write for WindowsDeviceWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

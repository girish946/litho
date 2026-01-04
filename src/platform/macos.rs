use super::{DeviceReader, DeviceWriter};
use anyhow::{Context, Result};
use log::debug;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::io::AsRawFd;

pub struct MacDeviceReader {
    file: File,
}

impl DeviceReader for MacDeviceReader {
    fn open(device_path: &str) -> Result<Self> {
        debug!("Opening macOS device for reading: {}", device_path);
        // On macOS, use raw disk device (e.g., /dev/rdisk2 instead of /dev/disk2)
        let raw_device = if device_path.starts_with("/dev/disk") {
            device_path.replace("/dev/disk", "/dev/rdisk")
        } else {
            device_path.to_string()
        };

        let file = OpenOptions::new()
            .read(true)
            .open(&raw_device)
            .context(format!("Failed to open device for reading: {}", raw_device))?;
        Ok(Self { file })
    }

    fn device_size(&self) -> Result<u64> {
        // On macOS, we can use DKIOCGETBLOCKCOUNT ioctl to get device size
        // For now, use metadata as fallback
        let metadata = self
            .file
            .metadata()
            .context("Failed to get device metadata")?;
        Ok(metadata.len())
    }
}

impl Read for MacDeviceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

pub struct MacDeviceWriter {
    file: File,
}

impl DeviceWriter for MacDeviceWriter {
    fn open(device_path: &str) -> Result<Self> {
        debug!("Opening macOS device for writing: {}", device_path);
        // On macOS, use raw disk device (e.g., /dev/rdisk2 instead of /dev/disk2)
        let raw_device = if device_path.starts_with("/dev/disk") {
            device_path.replace("/dev/disk", "/dev/rdisk")
        } else {
            device_path.to_string()
        };

        let file = OpenOptions::new()
            .write(true)
            .open(&raw_device)
            .context(format!("Failed to open device for writing: {}", raw_device))?;
        Ok(Self { file })
    }

    fn flush_and_sync(&mut self) -> Result<()> {
        self.file.flush().context("Failed to flush device")?;

        // Call fsync on macOS
        unsafe {
            if libc::fsync(self.file.as_raw_fd()) != 0 {
                return Err(std::io::Error::last_os_error()).context("Failed to sync device");
            }
            // On macOS, also call F_FULLFSYNC for complete sync
            if libc::fcntl(self.file.as_raw_fd(), libc::F_FULLFSYNC) != 0 {
                return Err(std::io::Error::last_os_error()).context("Failed to full sync device");
            }
        }
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

impl Write for MacDeviceWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

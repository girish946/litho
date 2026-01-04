use super::{DeviceReader, DeviceWriter};
use anyhow::{Context, Result};
use libc::{O_DIRECT, O_DSYNC, O_SYNC};
use log::debug;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

pub struct LinuxDeviceReader {
    file: File,
}

impl DeviceReader for LinuxDeviceReader {
    fn open(device_path: &str) -> Result<Self> {
        debug!("Opening Linux device for reading: {}", device_path);
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(O_DIRECT | O_SYNC | O_DSYNC)
            .open(device_path)
            .context(format!(
                "Failed to open device for reading: {}",
                device_path
            ))?;
        Ok(Self { file })
    }

    fn device_size(&self) -> Result<u64> {
        let metadata = self
            .file
            .metadata()
            .context("Failed to get device metadata")?;
        Ok(metadata.len())
    }
}

impl Read for LinuxDeviceReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

pub struct LinuxDeviceWriter {
    file: File,
}

impl DeviceWriter for LinuxDeviceWriter {
    fn open(device_path: &str) -> Result<Self> {
        debug!("Opening Linux device for writing: {}", device_path);
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(O_DIRECT | O_SYNC | O_DSYNC)
            .open(device_path)
            .context(format!(
                "Failed to open device for writing: {}",
                device_path
            ))?;
        Ok(Self { file })
    }

    fn flush_and_sync(&mut self) -> Result<()> {
        self.file.flush().context("Failed to flush device")?;

        // Call fsync to ensure data is written to disk
        unsafe {
            if libc::fsync(self.file.as_raw_fd()) != 0 {
                return Err(std::io::Error::last_os_error()).context("Failed to sync device");
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

impl Write for LinuxDeviceWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

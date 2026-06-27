use anyhow::Result;
use std::io::{Read, Write};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "windows")]
mod windows;

/// Trait for reading from a device in a platform-specific way
pub trait DeviceReader: Read {
    /// Open a device for reading
    fn open(device_path: &str) -> Result<Self>
    where
        Self: Sized;

    /// Get the size of the device in bytes
    fn device_size(&self) -> Result<u64>;
}

/// Trait for writing to a device in a platform-specific way
pub trait DeviceWriter: Write {
    /// Open a device for writing
    fn open(device_path: &str) -> Result<Self>
    where
        Self: Sized;

    /// Flush and sync data to ensure it's written to the device
    fn flush_and_sync(&mut self) -> Result<()>;

    /// Get the size of the device in bytes
    fn device_size(&self) -> Result<u64>;
}

/// Platform-specific device implementation factory
pub struct PlatformDevice;

impl PlatformDevice {
    /// Create a new platform-specific device reader
    pub fn new_reader(device_path: &str) -> Result<Box<dyn DeviceReader>> {
        #[cfg(target_os = "linux")]
        {
            Ok(Box::new(linux::LinuxDeviceReader::open(device_path)?))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Box::new(macos::MacDeviceReader::open(device_path)?))
        }
        #[cfg(target_os = "windows")]
        {
            Ok(Box::new(windows::WindowsDeviceReader::open(device_path)?))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            anyhow::bail!("Unsupported platform")
        }
    }

    /// Open a device for clone or verify reads (buffered/cached I/O).
    ///
    /// Clone must not use `O_DIRECT`: `read(2)` buffers from `Vec` are not sector-aligned.
    pub fn new_clone_reader(device_path: &str) -> Result<Box<dyn DeviceReader>> {
        Self::new_verify_reader(device_path)
    }

    /// Open a device for post-write checksum verification (buffered/cached reads).
    pub fn new_verify_reader(device_path: &str) -> Result<Box<dyn DeviceReader>> {
        #[cfg(target_os = "linux")]
        {
            Ok(Box::new(linux::LinuxBufferedDeviceReader::open(device_path)?))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Box::new(macos::MacDeviceReader::open(device_path)?))
        }
        #[cfg(target_os = "windows")]
        {
            Ok(Box::new(windows::WindowsDeviceReader::open(device_path)?))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            anyhow::bail!("Unsupported platform")
        }
    }

    /// Create a new platform-specific device writer
    pub fn new_writer(device_path: &str) -> Result<Box<dyn DeviceWriter>> {
        #[cfg(target_os = "linux")]
        {
            Ok(Box::new(linux::LinuxDeviceWriter::open(device_path)?))
        }
        #[cfg(target_os = "macos")]
        {
            Ok(Box::new(macos::MacDeviceWriter::open(device_path)?))
        }
        #[cfg(target_os = "windows")]
        {
            Ok(Box::new(windows::WindowsDeviceWriter::open(device_path)?))
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
        {
            anyhow::bail!("Unsupported platform")
        }
    }
}

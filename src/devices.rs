use anyhow::{Context, Result};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct DeviceInfo {
    pub device_name: String,
    pub vendor_name: String,
    pub model_name: String,
    pub removable: u8,
    pub size: u64,
}

impl fmt::Display for DeviceInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", json!(self).to_string())
    }
}

/// function to read the file contents as a string
fn get_file_content(input_file: String) -> Result<String> {
    let content =
        fs::read_to_string(&input_file).context(format!("Failed to read file: {}", input_file))?;
    Ok(content)
}

pub fn is_removable_device(device_path: &str) -> Result<bool> {
    // Extract device name from the device path
    let device_name = Path::new(device_path)
        .file_name()
        .context("Invalid device path")?
        .to_str()
        .context("Non UTF-8 device name")?;

    // Construct the path to the removable file
    let removable_path = format!("/sys/block/{}/removable", device_name);
    debug!("Checking removable path: {}", removable_path);

    // Read the contents of the removable file
    let mut file =
        fs::File::open(&removable_path).context(format!("Failed to open {}", removable_path))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .context("Failed to read removable file")?;

    // Check if the device is removable
    Ok(contents.trim() == "1")
}

pub fn get_storage_devices() -> Result<Vec<DeviceInfo>> {
    let paths = fs::read_dir("/sys/block/").context("Failed to read /sys/block/ directory")?;
    let mut devices: Vec<DeviceInfo> = Vec::new();

    for path in paths {
        let p = path.context("Failed to read directory entry")?;
        let mut dev = p.path().clone();
        let device_end_name = match p.path().clone().file_name() {
            Some(device) => match device.to_str() {
                Some(dev) => dev.to_string(),
                None => {
                    warn!("Could not convert device name to string");
                    continue;
                }
            },
            None => {
                warn!("Could not get device name");
                continue;
            }
        };

        dev.push("device");
        if dev.exists() {
            dev.push("vendor");
            let vendor_name_file = match dev.clone().into_os_string().to_str() {
                Some(name) => name.to_string(),
                None => {
                    warn!("Could not convert vendor path to string");
                    String::new()
                }
            };

            let mut dev_vendor_name = String::new();
            if !vendor_name_file.is_empty() {
                dev_vendor_name = match get_file_content(vendor_name_file) {
                    Ok(name) => name.replace('\n', ""),
                    Err(e) => {
                        warn!("Failed to read vendor name: {}", e);
                        String::new()
                    }
                };
            }

            dev.pop();
            dev.push("model");
            let model_name_file = match dev.clone().into_os_string().to_str() {
                Some(name) => name.to_string(),
                None => String::new(),
            };

            let model_name = match get_file_content(model_name_file) {
                Ok(model) => model.replace('\n', ""),
                Err(e) => {
                    warn!("Failed to read model name: {}", e);
                    String::new()
                }
            };

            dev.pop();
            dev.pop();
            dev.push("removable");
            let mut removable = String::new();
            if dev.exists() {
                removable = match fs::read_to_string(dev.clone()) {
                    Ok(removable) => removable,
                    Err(e) => {
                        warn!("Failed to read removable status: {}", e);
                        String::new()
                    }
                }
            }

            dev.pop();
            dev.push("size");
            let size: u64 = match fs::read_to_string(dev.clone()) {
                Ok(size_str) => match size_str.trim().parse::<u64>() {
                    Ok(s) => s,
                    Err(e) => {
                        warn!("Failed to parse size: {}", e);
                        0
                    }
                },
                Err(e) => {
                    warn!("Failed to read size: {}", e);
                    0
                }
            };

            if !device_end_name.is_empty() {
                let mut dev_path = PathBuf::from("/dev/");
                dev_path.push(device_end_name);
                if dev_path.exists() {
                    if removable == "1\n" {
                        let dev_info = DeviceInfo {
                            device_name: dev_path.display().to_string(),
                            vendor_name: dev_vendor_name,
                            model_name,
                            removable: 1,
                            size,
                        };
                        devices.push(dev_info);
                    } else if removable == "0\n" {
                        let dev_info = DeviceInfo {
                            device_name: dev_path.display().to_string(),
                            vendor_name: dev_vendor_name,
                            model_name,
                            removable: 0,
                            size,
                        };
                        devices.push(dev_info);
                    }
                }
            }
        }
    }
    debug!("Found {} devices", devices.len());
    Ok(devices)
}

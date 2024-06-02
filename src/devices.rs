use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::io::{self, Read};
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
/// function to read the file contents as a string
fn get_file_content(input_file: String) -> Result<String, Box<dyn std::error::Error>> {
    let content = match fs::read_to_string(input_file.clone()) {
        Ok(s) => s,
        Err(e) => {
            format!(
                "error coccured while reading :{}: {}",
                input_file.clone(),
                e
            );
            "".to_string()
        }
    };

    Ok(content)
}

pub fn is_removable_device(device_path: &str) -> io::Result<bool> {
    // Extract device name from the device path
    let device_name = Path::new(device_path)
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Invalid device path"))?
        .to_str()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Non UTF-8 device name"))?;

    // Construct the path to the removable file
    let removable_path = format!("/sys/block/{}/removable", device_name);

    println!("removable_path: {}", removable_path);

    // Read the contents of the removable file
    let mut file = fs::File::open(removable_path)?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)?;

    // Check if the device is removable
    Ok(contents.trim() == "1")
}

pub fn get_storage_devices() -> Result<Vec<String>, String> {
    let paths = match fs::read_dir("/sys/block/") {
        Ok(paths) => paths,
        Err(e) => {
            println!("could not get the subdirs of /sys/block/: {}", e);
            return Err("".to_string());
        }
    };
    let mut devices: Vec<String> = Vec::new();

    for path in paths {
        let p = match path {
            Ok(p) => p,
            Err(e) => {
                println!("could not get the path: {}", e);
                return Err("".to_string());
            }
        };
        let mut dev = p.path().clone();
        let device_end_name = match p.path().clone().file_name() {
            Some(device) => match device.to_str() {
                Some(dev) => dev.to_string(),
                None => {
                    println!("could not get the device name");
                    "".to_string()
                }
            },

            None => {
                println!("could not get the device name ");
                "".to_string()
            }
        };

        dev.push("device");
        if dev.exists() {
            dev.push("vendor");
            let vendor_name_file = match dev.clone().into_os_string().to_str() {
                Some(name) => name.to_string(),
                None => "".to_string(),
            };

            let mut dev_vendor_name = "".to_string();
            if !vendor_name_file.is_empty() {
                dev_vendor_name = match get_file_content(vendor_name_file) {
                    Ok(name) => name.replace('\n', ""),
                    Err(e) => {
                        println!("error occurred while reading vendor name: {}", e);
                        "".to_string()
                    }
                };
            }
            dev.pop();
            dev.push("model");
            let model_name_file = match dev.clone().into_os_string().to_str() {
                Some(name) => name.to_string(),
                None => "".to_string(),
            };

            let model_name = match get_file_content(model_name_file) {
                Ok(model) => model.replace('\n', ""),
                Err(e) => {
                    println!("error occurred while reading model name: {}", e);
                    "".to_string()
                }
            };
            dev.pop();
            dev.pop();
            dev.push("removable");
            let mut removable = "".to_string();
            if dev.exists() {
                removable = match fs::read_to_string(dev.clone()) {
                    Ok(removable) => removable,
                    Err(e) => {
                        println!("error occurred while reading removable: {}", e);
                        "".to_string()
                    }
                }
                // println!("removable: {}", removable);
            }

            dev.pop();
            dev.push("size");
            let size: u64 = match fs::read_to_string(dev.clone()) {
                Ok(size_str) => {
                    // println!("size: {}", size_str);
                    match size_str.trim().parse::<u64>() {
                        Ok(s) => s,
                        Err(e) => {
                            println!("error occurred while parsing size: {}", e);
                            0
                        }
                    }
                }
                Err(e) => {
                    println!("error occurred while reading size: {}", e);
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
                        let device_json = json!(dev_info).to_string();
                        devices.push(device_json);
                    } else if removable == "0\n" {
                        let dev_info = DeviceInfo {
                            device_name: dev_path.display().to_string(),
                            vendor_name: dev_vendor_name,
                            model_name,
                            removable: 0,
                            size,
                        };
                        let dev_json = json!(dev_info).to_string();
                        devices.push(dev_json);
                    }
                }
            }
        }
    }
    println!("devices: {:?}", devices);
    Ok(devices)
}

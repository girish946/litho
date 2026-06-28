use crate::devices::DeviceInfo;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use wmi::{COMLibrary, WMIConnection};

#[allow(non_camel_case_types)]
#[derive(Debug, Deserialize)]
#[serde(rename = "Win32_DiskDrive")]
#[serde(rename_all = "PascalCase")]
struct Win32DiskDrive {
    device_id: String,
    model: Option<String>,
    manufacturer: Option<String>,
    size: Option<u64>,
    media_type: Option<String>,
    index: Option<u32>,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Deserialize)]
#[serde(rename = "Win32_LogicalDiskToPartition")]
struct Win32LogicalDiskToPartition {
    #[serde(rename = "Antecedent")]
    antecedent: String,
    #[serde(rename = "Dependent")]
    dependent: String,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Deserialize)]
#[serde(rename = "Win32_DiskPartition")]
#[serde(rename_all = "PascalCase")]
struct Win32DiskPartition {
    device_id: String,
    disk_index: Option<u32>,
}

fn trim_or_empty(value: Option<String>) -> String {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_default()
}

fn is_removable(media_type: &Option<String>) -> u8 {
    media_type
        .as_ref()
        .map(|media| {
            let lower = media.to_lowercase();
            lower.contains("removable media") || lower.contains("external hard disk media")
        })
        .map(|removable| u8::from(removable))
        .unwrap_or(0)
}

fn extract_quoted_value(path: &str) -> Option<String> {
    let start = path.find('"')? + 1;
    let end = path.rfind('"')?;
    if end <= start {
        return None;
    }

    let value = path[start..end].trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn build_partition_to_drive_map(
    wmi: &WMIConnection,
) -> Result<HashMap<String, Vec<String>>> {
    let links: Vec<Win32LogicalDiskToPartition> = wmi
        .raw_query("SELECT Antecedent, Dependent FROM Win32_LogicalDiskToPartition")
        .context("Failed to query Win32_LogicalDiskToPartition")?;

    let mut map: HashMap<String, Vec<String>> = HashMap::new();
    for link in links {
        let Some(partition) = extract_quoted_value(&link.antecedent) else {
            continue;
        };
        let Some(drive_letter) = extract_quoted_value(&link.dependent)
            .map(|letter| letter.trim_end_matches('\\').to_string())
        else {
            continue;
        };
        map.entry(partition).or_default().push(drive_letter);
    }

    for letters in map.values_mut() {
        letters.sort();
        letters.dedup();
    }

    Ok(map)
}

fn build_disk_index_to_drive_letters(
    partitions: &[Win32DiskPartition],
    partition_to_drive: &HashMap<String, Vec<String>>,
) -> HashMap<u32, Vec<String>> {
    let mut map: HashMap<u32, Vec<String>> = HashMap::new();

    for partition in partitions {
        let Some(disk_index) = partition.disk_index else {
            continue;
        };
        let Some(mut letters) = partition_to_drive.get(&partition.device_id).cloned() else {
            continue;
        };

        map.entry(disk_index).or_default().append(&mut letters);
    }

    for letters in map.values_mut() {
        letters.sort();
        letters.dedup();
    }

    map
}

fn size_in_sectors(size_bytes: Option<u64>) -> u64 {
    size_bytes.map(|bytes| bytes / 512).unwrap_or(0)
}

pub fn get_storage_devices() -> Result<Vec<DeviceInfo>> {
    let com = COMLibrary::new().context("Failed to initialize COM for WMI")?;
    let wmi = WMIConnection::new(com.into()).context("Failed to connect to WMI")?;

    let raw_drives: Vec<Win32DiskDrive> = wmi
        .query()
        .context("Failed to query Win32_DiskDrive")?;
    let partitions: Vec<Win32DiskPartition> = wmi
        .raw_query("SELECT DeviceID, DiskIndex FROM Win32_DiskPartition")
        .context("Failed to query Win32_DiskPartition")?;
    let partition_to_drive = build_partition_to_drive_map(&wmi)?;
    let disk_index_to_letters = build_disk_index_to_drive_letters(&partitions, &partition_to_drive);

    let mut devices = Vec::with_capacity(raw_drives.len());
    for raw in raw_drives {
        let device_name = raw.device_id.trim().to_string();
        let _drive_letters = raw
            .index
            .and_then(|index| disk_index_to_letters.get(&index).cloned())
            .unwrap_or_default();

        devices.push(DeviceInfo {
            device_name,
            vendor_name: trim_or_empty(raw.manufacturer),
            model_name: trim_or_empty(raw.model),
            removable: is_removable(&raw.media_type),
            size: size_in_sectors(raw.size),
        });
    }

    devices.sort_by_key(|device| {
        parse_physical_drive_index(&device.device_name).unwrap_or(u32::MAX)
    });

    Ok(devices)
}

fn parse_physical_drive_index(device_name: &str) -> Option<u32> {
    let upper = device_name.to_ascii_uppercase();
    let suffix = upper
        .strip_prefix(r"\\.\PHYSICALDRIVE")
        .or_else(|| upper.strip_prefix("PHYSICALDRIVE"))?;
    suffix.parse().ok()
}

pub fn device_path_matches(device_name: &str, query_path: &str) -> bool {
    fn normalize(path: &str) -> String {
        let trimmed = path.trim();
        let without_prefix = trimmed
            .strip_prefix(r"\\.\")
            .or_else(|| trimmed.strip_prefix(r"\\.\"))
            .unwrap_or(trimmed);
        without_prefix.to_ascii_uppercase()
    }

    normalize(device_name) == normalize(query_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_path_matches_physical_drive_aliases() {
        assert!(device_path_matches(
            r"\\.\PHYSICALDRIVE0",
            r"\\.\PhysicalDrive0"
        ));
        assert!(device_path_matches("PhysicalDrive1", r"\\.\PHYSICALDRIVE1"));
        assert!(!device_path_matches(r"\\.\PHYSICALDRIVE0", r"\\.\PHYSICALDRIVE1"));
    }

    #[test]
    fn parse_physical_drive_index_handles_prefixes() {
        assert_eq!(parse_physical_drive_index(r"\\.\PHYSICALDRIVE2"), Some(2));
        assert_eq!(parse_physical_drive_index("PhysicalDrive3"), Some(3));
    }
}
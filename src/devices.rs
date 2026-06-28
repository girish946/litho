use anyhow::{Context, Result};
use log::{debug, warn};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

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

/// Require an exact whole-block device path such as `/dev/sdb` (no normalization).
pub fn validate_block_device_path(path: &str) -> Result<(), String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Device path is empty.".to_string());
    }
    if !trimmed.starts_with("/dev/") {
        return Err(format!(
            "Device must be a full block device path (e.g. /dev/sdb), got: {trimmed}"
        ));
    }
    let name = trimmed
        .strip_prefix("/dev/")
        .ok_or_else(|| format!("Invalid device path: {trimmed}"))?;
    if name.is_empty() {
        return Err(format!("Invalid device path: {trimmed}"));
    }
    if is_partition_block_name(name) {
        return Err(format!(
            "Partitions are not allowed; use the whole block device (got {trimmed})"
        ));
    }
    if is_rejected_block_name(name) {
        return Err(format!("Device type is not allowed for flash/clone: {trimmed}"));
    }
    if !is_whole_block_device_name(name) {
        return Err(format!("Not a recognized whole block device: {trimmed}"));
    }
    if !Path::new(trimmed).exists() {
        return Err(format!("Device path does not exist: {trimmed}"));
    }
    Ok(())
}

/// Require that `path` is a valid block device and appears in `known` (picker flows).
pub fn validate_listed_block_device(path: &str, known: &[impl AsRef<str>]) -> Result<(), String> {
    validate_device_safe_for_io(path)?;
    if !known.iter().any(|entry| entry.as_ref() == path) {
        return Err(format!(
            "Device {path} is not in the current device list. Refresh devices and select again."
        ));
    }
    Ok(())
}

/// Validate path format and refuse the system disk or mounted targets.
pub fn validate_device_safe_for_io(path: &str) -> Result<(), String> {
    validate_block_device_path(path)?;
    validate_device_not_system_disk(path)?;
    validate_device_not_busy(path)?;
    Ok(())
}

/// Refuse the whole block device that hosts the root filesystem.
pub fn validate_device_not_system_disk(path: &str) -> Result<(), String> {
    let target_whole = whole_disk_path(path)?;
    let root_sources = root_filesystem_sources()?;
    for source in &root_sources {
        let source_whole = whole_disk_path_from_source(source)?;
        if source_whole == target_whole {
            return Err(format!(
                "Refusing {path}: it is the system disk (root filesystem is on {source})"
            ));
        }
    }
    Ok(())
}

/// Refuse devices with partitions or the whole disk currently mounted.
pub fn validate_device_not_busy(path: &str) -> Result<(), String> {
    let mounts = busy_mounts_for_device(path)?;
    if mounts.is_empty() {
        return Ok(());
    }
    let details: Vec<String> = mounts
        .iter()
        .map(|(src, mp)| format!("{src} on {mp}"))
        .collect();
    Err(format!(
        "Refusing {path}: device is mounted ({})",
        details.join(", ")
    ))
}

fn scsi_disk_stem(name: &str) -> Option<&str> {
    name.strip_prefix("sd")
        .or_else(|| name.strip_prefix("vd"))
        .or_else(|| name.strip_prefix("hd"))
}

fn is_partition_block_name(name: &str) -> bool {
    if name.starts_with("mmcblk") {
        return name.rsplit_once('p').is_some_and(|(_, suffix)| {
            !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
        });
    }
    if name.starts_with("nvme") {
        return name.rsplit_once('p').is_some_and(|(prefix, suffix)| {
            prefix.contains('n')
                && !suffix.is_empty()
                && suffix.chars().all(|c| c.is_ascii_digit())
        });
    }
    if let Some(stem) = scsi_disk_stem(name) {
        return stem.chars().any(|c| c.is_ascii_digit());
    }
    false
}

fn is_rejected_block_name(name: &str) -> bool {
    name.starts_with("loop") || name.starts_with("dm-") || name.starts_with("ram")
}

/// Map a block device name or path to its parent whole-disk path (`/dev/sdb`, …).
pub fn whole_disk_path(path_or_name: &str) -> Result<String, String> {
    let name = if path_or_name.starts_with("/dev/") {
        path_or_name
            .strip_prefix("/dev/")
            .ok_or_else(|| format!("Invalid device path: {path_or_name}"))?
    } else {
        path_or_name
    };
    Ok(format!("/dev/{}", whole_disk_name_from_block(name)))
}

fn whole_disk_path_from_source(source: &str) -> Result<String, String> {
    if !source.starts_with("/dev/") {
        return Err(format!("Unsupported root device source: {source}"));
    }
    whole_disk_path(source)
}

fn whole_disk_name_from_block(name: &str) -> String {
    if is_partition_block_name(name) {
        if let Some((stem, suffix)) = name.rsplit_once('p') {
            if (name.starts_with("mmcblk") || name.starts_with("nvme"))
                && !suffix.is_empty()
                && suffix.chars().all(|c| c.is_ascii_digit())
            {
                return stem.to_string();
            }
        }
        let prefix: String = name.chars().take_while(|c| !c.is_ascii_digit()).collect();
        if !prefix.is_empty() && prefix.len() < name.len() {
            return prefix;
        }
    }
    name.to_string()
}

fn normalize_mount_source(source: &str) -> String {
    source
        .split_once('[')
        .map(|(base, _)| base)
        .unwrap_or(source)
        .trim()
        .to_string()
}

fn root_filesystem_sources() -> Result<Vec<String>, String> {
    match root_filesystem_source_from_findmnt() {
        Ok(source) => expand_block_sources(&normalize_mount_source(&source)),
        Err(findmnt_err) => {
            let source = root_filesystem_source_from_proc_mounts()
                .map_err(|proc_err| format!("{findmnt_err}; fallback: {proc_err}"))?;
            expand_block_sources(&normalize_mount_source(&source))
        }
    }
}

fn root_filesystem_source_from_findmnt() -> Result<String, String> {
    let output = Command::new("findmnt")
        .args(["-n", "-o", "SOURCE", "--target", "/"])
        .output()
        .map_err(|e| format!("Failed to run findmnt: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "findmnt failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let source = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if source.is_empty() {
        return Err("findmnt returned an empty root source".to_string());
    }
    Ok(source)
}

fn root_filesystem_source_from_proc_mounts() -> Result<String, String> {
    let contents = fs::read_to_string("/proc/mounts")
        .map_err(|e| format!("Failed to read /proc/mounts: {e}"))?;
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let Some(source) = parts.next() else {
            continue;
        };
        let Some(mount_point) = parts.next() else {
            continue;
        };
        if mount_point == "/" {
            return Ok(source.to_string());
        }
    }
    Err("Root mount not found in /proc/mounts".to_string())
}

fn expand_block_sources(source: &str) -> Result<Vec<String>, String> {
    if !source.starts_with("/dev/") {
        return Err(format!(
            "Root filesystem source {source} is not a block device path"
        ));
    }

    let block_name = Path::new(source)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("Invalid root device source: {source}"))?;

    if block_name.starts_with("dm-") || source.contains("/mapper/") {
        if let Some(dm_name) = dm_sysfs_name(source).or_else(|| {
            fs::read_link(source)
                .ok()
                .and_then(|link| link.file_name().and_then(|n| n.to_str()).map(str::to_string))
                .filter(|name| name.starts_with("dm-"))
        }) {
            let slaves_dir = format!("/sys/block/{dm_name}/slaves");
            if let Ok(entries) = fs::read_dir(&slaves_dir) {
                let slaves: Vec<String> = entries
                    .filter_map(|entry| entry.ok())
                    .filter_map(|entry| entry.file_name().into_string().ok())
                    .map(|name| format!("/dev/{name}"))
                    .collect();
                if !slaves.is_empty() {
                    return Ok(slaves);
                }
            }
        }
    }

    Ok(vec![source.to_string()])
}

fn dm_sysfs_name(source: &str) -> Option<String> {
    let path = Path::new(source);
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with("dm-") {
            return Some(name.to_string());
        }
    }

    let link = fs::read_link(source).ok()?;
    let link_name = link.file_name().and_then(|n| n.to_str())?;
    if link_name.starts_with("dm-") {
        return Some(link_name.to_string());
    }

    None
}

fn busy_mounts_for_device(device_path: &str) -> Result<Vec<(String, String)>, String> {
    let target_whole = whole_disk_path(device_path)?;
    let contents = fs::read_to_string("/proc/mounts")
        .map_err(|e| format!("Failed to read /proc/mounts: {e}"))?;

    let mut mounts = Vec::new();
    for line in contents.lines() {
        let mut parts = line.split_whitespace();
        let Some(source) = parts.next() else {
            continue;
        };
        let Some(mount_point) = parts.next() else {
            continue;
        };
        if !source.starts_with("/dev/") {
            continue;
        }
        let source_whole = whole_disk_path_from_source(source)?;
        if source_whole == target_whole {
            mounts.push((source.to_string(), mount_point.to_string()));
        }
    }
    Ok(mounts)
}

fn is_whole_block_device_name(name: &str) -> bool {
    if is_partition_block_name(name) || is_rejected_block_name(name) {
        return false;
    }
    if let Some(stem) = scsi_disk_stem(name) {
        return !stem.is_empty()
            && stem.chars().all(|c| c.is_ascii_lowercase())
            && !stem.chars().any(|c| c.is_ascii_digit());
    }
    if name.starts_with("mmcblk") {
        return name["mmcblk".len()..]
            .chars()
            .all(|c| c.is_ascii_digit());
    }
    if name.starts_with("nvme") {
        let rest = &name[4..];
        return !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == 'n');
    }
    false
}

#[cfg(test)]
mod validation_tests {
    use super::*;

    #[test]
    fn accepts_whole_block_paths() {
        for name in ["sdb", "mmcblk0", "nvme0n1", "vda"] {
            assert!(is_whole_block_device_name(name), "{name}");
            assert!(!is_partition_block_name(name), "{name}");
        }
    }

    #[test]
    fn rejects_partitions_and_loops() {
        for name in ["sdb1", "mmcblk0p1", "nvme0n1p2", "loop0", "dm-0"] {
            assert!(
                is_partition_block_name(name) || is_rejected_block_name(name),
                "{name}"
            );
        }
    }

    #[test]
    fn validate_requires_dev_prefix() {
        assert!(validate_block_device_path("sdb").is_err());
        assert!(validate_block_device_path("/dev/sdb1").is_err());
    }

    #[test]
    fn optimal_io_block_size_from_sectors_matches_lithographer_table() {
        assert_eq!(optimal_io_block_size_from_sectors(2_048), 4_096);
        assert_eq!(optimal_io_block_size_from_sectors(4_096), 4_096);
        assert_eq!(optimal_io_block_size_from_sectors(5_000), 8_192);
        assert_eq!(optimal_io_block_size_from_sectors(100_000), 131_072);
        assert_eq!(optimal_io_block_size_from_sectors(2_097_152), 2_097_152);
    }

    #[test]
    fn optimal_io_block_size_from_sectors_clamps_large_disks() {
        assert_eq!(optimal_io_block_size_from_sectors(100_000_000), 33_554_432);
        assert_eq!(optimal_io_block_size_from_sectors(500), 4_096);
    }

    #[test]
    fn whole_disk_path_strips_partitions() {
        assert_eq!(whole_disk_path("/dev/sdb1").unwrap(), "/dev/sdb");
        assert_eq!(whole_disk_path("/dev/nvme0n1p2").unwrap(), "/dev/nvme0n1");
        assert_eq!(whole_disk_path("/dev/mmcblk0p1").unwrap(), "/dev/mmcblk0");
        assert_eq!(whole_disk_path("/dev/sdb").unwrap(), "/dev/sdb");
    }

    #[test]
    fn busy_mounts_detects_partition_on_target_disk() {
        let proc_mounts = r#"
/dev/nvme0n1p2 / ext4 rw,relatime 0 0
/dev/sdb1 /mnt/usb vfat rw,relatime 0 0
/dev/sdb2 /media/backup ext4 rw,relatime 0 0
"#;
        let mounts = busy_mounts_from_lines(proc_mounts, "/dev/sdb").unwrap();
        assert_eq!(mounts.len(), 2);
        assert!(mounts.iter().any(|(s, m)| s == "/dev/sdb1" && m == "/mnt/usb"));
        assert!(mounts.iter().any(|(s, m)| s == "/dev/sdb2" && m == "/media/backup"));
    }

    #[test]
    fn busy_mounts_ignores_other_disks() {
        let proc_mounts = "/dev/nvme0n1p2 / ext4 rw,relatime 0 0\n";
        let mounts = busy_mounts_from_lines(proc_mounts, "/dev/sdb").unwrap();
        assert!(mounts.is_empty());
    }

    fn busy_mounts_from_lines(contents: &str, device_path: &str) -> Result<Vec<(String, String)>, String> {
        let target_whole = whole_disk_path(device_path)?;
        let mut mounts = Vec::new();
        for line in contents.lines() {
            let mut parts = line.split_whitespace();
            let Some(source) = parts.next() else {
                continue;
            };
            let Some(mount_point) = parts.next() else {
                continue;
            };
            if !source.starts_with("/dev/") {
                continue;
            }
            let source_whole = whole_disk_path_from_source(source)?;
            if source_whole == target_whole {
                mounts.push((source.to_string(), mount_point.to_string()));
            }
        }
        Ok(mounts)
    }
}

/// I/O buffer sizes (bytes) used by Lithographer legacy logic — pick based on device capacity.
const IO_BLOCK_SIZES: [usize; 14] = [
    4096, 8192, 16384, 32768, 65536, 131072, 262144, 524288, 1048576, 2097152, 4194304, 8388608,
    16777216, 33554432,
];

const DEFAULT_IO_BLOCK_SIZE: usize = IO_BLOCK_SIZES[0];

/// Pick I/O buffer size from device capacity (`DeviceInfo.size` — 512-byte sectors).
///
/// Matches the historical Lithographer `execute` logic: find the smallest table entry
/// that is >= `size_sectors`, clamped to the table range.
pub fn optimal_io_block_size_from_sectors(size_sectors: u64) -> usize {
    if size_sectors > IO_BLOCK_SIZES[13] as u64 {
        return IO_BLOCK_SIZES[13];
    }
    if size_sectors < IO_BLOCK_SIZES[0] as u64 {
        return IO_BLOCK_SIZES[0];
    }

    for &block in &IO_BLOCK_SIZES {
        if size_sectors <= block as u64 {
            return block;
        }
    }

    IO_BLOCK_SIZES[13]
}

/// Resolve block size for a device path via sysfs sector count.
pub fn optimal_io_block_size(device_path: &str) -> usize {
    device_size_sectors(device_path)
        .map(optimal_io_block_size_from_sectors)
        .unwrap_or(DEFAULT_IO_BLOCK_SIZE)
}

/// Returns device size in 512-byte sectors from `/sys/block/<name>/size`.
pub fn device_size_sectors(device_path: &str) -> Option<u64> {
    let block_name = Path::new(device_path)
        .file_name()
        .and_then(|n| n.to_str())?;
    let size_path = format!("/sys/block/{block_name}/size");
    fs::read_to_string(size_path)
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Returns device size in bytes from `/sys/block/<name>/size` (512-byte sectors).
pub fn device_size_bytes(device_path: &str) -> Option<u64> {
    device_size_sectors(device_path)
        .map(|sectors| sectors.saturating_mul(512))
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
    #[cfg(target_os = "windows")]
    {
        return crate::platform::windows_devices::get_storage_devices();
    }

    #[cfg(all(not(target_os = "linux"), not(target_os = "windows")))]
    {
        return Ok(Vec::new());
    }

    #[cfg(target_os = "linux")]
    {
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
}

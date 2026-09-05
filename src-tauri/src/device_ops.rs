//! Device operations via rockusb (chip/flash/storage/capability/boot/reset/erase).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Emitter, State};

use rockfile::boot::{
    RkBootEntry, RkBootEntryBytes, RkBootHeader, RkBootHeaderBytes, RkBootHeaderEntry,
};
#[cfg(not(target_os = "windows"))]
use rockusb::nusb::Device;
use rockusb::protocol::{Capability, ChipInfo, FlashId, FlashInfo, ResetOpcode, StorageIndex};
#[cfg(target_os = "windows")]
use rockusb::windows::Device;

use crate::devices;
#[cfg(not(target_os = "windows"))]
use crate::devices::{format_location_id_for, is_rockusb_device_info};
use crate::firmware::{
    build_gpt_tables, extract_firmware_for_upgrade, parse_gpt_parameter,
    parse_parameter_partitions, parse_sparse_chunk_header, parse_sparse_header, FirmwareImage,
    GptTables, SparseChunkKind, SparseHeader,
};
use crate::state::AppState;
use crate::upgrade_tool::{CurrentStorageInfo, DownloadExecutePayload, LogPayload};

const EVENT_TOOL_LOG: &str = "tool-log";

/// Match rkdeveloptool `EraseEmmc` / `erase_partition` chunk size (`1024 * 32` sectors).
const ERASE_LBA_CHUNK: u32 = 1024 * 32;

/// Match rkdeveloptool `MAX_ERASE_BLOCKS` for `RKU_EraseBlock`.
const ERASE_BLOCK_CHUNK: u16 = 16;

/// Match rkdeveloptool `DEFAULT_RW_LBA` / rockusb `MAXIO_SIZE` (128 sectors).
const READ_LBA_CHUNK: u32 = 128;

const SECTOR_SIZE: usize = 512;
const WRITE_LBA_CHUNK: usize = READ_LBA_CHUNK as usize * SECTOR_SIZE;
// Match upgrade_tool's 30-second RockUSB timeout while allowing for USB re-enumeration.
const LOADER_READY_RETRIES: usize = 120;
const LOADER_READY_INTERVAL: Duration = Duration::from_millis(250);
// Match rkdeveloptool `CRKDevice::DownloadBoot`, which waits after the final 0x472 upload.
const LOADER_BOOT_SETTLE_DELAY: Duration = Duration::from_secs(1);
const PARAMETER_START_SECTOR: u32 = 0x20;
const PARAMETER_READ_SECTORS: u32 = 128;
/// Firmware package parameter entries have a metadata offset of zero. Rockchip's
/// `PRM` command persists their PARM payload at this fixed flash address.
const FIRMWARE_PARAMETER_START_SECTOR: u64 = 0x2000;
const IDBLOCK_START_SECTOR: u32 = 0x40;
const IDBLOCK_ALIGNMENT: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq)]
struct DownloadPartition {
    name: String,
    start_sector: u64,
    sector_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LbaWriteChunk {
    start_sector: u32,
    source_len: usize,
    transfer_len: usize,
}

fn format_byte_count(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];

    let mut value = bytes as f64;
    let mut unit_index = 0;
    while value >= 1024.0 && unit_index < UNITS.len() - 1 {
        value /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        return format!("{bytes} B");
    }

    let value = format!("{value:.2}");
    let value = value.trim_end_matches('0').trim_end_matches('.');
    format!("{value} {}", UNITS[unit_index])
}

fn emit_log(app: &AppHandle, text: &str) {
    if text.is_empty() {
        return;
    }
    let lower = text.to_ascii_lowercase();
    let level = if lower.contains("success") || lower.contains("成功") {
        "success"
    } else if lower.contains("fail") || lower.contains("error") || lower.contains("失败") {
        "error"
    } else {
        "default"
    };
    let _ = app.emit(
        EVENT_TOOL_LOG,
        LogPayload {
            text: text.to_string(),
            level: level.to_string(),
            in_place: false,
        },
    );
}

fn emit_progress(app: &AppHandle, text: String) {
    let _ = app.emit(
        EVENT_TOOL_LOG,
        LogPayload {
            text,
            level: "default".to_string(),
            in_place: true,
        },
    );
}

fn should_emit_progress(last_percent: Option<u64>, percent: u64) -> bool {
    last_percent != Some(percent)
}

fn emit_lines(app: &AppHandle, text: &str) {
    for line in text.lines() {
        emit_log(app, line);
    }
}

/// Download / Advanced page storage name → 1-based UI SSD No.
pub fn storage_name_to_ui_no(name: &str) -> Result<u32, String> {
    match name.trim().to_ascii_uppercase().as_str() {
        "FLASH" | "NAND" => Ok(1),
        "EMMC" => Ok(2),
        "SD" | "SD0" => Ok(3),
        "SD1" => Ok(4),
        "SPINOR" => Ok(5),
        "SPINAND" => Ok(6),
        "RAM" => Ok(7),
        "USB" => Ok(8),
        "SATA" => Ok(9),
        "PCIE" => Ok(10),
        other if other.is_empty() => Err("Storage type is empty".to_string()),
        other => Err(format!("Unknown storage type: {other}")),
    }
}

/// UI / upgrade_tool 1-based SSD No. → protocol StorageIndex.
/// Matches Advanced / Download page numbering (SATA=9, PCIE=10).
/// SPI NOR/NAND 优先走 MTD 块设备入口（新 Loader 常见），与 GetStorageMedia 回报一致。
pub fn storage_from_ui_no(no: u32) -> Result<StorageIndex, String> {
    Ok(match no {
        1 => StorageIndex::Nand,
        2 => StorageIndex::Emmc,
        3 => StorageIndex::Sd0,
        4 => StorageIndex::Sd1,
        5 => StorageIndex::MtdBlkSpiNor,
        6 => StorageIndex::MtdBlkSpiNand,
        7 => StorageIndex::Ram,
        8 => StorageIndex::MtdBlkNand, // Advanced UI label: USB（无独立 USB 协议项）
        9 => StorageIndex::Sata,
        10 => StorageIndex::Pcie,
        other => {
            let idx = other
                .checked_sub(1)
                .ok_or_else(|| format!("Invalid storage index: {other}"))?;
            if idx > u8::MAX as u32 {
                return Err(format!("Invalid storage index: {other}"));
            }
            StorageIndex::from(idx as u8)
        }
    })
}

fn storage_to_ui(index: StorageIndex) -> CurrentStorageInfo {
    // Advanced 页列表只有 1..10：FLASH/EMMC/SD/SD1/SPINOR/SPINAND/RAM/USB/SATA/PCIE。
    // MTD_* 与同介质的非 MTD 入口折叠到同一 UI 项。
    let (no, name) = match index {
        StorageIndex::Nand => (1, "FLASH"),
        StorageIndex::MtdBlkNand => (8, "USB"),
        StorageIndex::Emmc => (2, "EMMC"),
        StorageIndex::Sd0 => (3, "SD"),
        StorageIndex::Sd1 => (4, "SD1"),
        StorageIndex::SpiNor | StorageIndex::MtdBlkSpiNor => (5, "SPINOR"),
        StorageIndex::SpiNand | StorageIndex::MtdBlkSpiNand => (6, "SPINAND"),
        StorageIndex::Ram => (7, "RAM"),
        StorageIndex::Sata => (9, "SATA"),
        StorageIndex::Pcie => (10, "PCIE"),
        StorageIndex::Ufs => {
            return CurrentStorageInfo {
                no: 0,
                name: "UFS".to_string(),
            };
        }
        StorageIndex::Unknown(v) => {
            return CurrentStorageInfo {
                no: 0,
                name: format!("UNKNOWN({v})"),
            };
        }
        other => {
            return CurrentStorageInfo {
                no: 0,
                name: format!("{other}").to_ascii_uppercase(),
            };
        }
    };
    CurrentStorageInfo {
        no,
        name: name.to_string(),
    }
}

fn format_chip_info(info: &ChipInfo) -> String {
    let bytes = info.inner();
    let hex: String = bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    format!("Chip Info: [{hex}]")
}

fn format_flash_id(id: &FlashId) -> String {
    format!("Flash ID: {}", id.to_str().trim())
}

fn format_flash_info(info: &FlashInfo) -> String {
    let mb = info.sectors() / 2048;
    format!(
        "Flash Info: size={} MB ({} sectors), block={} sectors, raw={:02x?}",
        mb,
        info.sectors(),
        info.block_size_sectors(),
        info.inner()
    )
}

fn format_capability(cap: &Capability) -> String {
    let mut lines = vec![format!("Capability raw: {:02x?}", cap.inner())];
    let mut flags = Vec::new();
    if cap.direct_lba() {
        flags.push("Direct LBA");
    }
    if cap.vendor_storage() {
        flags.push("Vendor storage");
    }
    if cap.first_4m_access() {
        flags.push("First 4M Access");
    }
    if cap.read_lba() {
        flags.push("Read LBA");
    }
    if cap.read_com_log() {
        flags.push("Read COM log");
    }
    if cap.read_idb_config() {
        flags.push("Read IDB config");
    }
    if cap.read_secure_mode() {
        flags.push("Read secure mode");
    }
    if cap.new_idb() {
        flags.push("New IDB");
    }
    if cap.switch_storage() {
        flags.push("Switch storage");
    }
    if flags.is_empty() {
        lines.push("Capability: (none)".to_string());
    } else {
        lines.push(format!("Capability: {}", flags.join(", ")));
    }
    lines.join("\n")
}

#[cfg(not(target_os = "windows"))]
async fn open_selected_device(state: &AppState) -> Result<Device, String> {
    let selected = state
        .selected_device
        .lock()
        .map_err(|e| e.to_string())?
        .clone();

    let infos: Vec<_> = rockusb::nusb::devices()
        .await
        .map_err(|e| format!("Failed to list USB devices: {e}"))?
        .filter(is_rockusb_device_info)
        .collect();

    if infos.is_empty() {
        return Err("No Rockchip device found (enter Maskrom/Loader)".to_string());
    }

    let info = if let Some(location_id) = selected.as_deref().filter(|s| !s.is_empty()) {
        infos
            .into_iter()
            .find(|d| format_location_id_for(d) == location_id)
            .ok_or_else(|| format!("Selected device {location_id} is no longer connected"))?
    } else if infos.len() == 1 {
        infos.into_iter().next().unwrap()
    } else {
        return Err("Multiple devices connected; select one in the status bar".to_string());
    };

    Device::from_usb_device_info(info)
        .await
        .map_err(|e| format!("Failed to open RockUSB device: {e}"))
}

#[cfg(target_os = "windows")]
async fn selected_driver_device_with_mode(
    state: &AppState,
    required_mode: Option<rockusb::windows::DeviceMode>,
) -> Result<rockusb::windows::DeviceInfo, String> {
    let selected = state
        .selected_device
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let mut infos = rockusb::windows::devices_with_modes()
        .await
        .map_err(|e| format!("Failed to list installed Rockusb driver interfaces: {e}"))?;

    if let Some(mode) = required_mode {
        infos.retain(|info| info.mode == mode);
    }

    if infos.is_empty() {
        return Err(match required_mode {
            Some(rockusb::windows::DeviceMode::Loader) => {
                "No Rockchip device in Loader mode with the official Rockusb driver found"
                    .to_string()
            }
            Some(rockusb::windows::DeviceMode::Maskrom) => {
                "No Rockchip device in Maskrom mode with the official Rockusb driver found"
                    .to_string()
            }
            None => {
                "No Rockchip device with the official Rockusb driver found (enter Maskrom/Loader)"
                    .to_string()
            }
        });
    }

    let info = match selected.as_deref().filter(|path| !path.is_empty()) {
        Some(path) => infos
            .iter()
            .find(|info| info.interface_path == path)
            .cloned()
            // A Maskrom boot download causes Windows to publish a new interface path.  When it
            // is the sole device, safely follow that transition instead of retaining a stale path.
            .or_else(|| (infos.len() == 1).then(|| infos[0].clone()))
            .ok_or_else(|| format!("Selected device {path} is no longer connected"))?,
        None if infos.len() == 1 => infos[0].clone(),
        None => return Err("Multiple devices connected; select one in the status bar".to_string()),
    };

    if selected.as_deref() != Some(info.interface_path.as_str()) {
        *state.selected_device.lock().map_err(|e| e.to_string())? =
            Some(info.interface_path.clone());
    }
    Ok(info)
}

#[cfg(target_os = "windows")]
async fn selected_driver_device(state: &AppState) -> Result<rockusb::windows::DeviceInfo, String> {
    selected_driver_device_with_mode(state, None).await
}

#[cfg(target_os = "windows")]
async fn open_selected_device(state: &AppState) -> Result<Device, String> {
    let info = selected_driver_device(state).await?;
    Device::from_interface_path(&info.interface_path)
        .map_err(|e| format!("Failed to open installed Rockusb driver: {e}"))
}

#[cfg(target_os = "windows")]
async fn open_loader_device(state: &AppState) -> Result<Device, String> {
    let info =
        selected_driver_device_with_mode(state, Some(rockusb::windows::DeviceMode::Loader)).await?;
    Device::from_interface_path(&info.interface_path)
        .map_err(|e| format!("Failed to open installed Rockusb Loader driver: {e}"))
}

#[cfg(not(target_os = "windows"))]
async fn open_loader_device(state: &AppState) -> Result<Device, String> {
    open_selected_device(state).await
}

#[cfg(not(target_os = "windows"))]
async fn selected_device_is_maskrom(state: &AppState) -> Result<bool, String> {
    let selected = state
        .selected_device
        .lock()
        .map_err(|e| e.to_string())?
        .clone();
    let infos: Vec<_> = rockusb::nusb::devices()
        .await
        .map_err(|e| format!("Failed to list USB devices: {e}"))?
        .filter(is_rockusb_device_info)
        .collect();

    let info = if let Some(location_id) = selected.as_deref().filter(|s| !s.is_empty()) {
        infos
            .into_iter()
            .find(|d| format_location_id_for(d) == location_id)
            .ok_or_else(|| format!("Selected device {location_id} is no longer connected"))?
    } else if infos.len() == 1 {
        infos.into_iter().next().unwrap()
    } else if infos.is_empty() {
        return Err("No Rockchip device found (enter Maskrom/Loader)".to_string());
    } else {
        return Err("Multiple devices connected; select one in the status bar".to_string());
    };

    Ok(info.usb_version() & 1 == 0)
}

#[cfg(target_os = "windows")]
async fn selected_device_is_maskrom(state: &AppState) -> Result<bool, String> {
    Ok(selected_driver_device(state).await?.mode == rockusb::windows::DeviceMode::Maskrom)
}

async fn with_busy_device<F, Fut, T>(
    app: &AppHandle,
    state: &State<'_, AppState>,
    op_name: &str,
    f: F,
) -> Result<T, String>
where
    F: FnOnce(Device) -> Fut,
    Fut: std::future::Future<Output = Result<(T, String), String>>,
{
    devices::ensure_backend_not_busy(state.inner())?;
    devices::set_backend_busy(state.inner(), true)?;

    let result = async {
        emit_log(app, &format!("> rockusb {op_name}"));
        let device = open_selected_device(state.inner()).await?;
        let (value, output) = f(device).await?;
        emit_lines(app, &output);
        Ok(value)
    }
    .await;

    let _ = devices::set_backend_busy(state.inner(), false);
    result
}

#[tauri::command]
pub async fn read_chip_info(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    with_busy_device(&app, &state, "chip-info", |mut device| async move {
        let info = device
            .chip_info()
            .await
            .map_err(|e| format!("Read chip info failed: {e}"))?;
        let output = format_chip_info(&info);
        Ok((output.clone(), output))
    })
    .await
}

#[tauri::command]
pub async fn get_current_storage(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<CurrentStorageInfo, String> {
    with_busy_device(&app, &state, "storage", |mut device| async move {
        let index = device
            .storage()
            .await
            .map_err(|e| format!("Get current storage failed: {e}"))?;
        let info = storage_to_ui(index);
        let output = format!("Current storage: No={} {} (*)", info.no, info.name);
        Ok((info, output))
    })
    .await
}

pub async fn switch_storage(
    app: AppHandle,
    state: State<'_, AppState>,
    ui_no: u32,
) -> Result<String, String> {
    let target = storage_from_ui_no(ui_no)?;
    let label = storage_to_ui(target).name;
    with_busy_device(
        &app,
        &state,
        &format!("switch-storage {ui_no} ({label})"),
        move |mut device| async move {
            device
                .switch_storage(target)
                .await
                .map_err(|e| format!("Switch storage failed: {e}"))?;
            // Protocol has no return code; re-query like rockusb example.
            let current = device
                .storage()
                .await
                .map_err(|e| format!("Switch storage verify failed: {e}"))?;
            let info = storage_to_ui(current);
            if current == target {
                let output = format!("Switched storage to {} (No={})", info.name, info.no);
                Ok((output.clone(), output))
            } else {
                Err(format!(
                    "Failed to switch storage to {label}; current is {} (No={})",
                    info.name, info.no
                ))
            }
        },
    )
    .await
}

pub async fn read_flash_id(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    with_busy_device(&app, &state, "flash-id", |mut device| async move {
        let id = device
            .flash_id()
            .await
            .map_err(|e| format!("Read Flash ID failed: {e}"))?;
        let output = format_flash_id(&id);
        Ok((output.clone(), output))
    })
    .await
}

pub async fn read_flash_info(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    with_busy_device(&app, &state, "flash-info", |mut device| async move {
        let info = device
            .flash_info()
            .await
            .map_err(|e| format!("Read Flash info failed: {e}"))?;
        let output = format_flash_info(&info);
        Ok((output.clone(), output))
    })
    .await
}

pub async fn read_capability(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    with_busy_device(&app, &state, "capability", |mut device| async move {
        let cap = device
            .capability()
            .await
            .map_err(|e| format!("Read capability failed: {e}"))?;
        let output = format_capability(&cap);
        Ok((output.clone(), output))
    })
    .await
}

async fn download_boot_entry(
    device: &mut Device,
    header: RkBootHeaderEntry,
    code: u16,
    file: &mut File,
    log: &mut String,
) -> Result<(), String> {
    for i in 0..header.count {
        let mut entry_bytes: RkBootEntryBytes = [0; 57];
        file.seek(SeekFrom::Start(
            u64::from(header.offset) + u64::from(header.size) * u64::from(i),
        ))
        .map_err(|e| format!("Seek boot entry failed: {e}"))?;
        file.read_exact(&mut entry_bytes)
            .map_err(|e| format!("Read boot entry failed: {e}"))?;

        let entry = RkBootEntry::from_bytes(&entry_bytes);
        let name = String::from_utf16(entry.name.as_slice())
            .unwrap_or_else(|_| format!("entry-{i}"))
            .trim_end_matches('\0')
            .to_string();
        log.push_str(&format!("Downloading 0x{code:x} #{i} ({name})...\n"));

        let mut data = vec![0u8; entry.data_size as usize];
        file.seek(SeekFrom::Start(u64::from(entry.data_offset)))
            .map_err(|e| format!("Seek boot data failed: {e}"))?;
        file.read_exact(&mut data)
            .map_err(|e| format!("Read boot data failed: {e}"))?;

        device
            .write_maskrom_area(code, &data)
            .await
            .map_err(|e| format!("Write maskrom area 0x{code:x} failed: {e}"))?;

        if entry.data_delay > 0 {
            thread::sleep(Duration::from_millis(u64::from(entry.data_delay)));
        }
    }
    Ok(())
}

async fn download_boot_to_device(device: &mut Device, boot_path: &Path) -> Result<String, String> {
    let mut file = File::open(boot_path).map_err(|e| format!("Open boot file failed: {e}"))?;
    let mut header_bytes: RkBootHeaderBytes = [0; 102];
    file.read_exact(&mut header_bytes)
        .map_err(|e| format!("Read boot header failed: {e}"))?;
    let header = RkBootHeader::from_bytes(&header_bytes).ok_or_else(|| {
        "Failed to parse Loader/Boot header (use MiniLoaderAll.bin or download.bin)".to_string()
    })?;

    let mut log = String::from("Download Boot Start\n");
    download_boot_entry(device, header.entry_471, 0x471, &mut file, &mut log).await?;
    download_boot_entry(device, header.entry_472, 0x472, &mut file, &mut log).await?;
    // The temporary USB Loader takes over the controller asynchronously after 0x472.
    // Do not probe the departing Maskrom interface during this handoff.
    thread::sleep(LOADER_BOOT_SETTLE_DELAY);
    log.push_str("Download Boot Success");
    Ok(log)
}

fn parse_download_lba(value: &str) -> Result<u64, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err("Address is required for force-by-address write".to_string());
    }

    let parsed = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .map(|hex| u64::from_str_radix(hex, 16))
        .unwrap_or_else(|| value.parse());
    parsed.map_err(|_| format!("Invalid LBA address: {value}"))
}

fn resolve_download_partition(
    partitions: &[DownloadPartition],
    name: &str,
) -> Result<DownloadPartition, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Partition name is required unless force-by-address is enabled".to_string());
    }

    if name.eq_ignore_ascii_case("parameter") {
        return Ok(DownloadPartition {
            name: name.to_string(),
            start_sector: u64::from(PARAMETER_START_SECTOR),
            sector_count: None,
        });
    }

    partitions
        .iter()
        .find(|partition| partition.name.eq_ignore_ascii_case(name))
        .cloned()
        .ok_or_else(|| format!("Partition {name} was not found on the selected storage"))
}

fn gpt_entry_layout(header: &[u8]) -> Result<Option<(u64, u32, u32)>, String> {
    if header.len() < SECTOR_SIZE || &header[..8] != b"EFI PART" {
        return Ok(None);
    }

    let header_size = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;
    if !(92..=SECTOR_SIZE).contains(&header_size) {
        return Err("Invalid GPT header size".to_string());
    }
    let entries_lba = u64::from_le_bytes(header[72..80].try_into().unwrap());
    let entry_count = u32::from_le_bytes(header[80..84].try_into().unwrap());
    let entry_size = u32::from_le_bytes(header[84..88].try_into().unwrap());
    if entry_count == 0 || entry_count > 4096 || !(128..=4096).contains(&entry_size) {
        return Err("Unsupported GPT partition entry layout".to_string());
    }
    Ok(Some((entries_lba, entry_count, entry_size)))
}

fn parse_gpt_download_partitions(
    header: &[u8],
    entries: &[u8],
) -> Result<Option<Vec<DownloadPartition>>, String> {
    let Some((_, entry_count, entry_size)) = gpt_entry_layout(header)? else {
        return Ok(None);
    };
    let total_bytes = usize::try_from(u64::from(entry_count) * u64::from(entry_size))
        .map_err(|_| "GPT partition table is too large".to_string())?;
    if entries.len() < total_bytes {
        return Err("GPT partition table is incomplete".to_string());
    }

    let entry_size = entry_size as usize;
    let mut partitions = Vec::new();
    for entry in entries[..total_bytes].chunks_exact(entry_size) {
        if entry[..16].iter().all(|byte| *byte == 0) {
            continue;
        }
        let start_sector = u64::from_le_bytes(entry[32..40].try_into().unwrap());
        let end_sector = u64::from_le_bytes(entry[40..48].try_into().unwrap());
        if end_sector < start_sector {
            return Err("GPT partition has an invalid sector range".to_string());
        }

        let mut name_words = Vec::new();
        for bytes in entry[56..].chunks_exact(2) {
            let word = u16::from_le_bytes(bytes.try_into().unwrap());
            if word == 0 {
                break;
            }
            name_words.push(word);
        }
        let name = String::from_utf16(&name_words)
            .map_err(|_| "GPT partition name is not valid UTF-16".to_string())?;
        if name.trim().is_empty() {
            continue;
        }
        partitions.push(DownloadPartition {
            name,
            start_sector,
            sector_count: Some(
                end_sector
                    .checked_sub(start_sector)
                    .and_then(|count| count.checked_add(1))
                    .ok_or_else(|| "GPT partition size overflows".to_string())?,
            ),
        });
    }
    Ok(Some(partitions))
}

async fn read_lba_sectors(
    device: &mut Device,
    start_sector: u32,
    sector_count: u32,
) -> Result<Vec<u8>, String> {
    let byte_count = usize::try_from(u64::from(sector_count) * SECTOR_SIZE as u64)
        .map_err(|_| "LBA read is too large".to_string())?;
    let mut data = vec![0u8; byte_count];
    let mut done = 0u32;
    while done < sector_count {
        let count = (sector_count - done).min(READ_LBA_CHUNK);
        let offset = usize::try_from(u64::from(done) * SECTOR_SIZE as u64)
            .map_err(|_| "LBA read offset overflows".to_string())?;
        let len = count as usize * SECTOR_SIZE;
        let sector = start_sector
            .checked_add(done)
            .ok_or_else(|| "LBA read address overflows".to_string())?;
        let transferred = device
            .read_lba(sector, &mut data[offset..offset + len])
            .await
            .map_err(|e| format!("Read LBA {sector} failed: {e}"))?;
        if transferred as usize != len {
            return Err(format!(
                "Short LBA read at {sector}: got {transferred} bytes, expected {len}"
            ));
        }
        done += count;
    }
    Ok(data)
}

async fn read_download_partitions(device: &mut Device) -> Result<Vec<DownloadPartition>, String> {
    let header = read_lba_sectors(device, 1, 1).await?;
    if let Some((entries_lba, entry_count, entry_size)) = gpt_entry_layout(&header)? {
        let entry_bytes = u64::from(entry_count) * u64::from(entry_size);
        let entry_sectors = entry_bytes.div_ceil(SECTOR_SIZE as u64);
        let entry_start = u32::try_from(entries_lba)
            .map_err(|_| "GPT partition table address exceeds RockUSB LBA range".to_string())?;
        let entry_sectors = u32::try_from(entry_sectors)
            .map_err(|_| "GPT partition table is too large".to_string())?;
        let entries = read_lba_sectors(device, entry_start, entry_sectors).await?;
        return parse_gpt_download_partitions(&header, &entries)?
            .ok_or_else(|| "GPT partition table is missing".to_string());
    }

    let parameter =
        read_lba_sectors(device, PARAMETER_START_SECTOR, PARAMETER_READ_SECTORS).await?;
    let partitions = parse_parameter_partitions(&parameter)?
        .ok_or_else(|| "No GPT or Rockchip parameter partition table found".to_string())?;
    Ok(partitions
        .into_iter()
        .map(|partition| DownloadPartition {
            name: partition.name,
            start_sector: partition.start_sector,
            sector_count: partition.sector_count,
        })
        .collect())
}

fn image_for_download(
    name: &str,
    path: &str,
    partition: DownloadPartition,
) -> Result<FirmwareImage, String> {
    let path = PathBuf::from(path);
    let byte_count = std::fs::metadata(&path)
        .map_err(|e| format!("Read image metadata failed for {}: {e}", path.display()))?
        .len();
    if byte_count == 0 {
        return Err(format!("Image {} is empty", path.display()));
    }
    Ok(FirmwareImage {
        name: name.trim().to_string(),
        path,
        flash_offset_sectors: partition.start_sector,
        flash_size_sectors: partition.sector_count.unwrap_or(0),
        byte_count,
    })
}

fn loader_entry_data(
    bytes: &[u8],
    entries: RkBootHeaderEntry,
    name: &str,
) -> Result<Option<Vec<u8>>, String> {
    if entries.size < 57 {
        return Err("Loader entry table is smaller than a boot entry".to_string());
    }

    for index in 0..entries.count {
        let entry_offset = u64::from(entries.offset)
            .checked_add(u64::from(entries.size) * u64::from(index))
            .ok_or_else(|| "Loader entry table offset overflows".to_string())?;
        let entry_offset = usize::try_from(entry_offset)
            .map_err(|_| "Loader entry table offset is too large".to_string())?;
        let entry_end = entry_offset
            .checked_add(57)
            .ok_or_else(|| "Loader entry table range overflows".to_string())?;
        let entry_bytes = bytes
            .get(entry_offset..entry_end)
            .ok_or_else(|| "Loader entry table is truncated".to_string())?;
        let entry = RkBootEntry::from_bytes(entry_bytes.try_into().unwrap());
        let entry_name = String::from_utf16_lossy(&entry.name)
            .trim_end_matches('\0')
            .to_string();
        if !entry_name.eq_ignore_ascii_case(name) {
            continue;
        }

        let start = entry.data_offset as usize;
        let end = start
            .checked_add(entry.data_size as usize)
            .ok_or_else(|| format!("Loader entry {name} range overflows"))?;
        return bytes
            .get(start..end)
            .map(|data| Some(data.to_vec()))
            .ok_or_else(|| format!("Loader entry {name} is truncated"));
    }
    Ok(None)
}

fn align_idblock_bytes(byte_count: usize) -> Result<usize, String> {
    byte_count
        .checked_add(IDBLOCK_ALIGNMENT - 1)
        .map(|size| size & !(IDBLOCK_ALIGNMENT - 1))
        .ok_or_else(|| "Loader IDBlock is too large".to_string())
}

fn rc4_crypt(data: &mut [u8]) {
    const KEY: [u8; 16] = [124, 78, 3, 4, 85, 5, 9, 7, 45, 44, 123, 56, 23, 13, 23, 17];
    let mut state = [0u8; 256];
    for (index, value) in state.iter_mut().enumerate() {
        *value = index as u8;
    }
    let mut j = 0usize;
    for index in 0..256 {
        j = (j + usize::from(state[index]) + usize::from(KEY[index & 0x0f])) & 0xff;
        state.swap(index, j);
    }
    let mut i = 0usize;
    j = 0;
    for value in data {
        i = (i + 1) & 0xff;
        j = (j + usize::from(state[i])) & 0xff;
        state.swap(i, j);
        *value ^= state[(usize::from(state[i]) + usize::from(state[j])) & 0xff];
    }
}

fn rc4_full_sectors(data: &mut [u8]) {
    for sector in data.chunks_exact_mut(SECTOR_SIZE) {
        rc4_crypt(sector);
    }
}

fn crc16_ccitt(data: &[u8]) -> u16 {
    let mut crc = 0u16;
    for byte in data {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn rockchip_crc32(data: &[u8]) -> u32 {
    let mut crc = 0u32;
    for byte in data {
        crc ^= u32::from(*byte) << 24;
        for _ in 0..8 {
            crc = if crc & 0x8000_0000 != 0 {
                (crc << 1) ^ 0x04c1_1db7
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn new_idblock(
    mut head: Vec<u8>,
    mut boost: Option<Vec<u8>>,
    mut data: Vec<u8>,
    mut boot: Vec<u8>,
    rc4_enabled: bool,
) -> Result<Vec<u8>, String> {
    if rc4_enabled {
        rc4_full_sectors(&mut head);
        if let Some(boost) = boost.as_deref_mut() {
            rc4_full_sectors(boost);
        }
        rc4_full_sectors(&mut data);
        rc4_full_sectors(&mut boot);
    }
    let head_size = align_idblock_bytes(head.len())?;
    let boost_size = boost
        .as_ref()
        .map(|boost| align_idblock_bytes(boost.len()))
        .transpose()?
        .unwrap_or(0);
    let data_size = align_idblock_bytes(data.len())?;
    let boot_size = align_idblock_bytes(boot.len())?;
    let total = head_size
        .checked_add(boost_size)
        .and_then(|size| size.checked_add(data_size))
        .and_then(|size| size.checked_add(boot_size))
        .ok_or_else(|| "Loader IDBlock is too large".to_string())?;
    let mut idblock = vec![0u8; total];
    idblock[..head.len()].copy_from_slice(&head);
    let data_offset = head_size + boost_size;
    if let Some(boost) = boost {
        idblock[head_size..head_size + boost.len()].copy_from_slice(&boost);
    }
    idblock[data_offset..data_offset + data.len()].copy_from_slice(&data);
    let boot_offset = data_offset + data_size;
    idblock[boot_offset..boot_offset + boot.len()].copy_from_slice(&boot);
    Ok(idblock)
}

fn legacy_idblock(mut data: Vec<u8>, mut boot: Vec<u8>, rc4_enabled: bool) -> Result<Vec<u8>, String> {
    let data_sectors = align_idblock_bytes(data.len())? / SECTOR_SIZE;
    let boot_sectors = align_idblock_bytes(boot.len())? / SECTOR_SIZE;
    let data_sectors = u16::try_from(data_sectors)
        .map_err(|_| "Loader data is too large for a legacy IDBlock".to_string())?;
    let boot_sectors = u16::try_from(boot_sectors)
        .map_err(|_| "Loader code is too large for a legacy IDBlock".to_string())?;
    let code_sectors = data_sectors
        .checked_add(boot_sectors)
        .ok_or_else(|| "Legacy IDBlock sector count overflows".to_string())?;
    if rc4_enabled {
        rc4_full_sectors(&mut data);
        rc4_full_sectors(&mut boot);
    }

    let mut idblock = vec![0u8; (4 + usize::from(code_sectors)) * SECTOR_SIZE];
    let mut sector0 = [0u8; SECTOR_SIZE];
    sector0[..4].copy_from_slice(&0x0ff0_aa55u32.to_le_bytes());
    sector0[8..12].copy_from_slice(&(rc4_enabled as u32).to_le_bytes());
    sector0[12..14].copy_from_slice(&4u16.to_le_bytes());
    sector0[14..16].copy_from_slice(&4u16.to_le_bytes());
    sector0[506..508].copy_from_slice(&data_sectors.to_le_bytes());
    sector0[508..510].copy_from_slice(&code_sectors.to_le_bytes());

    let mut sector1 = [0u8; SECTOR_SIZE];
    sector1[..2].copy_from_slice(&0x000cu16.to_le_bytes());
    sector1[2..4].copy_from_slice(&0xffffu16.to_le_bytes());
    sector1[10..14].copy_from_slice(&0x3832_4b52u32.to_le_bytes());

    let mut sector2 = [0u8; SECTOR_SIZE];
    sector2[491..494].copy_from_slice(b"VC\0");
    sector2[494..496].copy_from_slice(&crc16_ccitt(&sector0).to_le_bytes());
    sector2[496..498].copy_from_slice(&crc16_ccitt(&sector1).to_le_bytes());
    sector2[506..510].copy_from_slice(b"CRC\0");

    idblock[..SECTOR_SIZE].copy_from_slice(&sector0);
    idblock[SECTOR_SIZE..SECTOR_SIZE * 2].copy_from_slice(&sector1);
    idblock[SECTOR_SIZE * 4..SECTOR_SIZE * 4 + data.len()].copy_from_slice(&data);
    let boot_start = SECTOR_SIZE * (4 + usize::from(data_sectors));
    idblock[boot_start..boot_start + boot.len()].copy_from_slice(&boot);
    sector2[498..502].copy_from_slice(&rockchip_crc32(&idblock[SECTOR_SIZE * 4..]).to_le_bytes());
    sector2[510..512].copy_from_slice(&crc16_ccitt(&idblock[SECTOR_SIZE * 3..SECTOR_SIZE * 4]).to_le_bytes());
    idblock[SECTOR_SIZE * 2..SECTOR_SIZE * 3].copy_from_slice(&sector2);
    rc4_crypt(&mut idblock[..SECTOR_SIZE]);
    rc4_crypt(&mut idblock[SECTOR_SIZE * 2..SECTOR_SIZE * 3]);
    rc4_crypt(&mut idblock[SECTOR_SIZE * 3..SECTOR_SIZE * 4]);
    Ok(idblock)
}

fn build_loader_idblock(loader_path: &Path, capability: &Capability) -> Result<(Vec<u8>, &'static str), String> {
    let bytes = std::fs::read(loader_path)
        .map_err(|e| format!("Read Loader {} failed: {e}", loader_path.display()))?;
    let header_bytes: RkBootHeaderBytes = bytes
        .get(..102)
        .ok_or_else(|| "Loader is too small for a boot header".to_string())?
        .try_into()
        .unwrap();
    let header = RkBootHeader::from_bytes(&header_bytes)
        .ok_or_else(|| "Failed to parse Loader/Boot header".to_string())?;
    let flash_boot = loader_entry_data(&bytes, header.entry_loader.clone(), "FlashBoot")?
        .ok_or_else(|| "Loader has no FlashBoot entry".to_string())?;
    let flash_data = loader_entry_data(&bytes, header.entry_loader.clone(), "FlashData")?
        .ok_or_else(|| "Loader has no FlashData entry".to_string())?;
    let flash_head = loader_entry_data(&bytes, header.entry_loader.clone(), "FlashHead")?;
    let rc4_enabled = header.rc4_flag != 0;

    match flash_head {
        Some(head) => {
            if !capability.new_idb() {
                return Err("Loader requires New IDBlock support, but the device does not provide it".to_string());
            }
            let flash_boost = loader_entry_data(&bytes, header.entry_loader, "FlashBoost")?;
            let layout = if flash_boost.is_some() {
                "New IDBlock + FlashBoost"
            } else {
                "New IDBlock"
            };
            Ok((
                new_idblock(head, flash_boost, flash_data, flash_boot, rc4_enabled)?,
                layout,
            ))
        }
        None => Ok((legacy_idblock(flash_data, flash_boot, rc4_enabled)?, "legacy IDBlock")),
    }
}

async fn write_loader_idblock(
    app: &AppHandle,
    device: &mut Device,
    loader_path: &Path,
    flash_sectors: u32,
) -> Result<(), String> {
    let capability = device
        .capability()
        .await
        .map_err(|e| format!("Read Loader capability failed: {e}"))?;
    let (idblock, layout) = build_loader_idblock(loader_path, &capability)?;
    let sectors = u32::try_from(idblock.len() / SECTOR_SIZE)
        .map_err(|_| "Loader IDBlock is too large".to_string())?;
    let end_sector = IDBLOCK_START_SECTOR
        .checked_add(sectors)
        .ok_or_else(|| "Loader IDBlock range overflows".to_string())?;
    if end_sector > flash_sectors {
        return Err("Loader IDBlock exceeds the target flash size".to_string());
    }
    emit_log(
        app,
        &format!(
            "Writing Loader ({layout}): LBA 0x{IDBLOCK_START_SECTOR:08x}-0x{:08x} ({})",
            end_sector.saturating_sub(1),
            format_byte_count(idblock.len() as u64),
        ),
    );
    let mut written = 0usize;
    let mut last_progress_percent = None;
    for chunk in idblock.chunks(WRITE_LBA_CHUNK) {
        let sector = IDBLOCK_START_SECTOR
            .checked_add(u32::try_from(written / SECTOR_SIZE).unwrap())
            .ok_or_else(|| "Loader IDBlock address overflows".to_string())?;
        let transferred = device
            .write_lba(sector, chunk)
            .await
            .map_err(|e| format!("Write Loader IDBlock at LBA {sector} failed: {e}"))?;
        if transferred as usize != chunk.len() {
            return Err(format!(
                "Short Loader IDBlock write at LBA {sector}: got {transferred} bytes, expected {}",
                chunk.len()
            ));
        }
        written += chunk.len();
        let percent = (written as u64 * 100) / idblock.len() as u64;
        if should_emit_progress(last_progress_percent, percent) {
            emit_progress(
                app,
                format!(
                    "Writing Loader ({layout})... {}/{} ({percent}%)",
                    format_byte_count(written as u64),
                    format_byte_count(idblock.len() as u64),
                ),
            );
            last_progress_percent = Some(percent);
        }
    }
    emit_log(app, &format!("Written Loader ({layout}) successfully"));
    Ok(())
}

fn write_lba_chunk_plan(
    mut start_sector: u64,
    byte_count: u64,
) -> Result<Vec<LbaWriteChunk>, String> {
    if byte_count == 0 {
        return Err("Firmware image is empty".to_string());
    }

    let last_sector = start_sector
        .checked_add(byte_count.div_ceil(SECTOR_SIZE as u64).saturating_sub(1))
        .ok_or_else(|| "Firmware image sector range overflows".to_string())?;
    if last_sector > u64::from(u32::MAX) {
        return Err("Firmware image exceeds the RockUSB LBA address range".to_string());
    }

    let mut remaining = usize::try_from(byte_count)
        .map_err(|_| "Firmware image is too large for this platform".to_string())?;
    let mut chunks = Vec::new();
    while remaining > 0 {
        let source_len = remaining.min(WRITE_LBA_CHUNK);
        let transfer_len = source_len.next_multiple_of(SECTOR_SIZE);
        chunks.push(LbaWriteChunk {
            start_sector: start_sector as u32,
            source_len,
            transfer_len,
        });
        start_sector += (transfer_len / SECTOR_SIZE) as u64;
        remaining -= source_len;
    }
    Ok(chunks)
}

async fn write_firmware_image(
    app: &AppHandle,
    device: &mut Device,
    image: &FirmwareImage,
    flash_sectors: u32,
) -> Result<String, String> {
    if let Some((input, sparse)) = open_android_sparse_image(&image.path)? {
        return write_sparse_firmware_image(app, device, image, input, sparse, flash_sectors).await;
    }

    let actual_size = std::fs::metadata(&image.path)
        .map_err(|e| format!("Read extracted image metadata failed: {e}"))?
        .len();
    if actual_size != image.byte_count {
        return Err(format!(
            "Extracted image size changed for {}: expected {}, got {}",
            image.name, image.byte_count, actual_size
        ));
    }

    let chunks = write_lba_chunk_plan(image.flash_offset_sectors, actual_size)?;
    let total_transfer = chunks
        .iter()
        .map(|chunk| chunk.transfer_len as u64)
        .sum::<u64>();
    let total_sectors = total_transfer / SECTOR_SIZE as u64;
    if image.flash_size_sectors > 0 && total_sectors > image.flash_size_sectors {
        return Err(format!(
            "Image {} ({total_sectors} sectors) exceeds its firmware partition ({} sectors)",
            image.name, image.flash_size_sectors
        ));
    }

    let end_sector = image
        .flash_offset_sectors
        .checked_add(total_sectors)
        .ok_or_else(|| "Firmware image range overflows".to_string())?;
    if end_sector > u64::from(flash_sectors) {
        return Err(format!(
            "Image {} LBA range 0x{:x}-0x{:x} exceeds flash size 0x{flash_sectors:x}",
            image.name,
            image.flash_offset_sectors,
            end_sector.saturating_sub(1)
        ));
    }

    emit_log(
        app,
        &format!(
            "Writing {}: LBA 0x{:08x}-0x{:08x} ({})",
            image.name,
            image.flash_offset_sectors,
            end_sector.saturating_sub(1),
            format_byte_count(actual_size),
        ),
    );

    let mut input =
        File::open(&image.path).map_err(|e| format!("Open extracted image failed: {e}"))?;
    let mut written = 0u64;
    let mut last_progress_percent = None;
    for chunk in &chunks {
        let mut data = vec![0u8; chunk.transfer_len];
        input
            .read_exact(&mut data[..chunk.source_len])
            .map_err(|e| format!("Read extracted image failed: {e}"))?;
        let transferred = device
            .write_lba(chunk.start_sector, &data)
            .await
            .map_err(|e| {
                format!(
                    "Write {} at LBA {} failed: {e}",
                    image.name, chunk.start_sector
                )
            })?;
        if transferred as usize != chunk.transfer_len {
            return Err(format!(
                "Short write for {} at LBA {}: got {transferred} bytes, expected {}",
                image.name, chunk.start_sector, chunk.transfer_len
            ));
        }

        written += chunk.source_len as u64;
        let percent = (written * 100) / actual_size;
        if should_emit_progress(last_progress_percent, percent) {
            emit_progress(
                app,
                format!(
                    "Writing {}... {}/{} ({percent}%)",
                    image.name,
                    format_byte_count(written),
                    format_byte_count(actual_size),
                ),
            );
            last_progress_percent = Some(percent);
        }
    }

    Ok(format!("Written {} successfully", image.name))
}

fn open_android_sparse_image(path: &Path) -> Result<Option<(File, SparseHeader)>, String> {
    let mut input = File::open(path).map_err(|e| format!("Open extracted image failed: {e}"))?;
    if input
        .metadata()
        .map_err(|e| format!("Read extracted image metadata failed: {e}"))?
        .len()
        < 4
    {
        return Ok(None);
    }
    let mut first_header = [0u8; 28];
    input
        .read_exact(&mut first_header[..4])
        .map_err(|e| format!("Read extracted image failed: {e}"))?;
    if u32::from_le_bytes(first_header[..4].try_into().unwrap()) != 0xed26_ff3a {
        return Ok(None);
    }
    input
        .read_exact(&mut first_header[4..])
        .map_err(|e| format!("Read Android sparse header failed: {e}"))?;
    let sparse = parse_sparse_header(&first_header)?
        .ok_or_else(|| "Android sparse signature disappeared while reading header".to_string())?;

    if sparse.file_header_size > first_header.len() {
        let mut extra_header = vec![0u8; sparse.file_header_size - first_header.len()];
        input
            .read_exact(&mut extra_header)
            .map_err(|e| format!("Read Android sparse header failed: {e}"))?;
    }
    Ok(Some((input, sparse)))
}

async fn write_sparse_firmware_image(
    app: &AppHandle,
    device: &mut Device,
    image: &FirmwareImage,
    mut input: File,
    sparse: SparseHeader,
    flash_sectors: u32,
) -> Result<String, String> {
    let output_sectors = sparse.output_bytes / SECTOR_SIZE as u64;
    let total_sectors = validate_firmware_range(image, output_sectors, flash_sectors)?;
    let end_sector = image
        .flash_offset_sectors
        .checked_add(total_sectors)
        .ok_or_else(|| "Firmware image range overflows".to_string())?;
    emit_log(
        app,
        &format!(
            "Writing {} (Android sparse): LBA 0x{:08x}-0x{:08x} ({})",
            image.name,
            image.flash_offset_sectors,
            end_sector.saturating_sub(1),
            format_byte_count(sparse.output_bytes),
        ),
    );

    let mut written = 0u64;
    let mut last_progress_percent = None;
    for _ in 0..sparse.total_chunks {
        let mut chunk_header = vec![0u8; sparse.chunk_header_size];
        input
            .read_exact(&mut chunk_header)
            .map_err(|e| format!("Read Android sparse chunk header failed: {e}"))?;
        let chunk = parse_sparse_chunk_header(&sparse, &chunk_header)?;

        if sparse_chunk_requires_write(chunk.kind) {
            match chunk.kind {
                SparseChunkKind::Raw => {
                    write_sparse_stream_chunk(
                        device,
                        &mut input,
                        image,
                        written,
                        chunk.output_bytes,
                    )
                    .await?
                }
                SparseChunkKind::Fill => {
                    let mut fill = [0u8; 4];
                    input
                        .read_exact(&mut fill)
                        .map_err(|e| format!("Read Android sparse fill value failed: {e}"))?;
                    write_sparse_fill_chunk(device, image, written, chunk.output_bytes, &fill)
                        .await?
                }
                SparseChunkKind::DontCare | SparseChunkKind::Crc32 => unreachable!(),
            }
        } else if chunk.kind == SparseChunkKind::Crc32 {
            let mut checksum = [0u8; 4];
            input
                .read_exact(&mut checksum)
                .map_err(|e| format!("Read Android sparse checksum failed: {e}"))?;
        }

        written = written
            .checked_add(chunk.output_bytes)
            .ok_or_else(|| "Android sparse output size overflows".to_string())?;
        let percent = (written * 100) / sparse.output_bytes;
        if should_emit_progress(last_progress_percent, percent) {
            emit_progress(
                app,
                format!(
                    "Writing {}... {}/{} ({percent}%)",
                    image.name,
                    format_byte_count(written),
                    format_byte_count(sparse.output_bytes),
                ),
            );
            last_progress_percent = Some(percent);
        }
    }

    if written != sparse.output_bytes {
        return Err(format!(
            "Android sparse image {} expands to {written} bytes, expected {}",
            image.name, sparse.output_bytes
        ));
    }
    Ok(format!("Written {} successfully", image.name))
}

fn sparse_chunk_requires_write(kind: SparseChunkKind) -> bool {
    matches!(kind, SparseChunkKind::Raw | SparseChunkKind::Fill)
}

async fn write_sparse_stream_chunk(
    device: &mut Device,
    input: &mut File,
    image: &FirmwareImage,
    output_offset: u64,
    byte_count: u64,
) -> Result<(), String> {
    let mut remaining = byte_count;
    let mut offset = output_offset;
    while remaining > 0 {
        let count = remaining.min(WRITE_LBA_CHUNK as u64) as usize;
        let mut data = vec![0u8; count];
        input
            .read_exact(&mut data)
            .map_err(|e| format!("Read Android sparse payload failed: {e}"))?;
        write_sparse_lba_chunk(device, image, offset, &data).await?;
        remaining -= count as u64;
        offset += count as u64;
    }
    Ok(())
}

async fn write_sparse_fill_chunk(
    device: &mut Device,
    image: &FirmwareImage,
    output_offset: u64,
    byte_count: u64,
    fill: &[u8; 4],
) -> Result<(), String> {
    let mut remaining = byte_count;
    let mut offset = output_offset;
    let mut data = vec![0u8; WRITE_LBA_CHUNK];
    for pattern in data.chunks_exact_mut(fill.len()) {
        pattern.copy_from_slice(fill);
    }
    while remaining > 0 {
        let count = remaining.min(data.len() as u64) as usize;
        write_sparse_lba_chunk(device, image, offset, &data[..count]).await?;
        remaining -= count as u64;
        offset += count as u64;
    }
    Ok(())
}

async fn write_sparse_lba_chunk(
    device: &mut Device,
    image: &FirmwareImage,
    output_offset: u64,
    data: &[u8],
) -> Result<(), String> {
    let sector_offset = output_offset / SECTOR_SIZE as u64;
    let start_sector = image
        .flash_offset_sectors
        .checked_add(sector_offset)
        .and_then(|sector| u32::try_from(sector).ok())
        .ok_or_else(|| "Android sparse LBA address overflows".to_string())?;
    let transferred = device
        .write_lba(start_sector, data)
        .await
        .map_err(|e| format!("Write {} at LBA {start_sector} failed: {e}", image.name))?;
    if transferred as usize != data.len() {
        return Err(format!(
            "Short write for {} at LBA {start_sector}: got {transferred} bytes, expected {}",
            image.name,
            data.len()
        ));
    }
    Ok(())
}

fn validate_firmware_range(
    image: &FirmwareImage,
    total_sectors: u64,
    flash_sectors: u32,
) -> Result<u64, String> {
    if image.flash_size_sectors > 0 && total_sectors > image.flash_size_sectors {
        return Err(format!(
            "Image {} ({total_sectors} sectors) exceeds its firmware partition ({} sectors)",
            image.name, image.flash_size_sectors
        ));
    }
    let end_sector = image
        .flash_offset_sectors
        .checked_add(total_sectors)
        .ok_or_else(|| "Firmware image range overflows".to_string())?;
    if end_sector > u64::from(flash_sectors) {
        return Err(format!(
            "Image {} LBA range 0x{:x}-0x{:x} exceeds flash size 0x{flash_sectors:x}",
            image.name,
            image.flash_offset_sectors,
            end_sector.saturating_sub(1)
        ));
    }
    Ok(total_sectors)
}

fn gpt_tables_for_firmware(
    images: &[FirmwareImage],
    flash_sectors: u32,
) -> Result<Option<GptTables>, String> {
    let Some(parameter) = images.iter().find(|image| is_parameter_image(image)) else {
        return Ok(None);
    };
    let bytes =
        std::fs::read(&parameter.path).map_err(|e| format!("Read parameter image failed: {e}"))?;
    let Some(partitions) = parse_gpt_parameter(&bytes)? else {
        return Ok(None);
    };
    Ok(Some(build_gpt_tables(&partitions, flash_sectors)?))
}

fn is_parameter_image(image: &FirmwareImage) -> bool {
    image.name.eq_ignore_ascii_case("parameter")
        || image
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("parameter.txt"))
}

fn firmware_write_target(image: &FirmwareImage) -> FirmwareImage {
    let mut target = image.clone();
    if is_parameter_image(&target) {
        target.flash_offset_sectors = FIRMWARE_PARAMETER_START_SECTOR;
    }
    target
}

#[derive(Debug, serde::Serialize)]
pub struct DevicePartitionEntry {
    pub name: String,
    pub start_sector: u64,
    pub sector_count: Option<u64>,
}

/// Parse a device parameter table file (SDK parameter.txt style) into entries
/// for the Download Image page.
#[tauri::command]
pub async fn parse_device_partition_table(
    path: String,
) -> Result<Vec<DevicePartitionEntry>, String> {
    let data = std::fs::read(&path).map_err(|e| format!("Read partition table failed: {e}"))?;
    let partitions = match parse_gpt_parameter(&data)? {
        Some(partitions) => partitions,
        None => parse_parameter_partitions(&data)?
            .ok_or_else(|| "Parameter file has no partition entries".to_string())?,
    };
    Ok(partitions
        .into_iter()
        .map(|partition| DevicePartitionEntry {
            name: partition.name,
            start_sector: partition.start_sector,
            sector_count: partition.sector_count,
        })
        .collect())
}

async fn write_gpt_tables(
    app: &AppHandle,
    device: &mut Device,
    tables: &GptTables,
) -> Result<(), String> {
    emit_log(app, "Writing GPT partition table");
    write_gpt_bytes(device, 0, &tables.primary).await?;
    write_gpt_bytes(device, tables.backup_start_sector, &tables.backup).await?;
    emit_log(app, "Written GPT partition table successfully");
    Ok(())
}

async fn write_gpt_bytes(
    device: &mut Device,
    start_sector: u32,
    data: &[u8],
) -> Result<(), String> {
    for (index, chunk) in data.chunks(WRITE_LBA_CHUNK).enumerate() {
        let sector_offset = u32::try_from(index * (WRITE_LBA_CHUNK / SECTOR_SIZE))
            .map_err(|_| "GPT LBA address overflows".to_string())?;
        let sector = start_sector
            .checked_add(sector_offset)
            .ok_or_else(|| "GPT LBA address overflows".to_string())?;
        let transferred = device
            .write_lba(sector, chunk)
            .await
            .map_err(|e| format!("Write GPT at LBA {sector} failed: {e}"))?;
        if transferred as usize != chunk.len() {
            return Err(format!(
                "Short GPT write at LBA {sector}: got {transferred} bytes, expected {}",
                chunk.len()
            ));
        }
    }
    Ok(())
}

fn create_upgrade_temp_dir() -> Result<PathBuf, String> {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path =
        std::env::temp_dir().join(format!("rkdevtool-upgrade-{}-{nonce}", std::process::id()));
    std::fs::create_dir(&path).map_err(|e| format!("Create upgrade temp directory failed: {e}"))?;
    Ok(path)
}

fn loader_ready_progress_percent(attempt: usize) -> u64 {
    if attempt == 0 {
        return 0;
    }
    ((attempt as u64 * 100) / LOADER_READY_RETRIES as u64).clamp(1, 99)
}

fn download_requires_reset(has_images: bool) -> bool {
    has_images
}

async fn wait_for_loader_device(
    app: &AppHandle,
    state: &AppState,
) -> Result<(Device, FlashInfo), String> {
    let mut last_status = String::from("Loader is still starting");
    for attempt in 0..LOADER_READY_RETRIES {
        emit_progress(
            app,
            format!(
                "Waiting for Loader... ({}%)",
                loader_ready_progress_percent(attempt)
            ),
        );
        // Prefer a newly enumerated Loader interface. During the transition from
        // Maskrom, Windows can keep the old interface visible for a while, so
        // also probe the currently selected Rockusb interface as a fallback.
        match open_loader_device(state).await {
            Ok(mut device) => {
                match device.flash_info().await {
                    Ok(info) => {
                        emit_progress(app, "Waiting for Loader... (100%)".to_string());
                        emit_log(app, "Loader is ready");
                        return Ok((device, info));
                    }
                    Err(err) => {
                        last_status = format!("Flash is not ready: {err}");
                    }
                }
            }
            Err(loader_err) => match open_selected_device(state).await {
                Ok(mut device) => match device.flash_info().await {
                    Ok(info) => {
                        emit_progress(app, "Waiting for Loader... (100%)".to_string());
                        emit_log(app, "Loader is ready");
                        return Ok((device, info));
                    }
                    Err(err) => {
                        last_status = format!(
                            "{loader_err}; current interface probe failed: {err}"
                        );
                    }
                },
                Err(current_err) => {
                    last_status = format!("{loader_err}; current interface unavailable: {current_err}");
                }
            },
        }
        thread::sleep(LOADER_READY_INTERVAL);
    }
    Err(format!(
        "Loader did not become ready after Boot download; re-enter Maskrom and retry ({last_status})"
    ))
}

async fn open_loader_and_read_flash_info(state: &AppState) -> Result<(Device, FlashInfo), String> {
    let mut device = open_loader_device(state).await?;
    let info = device
        .flash_info()
        .await
        .map_err(|e| format!("Read Flash info failed: {e}"))?;
    Ok((device, info))
}

/// Extract a firmware package, download its Loader in Maskrom, then write every package entry via RockUSB LBA.
#[tauri::command]
pub async fn upgrade_firmware(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
    no_reset: Option<bool>,
) -> Result<(), String> {
    devices::ensure_backend_not_busy(state.inner())?;
    devices::set_backend_busy(state.inner(), true)?;

    let temp_dir = match create_upgrade_temp_dir() {
        Ok(path) => path,
        Err(err) => {
            let _ = devices::set_backend_busy(state.inner(), false);
            return Err(err);
        }
    };
    let result = async {
        emit_log(&app, &format!("> rockusb upgrade {path}"));
        emit_log(
            &app,
            &format!("Extracting firmware to {}", temp_dir.display()),
        );
        let extract_path = path.clone();
        let extract_dir = temp_dir.clone();
        let firmware = tauri::async_runtime::spawn_blocking(move || {
            extract_firmware_for_upgrade(&extract_path, &extract_dir.display().to_string())
        })
        .await
        .map_err(|e| e.to_string())??;
        emit_lines(&app, &firmware.log);

        let (mut device, flash_info) = if selected_device_is_maskrom(state.inner()).await? {
            let loader_path = firmware.loader_path.as_ref().ok_or_else(|| {
                "Firmware has no download.bin or MiniLoaderAll.bin for Maskrom Boot download"
                    .to_string()
            })?;
            emit_log(
                &app,
                &format!("Downloading Loader: {}", loader_path.display()),
            );
            let mut maskrom_device = open_selected_device(state.inner()).await?;
            let loader_log = download_boot_to_device(&mut maskrom_device, loader_path).await?;
            emit_lines(&app, &loader_log);
            drop(maskrom_device);
            wait_for_loader_device(&app, state.inner()).await?
        } else {
            open_loader_and_read_flash_info(state.inner()).await?
        };

        let flash_sectors = flash_info.sectors();
        if flash_sectors == 0 {
            return Err("Flash reports 0 sectors".to_string());
        }

        if let Some(loader_path) = firmware.loader_path.as_deref() {
            write_loader_idblock(&app, &mut device, loader_path, flash_sectors).await?;
        }

        let gpt_tables = gpt_tables_for_firmware(&firmware.images, flash_sectors)?;
        if let Some(tables) = gpt_tables.as_ref() {
            write_gpt_tables(&app, &mut device, tables).await?;
        }

        for image in &firmware.images {
            let target = firmware_write_target(image);
            let line = write_firmware_image(&app, &mut device, &target, flash_sectors).await?;
            emit_log(&app, &line);
        }

        if !no_reset.unwrap_or(false) {
            device
                .reset_device(ResetOpcode::Reset)
                .await
                .map_err(|e| format!("Reset device after upgrade failed: {e}"))?;
            emit_log(&app, "Reset Device Success");
        }
        emit_log(&app, "Firmware upgrade succeeded");
        Ok(())
    }
    .await;

    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = devices::set_backend_busy(state.inner(), false);
    result
}

/// Download Boot / Loader into Maskrom.
#[tauri::command]
pub async fn download_boot(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    let boot_path = Path::new(&path).to_path_buf();
    if !boot_path.is_file() {
        return Err(format!("Boot/Loader file not found: {path}"));
    }

    with_busy_device(
        &app,
        &state,
        &format!("download-boot {path}"),
        move |mut device| async move {
            let log = download_boot_to_device(&mut device, &boot_path).await?;
            Ok(((), log))
        },
    )
    .await
}

async fn switch_download_storage(
    app: &AppHandle,
    device: &mut Device,
    storage: &str,
) -> Result<(), String> {
    let no = storage_name_to_ui_no(storage)?;
    let target = storage_from_ui_no(no)?;
    device
        .switch_storage(target)
        .await
        .map_err(|e| format!("Switch storage to {} failed: {e}", storage.trim()))?;
    let current = device
        .storage()
        .await
        .map_err(|e| format!("Verify storage switch failed: {e}"))?;
    if current != target {
        return Err(format!(
            "Storage switch to {} was not applied; device reports {}",
            storage.trim(),
            storage_to_ui(current).name
        ));
    }
    emit_log(
        app,
        &format!(
            "Switched storage to {}",
            storage.trim().to_ascii_uppercase()
        ),
    );
    Ok(())
}

/// Execute Download Image page writes through RockUSB without invoking upgrade_tool.
pub async fn download_execute(
    app: AppHandle,
    state: State<'_, AppState>,
    payload: DownloadExecutePayload,
) -> Result<(), String> {
    let rows: Vec<_> = payload
        .rows
        .into_iter()
        .filter(|row| row.enabled && !row.path.trim().is_empty())
        .collect();
    if rows.is_empty() {
        return Err("Enable at least one row and provide an image path".to_string());
    }

    let loader_rows: Vec<_> = rows
        .iter()
        .filter(|row| row.name.trim().eq_ignore_ascii_case("loader"))
        .collect();
    if loader_rows.len() > 1 {
        return Err("Only one Loader row can be written at a time".to_string());
    }

    devices::ensure_backend_not_busy(state.inner())?;
    devices::set_backend_busy(state.inner(), true)?;
    let result = async {
        emit_log(&app, "> rockusb download-image");
        let mut device = open_selected_device(state.inner()).await?;
        let loader_storage = loader_rows
            .first()
            .map(|row| row.storage.trim())
            .filter(|storage| !storage.is_empty())
            .map(str::to_string);

        if let Some(loader) = loader_rows.first() {
            let was_maskrom = selected_device_is_maskrom(state.inner()).await?;
            emit_log(&app, &format!("Downloading Loader: {}", loader.path));
            let log = download_boot_to_device(&mut device, Path::new(&loader.path)).await?;
            emit_lines(&app, &log);
            if was_maskrom {
                drop(device);
                device = wait_for_loader_device(&app, state.inner()).await?.0;
            }
        }

        let has_images = rows
            .iter()
            .any(|row| !row.name.trim().eq_ignore_ascii_case("loader"));
        if !has_images {
            emit_log(&app, "Download image succeeded");
            return Ok(());
        }

        let mut active_storage: Option<String> = None;
        for row in rows
            .iter()
            .filter(|row| !row.name.trim().eq_ignore_ascii_case("loader"))
        {
            let requested_storage = if row.storage.trim().is_empty() {
                loader_storage.as_deref()
            } else {
                Some(row.storage.trim())
            };
            if let Some(storage) = requested_storage {
                let storage = storage.to_ascii_uppercase();
                if active_storage.as_deref() != Some(storage.as_str()) {
                    switch_download_storage(&app, &mut device, &storage).await?;
                    active_storage = Some(storage);
                }
            }

            let partition = if payload.force_by_address {
                DownloadPartition {
                    name: row.name.trim().to_string(),
                    start_sector: parse_download_lba(&row.address)?,
                    sector_count: None,
                }
            } else {
                let partitions = read_download_partitions(&mut device).await?;
                resolve_download_partition(&partitions, &row.name)?
            };
            let flash_sectors = device
                .flash_info()
                .await
                .map_err(|e| format!("Read Flash info failed: {e}"))?
                .sectors();
            if flash_sectors == 0 {
                return Err("Flash reports 0 sectors".to_string());
            }
            let image = image_for_download(&row.name, &row.path, partition)?;
            let line = write_firmware_image(&app, &mut device, &image, flash_sectors).await?;
            emit_log(&app, &line);
        }

        if download_requires_reset(has_images) {
            device
                .reset_device(ResetOpcode::Reset)
                .await
                .map_err(|e| format!("Reset device after image download failed: {e}"))?;
            emit_log(&app, "Reset Device Success");
        }
        emit_log(&app, "Download image succeeded");
        Ok(())
    }
    .await;
    let _ = devices::set_backend_busy(state.inner(), false);
    result
}

/// Test device connectivity via Test Unit Ready (rkdeveloptool / upgrade_tool `TD`).
pub async fn test_device(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    with_busy_device(&app, &state, "test-device", |mut device| async move {
        device
            .test_unit_ready()
            .await
            .map_err(|e| format!("Test Device failed: {e}"))?;
        let output = "Test Device OK.".to_string();
        Ok((output.clone(), output))
    })
    .await
}

pub async fn reset_device(
    app: AppHandle,
    state: State<'_, AppState>,
    opcode: ResetOpcode,
) -> Result<String, String> {
    let label = opcode.to_string();
    let result = with_busy_device(
        &app,
        &state,
        &format!("reset-device {label}"),
        move |mut device| async move {
            device
                .reset_device(opcode)
                .await
                .map_err(|e| format!("Reset device failed: {e}"))?;
            let output = match opcode {
                ResetOpcode::Maskrom => {
                    "Maskrom reset command acknowledged; waiting for USB re-enumeration".to_string()
                }
                ResetOpcode::Reset => "Reset Device Success".to_string(),
                other => format!("Reset device ({other}) success"),
            };
            Ok((output.clone(), output))
        },
    )
    .await?;

    Ok(result)
}

/// Plan LBA operation chunks: `(offset, count)` pairs.
fn lba_chunks(start: u32, count: u32, chunk_size: u32) -> Result<Vec<(u32, u16)>, String> {
    if count == 0 {
        return Err("Sector count must be greater than 0".to_string());
    }
    if chunk_size == 0 || chunk_size > u32::from(u16::MAX) {
        return Err("Invalid LBA chunk size".to_string());
    }
    let _end = start
        .checked_add(count)
        .ok_or_else(|| "Start sector + count overflows u32".to_string())?;

    let mut chunks = Vec::new();
    let mut offset = start;
    let mut remaining = count;
    while remaining > 0 {
        let chunk = remaining.min(chunk_size) as u16;
        chunks.push((offset, chunk));
        offset += u32::from(chunk);
        remaining -= u32::from(chunk);
    }
    Ok(chunks)
}

fn erase_lba_chunks(start: u32, count: u32) -> Result<Vec<(u32, u16)>, String> {
    lba_chunks(start, count, ERASE_LBA_CHUNK)
}

fn read_lba_chunks(start: u32, count: u32) -> Result<Vec<(u32, u16)>, String> {
    lba_chunks(start, count, READ_LBA_CHUNK)
}

/// Erase LBA sectors (upgrade_tool `EL` / rkdeveloptool `RKU_EraseLBA`).
pub async fn erase_sectors(
    app: AppHandle,
    state: State<'_, AppState>,
    start: u32,
    count: u32,
) -> Result<String, String> {
    let chunks = erase_lba_chunks(start, count)?;
    let chunk_total = chunks.len();

    with_busy_device(
        &app,
        &state,
        &format!("erase-lba start={start} count={count}"),
        move |mut device| async move {
            for (offset, chunk) in chunks {
                device.erase_lba(offset, chunk).await.map_err(|e| {
                    format!("Erase LBA failed at sector {offset} (count {chunk}): {e}")
                })?;
            }
            let output =
                format!("Erase sectors OK: start={start}, count={count}, chunks={chunk_total}");
            Ok((output.clone(), output))
        },
    )
    .await
}

fn is_emmc_flash_id(id: &FlashId) -> bool {
    id.to_str().as_bytes().get(..4) == Some(b"EMMC")
}

/// Plan `RKU_EraseBlock` chunks: `(block_offset, block_count)`.
fn erase_block_chunks(block_count: u32) -> Result<Vec<(u32, u16)>, String> {
    if block_count == 0 {
        return Err("Flash block count is 0".to_string());
    }
    let mut chunks = Vec::new();
    let mut offset = 0u32;
    let mut remaining = block_count;
    while remaining > 0 {
        let chunk = remaining.min(u32::from(ERASE_BLOCK_CHUNK)) as u16;
        chunks.push((offset, chunk));
        offset += u32::from(chunk);
        remaining -= u32::from(chunk);
    }
    Ok(chunks)
}

fn flash_block_count(info: &FlashInfo) -> Result<u32, String> {
    let block_size = u32::from(info.block_size_sectors());
    if block_size == 0 {
        return Err("Flash reports block size 0".to_string());
    }
    let sectors = info.sectors();
    if sectors == 0 {
        return Err("Flash reports 0 sectors".to_string());
    }
    Ok(sectors / block_size)
}

/// Erase entire flash (upgrade_tool `EF` / rkdeveloptool `ef` / `EraseAllBlocks`).
///
/// Expects Loader mode (user downloads Boot separately). Direct LBA / eMMC uses
/// `EraseLBA` from sector 0; otherwise `EraseForce` by flash blocks (CS0).
pub async fn erase_all(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let app_for_progress = app.clone();
    with_busy_device(&app, &state, "erase-flash", move |mut device| async move {
        let info = device.flash_info().await.map_err(|e| {
            format!(
                "Read Flash info failed: {e} (in Maskrom, download Boot/Loader first, then retry)"
            )
        })?;
        let sectors = info.sectors();
        if sectors == 0 {
            return Err("Flash reports 0 sectors".to_string());
        }

        let direct_lba = device
            .capability()
            .await
            .map(|cap| cap.direct_lba())
            .unwrap_or(false);
        let is_emmc = device
            .flash_id()
            .await
            .map(|id| is_emmc_flash_id(&id))
            .unwrap_or(false);

        let mut log = format!(
            "Flash: {} sectors ({} MB), block={} sectors, direct_lba={}, emmc={}\n",
            sectors,
            sectors / 2048,
            info.block_size_sectors(),
            direct_lba,
            is_emmc
        );

        if direct_lba || is_emmc {
            let chunks = erase_lba_chunks(0, sectors)?;
            let total = chunks.len();
            log.push_str(&format!("Erase path: LBA ({total} chunks)\n"));
            for (i, (offset, chunk)) in chunks.into_iter().enumerate() {
                device.erase_lba(offset, chunk).await.map_err(|e| {
                    format!("Erase LBA failed at sector {offset} (count {chunk}): {e}")
                })?;
                if (i + 1) % 8 == 0 || i + 1 == total {
                    let done = offset.saturating_add(u32::from(chunk)).min(sectors);
                    emit_log(
                        &app_for_progress,
                        &format!("Erase progress: {done}/{sectors} sectors"),
                    );
                }
            }
        } else {
            let block_count = flash_block_count(&info)?;
            let chunks = erase_block_chunks(block_count)?;
            let total = chunks.len();
            log.push_str(&format!(
                "Erase path: Force block erase, {block_count} blocks ({total} chunks)\n"
            ));
            for (i, (offset, chunk)) in chunks.into_iter().enumerate() {
                device.erase_force(offset, chunk).await.map_err(|e| {
                    format!("Erase block failed at block {offset} (count {chunk}): {e}")
                })?;
                if (i + 1) % 8 == 0 || i + 1 == total {
                    let done = offset.saturating_add(u32::from(chunk)).min(block_count);
                    emit_log(
                        &app_for_progress,
                        &format!("Erase progress: {done}/{block_count} blocks"),
                    );
                }
            }
        }

        log.push_str("Erase Flash OK");
        Ok((log.clone(), log))
    })
    .await
}

/// Export flash image via ReadLBA (rkdeveloptool `rl`).
///
/// `count == None` means read from `start` to end of flash.
pub async fn export_image(
    app: AppHandle,
    state: State<'_, AppState>,
    start: u32,
    count: Option<u32>,
    output_path: String,
) -> Result<String, String> {
    if output_path.trim().is_empty() {
        return Err("Output path is required".to_string());
    }
    let out = Path::new(&output_path).to_path_buf();
    if let Some(parent) = out.parent() {
        if !parent.as_os_str().is_empty() && !parent.exists() {
            return Err(format!(
                "Output directory does not exist: {}",
                parent.display()
            ));
        }
    }

    let app_for_progress = app.clone();
    with_busy_device(
        &app,
        &state,
        &format!("read-lba start={start} -> {}", out.display()),
        move |mut device| async move {
            let total_sectors = device
                .flash_info()
                .await
                .map_err(|e| {
                    format!(
                        "Read Flash info failed: {e} (in Maskrom, download Boot/Loader first, then retry)"
                    )
                })?
                .sectors();
            if total_sectors == 0 {
                return Err("Flash reports 0 sectors".to_string());
            }
            if start >= total_sectors {
                return Err(format!(
                    "Start sector {start} is beyond flash size ({total_sectors} sectors)"
                ));
            }

            let count = match count {
                Some(c) if c > 0 => c,
                Some(_) => return Err("Sector count must be greater than 0".to_string()),
                None => total_sectors - start,
            };
            let end = start
                .checked_add(count)
                .ok_or_else(|| "Start sector + count overflows u32".to_string())?;
            if end > total_sectors {
                return Err(format!(
                    "Read range [{start}, {end}) exceeds flash size ({total_sectors} sectors)"
                ));
            }

            let chunks = read_lba_chunks(start, count)?;
            let chunk_total = chunks.len();
            let mut file = File::create(&out)
                .map_err(|e| format!("Create output file failed: {e}"))?;
            let mut buf = vec![0u8; READ_LBA_CHUNK as usize * 512];

            for (i, (offset, chunk)) in chunks.into_iter().enumerate() {
                let nbytes = chunk as usize * 512;
                let slice = &mut buf[..nbytes];
                let transferred = device.read_lba(offset, slice).await.map_err(|e| {
                    format!("Read LBA failed at sector {offset} (count {chunk}): {e}")
                })?;
                if transferred as usize != nbytes {
                    return Err(format!(
                        "Short read at sector {offset}: got {transferred} bytes, expected {nbytes}"
                    ));
                }
                file.write_all(slice)
                    .map_err(|e| format!("Write output file failed: {e}"))?;

                if (i + 1) % 32 == 0 || i + 1 == chunk_total {
                    let done = offset.saturating_add(u32::from(chunk)).saturating_sub(start);
                    let pct = ((u64::from(done) * 100) / u64::from(count)).min(100);
                    emit_log(
                        &app_for_progress,
                        &format!("Export progress: {done}/{count} sectors ({pct}%)"),
                    );
                }
            }
            file.flush()
                .map_err(|e| format!("Flush output file failed: {e}"))?;

            let bytes = u64::from(count) * 512;
            let output = format!(
                "Export image OK: start={start}, count={count}, bytes={bytes}, file={}",
                out.display()
            );
            Ok((output.clone(), output))
        },
    )
    .await
}

/// Handle rockusb-backed advanced actions. Returns `None` if action stays on upgrade_tool.
pub async fn try_run_action(
    app: AppHandle,
    state: State<'_, AppState>,
    action: &str,
    start_sector: Option<&str>,
    sector_count: Option<&str>,
    output_path: Option<&str>,
) -> Result<Option<String>, String> {
    match action {
        "read-flash-id" => Ok(Some(read_flash_id(app, state).await?)),
        "read-flash-info" => Ok(Some(read_flash_info(app, state).await?)),
        "read-chip-info" => Ok(Some(read_chip_info(app, state).await?)),
        "read-capability" => Ok(Some(read_capability(app, state).await?)),
        "test-device" => Ok(Some(test_device(app, state).await?)),
        "reboot-device" => Ok(Some(reset_device(app, state, ResetOpcode::Reset).await?)),
        "enter-maskrom" => Ok(Some(reset_device(app, state, ResetOpcode::Maskrom).await?)),
        "switch-storage" => {
            let no: u32 = start_sector
                .filter(|s| !s.is_empty())
                .unwrap_or("1")
                .parse()
                .map_err(|_| "Invalid storage index".to_string())?;
            Ok(Some(switch_storage(app, state, no).await?))
        }
        "erase-sector" => {
            let start: u32 = start_sector
                .filter(|s| !s.is_empty())
                .unwrap_or("0")
                .parse()
                .map_err(|_| "Invalid start sector".to_string())?;
            let count: u32 = sector_count
                .filter(|s| !s.is_empty())
                .unwrap_or("1")
                .parse()
                .map_err(|_| "Invalid sector count".to_string())?;
            Ok(Some(erase_sectors(app, state, start, count).await?))
        }
        "erase-all" => Ok(Some(erase_all(app, state).await?)),
        "export-image" => {
            let start: u32 = start_sector
                .filter(|s| !s.is_empty())
                .unwrap_or("0")
                .parse()
                .map_err(|_| "Invalid start sector".to_string())?;
            let count = match sector_count.filter(|s| !s.is_empty()) {
                Some(s) => Some(
                    s.parse::<u32>()
                        .map_err(|_| "Invalid sector count".to_string())?,
                ),
                None => None,
            };
            let path = output_path
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "Output path is required".to_string())?
                .to_string();
            Ok(Some(export_image(app, state, start, count, path).await?))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_progress_emits_only_when_the_percent_changes() {
        assert!(should_emit_progress(None, 0));
        assert!(!should_emit_progress(Some(75), 75));
        assert!(should_emit_progress(Some(75), 76));
        assert!(should_emit_progress(Some(99), 100));
    }

    #[test]
    fn android_sparse_dont_care_chunks_do_not_transfer_data() {
        assert!(sparse_chunk_requires_write(SparseChunkKind::Raw));
        assert!(sparse_chunk_requires_write(SparseChunkKind::Fill));
        assert!(!sparse_chunk_requires_write(SparseChunkKind::DontCare));
    }

    #[test]
    fn byte_count_below_one_kib_uses_bytes() {
        assert_eq!(format_byte_count(512), "512 B");
    }

    #[test]
    fn byte_count_at_one_kib_uses_kib() {
        assert_eq!(format_byte_count(1024), "1 KiB");
    }

    #[test]
    fn byte_count_in_mebibytes_uses_two_decimal_places() {
        assert_eq!(format_byte_count(3_223_040), "3.07 MiB");
    }

    #[test]
    fn byte_count_at_one_gib_uses_gib() {
        assert_eq!(format_byte_count(1024 * 1024 * 1024), "1 GiB");
    }

    #[test]
    fn write_plan_pads_the_final_sector_and_limits_chunks() {
        let chunks = write_lba_chunk_plan(0x20, 128 * 512 + 1).unwrap();

        assert_eq!(
            chunks,
            vec![
                LbaWriteChunk {
                    start_sector: 0x20,
                    source_len: 128 * 512,
                    transfer_len: 128 * 512,
                },
                LbaWriteChunk {
                    start_sector: 0x20 + 128,
                    source_len: 1,
                    transfer_len: 512,
                },
            ]
        );
    }

    #[test]
    fn new_idblock_aligns_each_loader_component_to_four_sectors() {
        let idblock = new_idblock(
            vec![1; 512],
            None,
            vec![2; 513],
            vec![3; 2048],
            false,
        )
        .unwrap();

        assert_eq!(idblock.len(), 2048 * 3);
        assert_eq!(&idblock[..512], &[1; 512]);
        assert_eq!(&idblock[2048..2048 + 513], &[2; 513]);
        assert_eq!(&idblock[4096..4096 + 2048], &[3; 2048]);
    }

    #[test]
    fn new_idblock_places_flash_boost_before_flash_data() {
        let idblock = new_idblock(
            vec![1; 512],
            Some(vec![2; 512]),
            vec![3; 512],
            vec![4; 512],
            false,
        )
        .unwrap();

        assert_eq!(idblock.len(), 2048 * 4);
        assert_eq!(&idblock[..512], &[1; 512]);
        assert_eq!(&idblock[2048..2048 + 512], &[2; 512]);
        assert_eq!(&idblock[4096..4096 + 512], &[3; 512]);
        assert_eq!(&idblock[6144..6144 + 512], &[4; 512]);
    }

    #[test]
    fn legacy_idblock_has_the_official_header_after_rc4_decoding() {
        let mut idblock = legacy_idblock(vec![0x11; 512], vec![0x22; 512], false).unwrap();

        rc4_crypt(&mut idblock[..SECTOR_SIZE]);
        rc4_crypt(&mut idblock[SECTOR_SIZE * 2..SECTOR_SIZE * 3]);
        assert_eq!(&idblock[..4], &0x0ff0_aa55u32.to_le_bytes());
        assert_eq!(&idblock[506..508], &4u16.to_le_bytes());
        assert_eq!(&idblock[508..510], &8u16.to_le_bytes());
        assert_eq!(&idblock[SECTOR_SIZE * 4..SECTOR_SIZE * 5], &[0x11; 512]);
        assert_eq!(&idblock[SECTOR_SIZE * 8..SECTOR_SIZE * 9], &[0x22; 512]);
    }

    #[test]
    fn firmware_parameter_uses_rockchip_parameter_lba() {
        let image = FirmwareImage {
            name: "parameter".to_string(),
            path: PathBuf::from("parameter.txt"),
            flash_offset_sectors: 0,
            flash_size_sectors: 0,
            byte_count: 512,
        };

        assert_eq!(
            firmware_write_target(&image).flash_offset_sectors,
            FIRMWARE_PARAMETER_START_SECTOR
        );
    }

    #[test]
    fn loader_ready_progress_increases_across_retry_attempts() {
        assert_eq!(loader_ready_progress_percent(0), 0);
        assert!(loader_ready_progress_percent(1) > 0);
        assert!(loader_ready_progress_percent(LOADER_READY_RETRIES - 1) < 100);
    }

    #[test]
    fn download_resets_only_after_an_image_was_written() {
        assert!(!download_requires_reset(false));
        assert!(download_requires_reset(true));
    }

    #[test]
    fn ui_storage_roundtrip_common() {
        for no in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            let idx = storage_from_ui_no(no).unwrap();
            let info = storage_to_ui(idx);
            assert_eq!(info.no, no, "no={no} idx={idx:?}");
        }
    }

    #[test]
    fn storage_name_maps_spinand() {
        assert_eq!(storage_name_to_ui_no("SPINAND").unwrap(), 6);
        assert_eq!(storage_name_to_ui_no("spinand").unwrap(), 6);
        assert_eq!(
            storage_from_ui_no(storage_name_to_ui_no("SPINAND").unwrap()).unwrap(),
            StorageIndex::MtdBlkSpiNand
        );
    }

    #[test]
    fn mtd_spi_nand_maps_to_spinand_ui_slot() {
        let info = storage_to_ui(StorageIndex::MtdBlkSpiNand);
        assert_eq!(info.no, 6);
        assert_eq!(info.name, "SPINAND");
        assert_eq!(storage_from_ui_no(6).unwrap(), StorageIndex::MtdBlkSpiNand);
    }

    #[test]
    fn sata_pcie_map_to_protocol_bits() {
        assert_eq!(storage_from_ui_no(9).unwrap(), StorageIndex::Sata);
        assert_eq!(storage_from_ui_no(10).unwrap(), StorageIndex::Pcie);
        assert_eq!(storage_to_ui(StorageIndex::Sata).name, "SATA");
        assert_eq!(storage_to_ui(StorageIndex::SpiNand).no, 6);
    }

    #[test]
    fn erase_lba_chunks_single_and_split() {
        assert_eq!(erase_lba_chunks(0, 1).unwrap(), vec![(0, 1)]);
        assert_eq!(
            erase_lba_chunks(100, ERASE_LBA_CHUNK).unwrap(),
            vec![(100, ERASE_LBA_CHUNK as u16)]
        );
        assert_eq!(
            erase_lba_chunks(0, ERASE_LBA_CHUNK + 10).unwrap(),
            vec![(0, ERASE_LBA_CHUNK as u16), (ERASE_LBA_CHUNK, 10)]
        );
        assert!(erase_lba_chunks(0, 0).is_err());
        assert!(erase_lba_chunks(u32::MAX, 1).is_err());
    }

    #[test]
    fn read_lba_chunks_match_rkdeveloptool_default() {
        assert_eq!(read_lba_chunks(0, 128).unwrap(), vec![(0, 128)]);
        assert_eq!(
            read_lba_chunks(10, 200).unwrap(),
            vec![(10, 128), (138, 72)]
        );
    }

    #[test]
    fn erase_block_chunks_respect_max() {
        assert_eq!(erase_block_chunks(1).unwrap(), vec![(0, 1)]);
        assert_eq!(erase_block_chunks(20).unwrap(), vec![(0, 16), (16, 4)]);
        assert!(erase_block_chunks(0).is_err());
    }

    #[test]
    fn flash_block_count_from_sectors() {
        // 256 MiB / 128 KiB blocks → 2048 blocks
        let mut raw = [0u8; 11];
        raw[0..4].copy_from_slice(&524288u32.to_le_bytes()); // sectors
        raw[4..6].copy_from_slice(&256u16.to_le_bytes()); // block size in sectors (128KiB)
        let info = FlashInfo::from_bytes(raw);
        assert_eq!(flash_block_count(&info).unwrap(), 2048);
    }

    #[test]
    fn download_address_accepts_hexadecimal_and_decimal_lba() {
        assert_eq!(parse_download_lba("0x00002000").unwrap(), 0x2000);
        assert_eq!(parse_download_lba("8192").unwrap(), 0x2000);
        assert!(parse_download_lba("0xnope").is_err());
    }

    #[test]
    fn resolves_download_partition_without_case_sensitivity() {
        let partitions = vec![DownloadPartition {
            name: "boot".to_string(),
            start_sector: 0xc800,
            sector_count: Some(0x14000),
        }];

        let target = resolve_download_partition(&partitions, "BOOT").unwrap();
        assert_eq!(target.start_sector, 0xc800);
        assert_eq!(target.sector_count, Some(0x14000));
    }

    #[test]
    fn reads_download_partitions_from_gpt_entries() {
        let parameter = b"TYPE: GPT\0CMDLINE:mtdparts=rk29xxnand:0x00002000@0x00002000(uboot),0x00014000@0x0000c800(boot)\0";
        let source = parse_gpt_parameter(parameter).unwrap().unwrap();
        let tables = build_gpt_tables(&source, 0x0080_0000).unwrap();

        let partitions = parse_gpt_download_partitions(
            &tables.primary[SECTOR_SIZE..SECTOR_SIZE * 2],
            &tables.primary[SECTOR_SIZE * 2..],
        )
        .unwrap()
        .unwrap();

        assert_eq!(partitions[1].name, "boot");
        assert_eq!(partitions[1].start_sector, 0xc800);
        assert_eq!(partitions[1].sector_count, Some(0x14000));
    }
}

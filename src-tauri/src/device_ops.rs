//! Device operations via rockusb (chip/flash/storage/capability/boot/reset).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::thread;
use std::time::Duration;

use tauri::{AppHandle, Emitter, State};

use rockfile::boot::{
    RkBootEntry, RkBootEntryBytes, RkBootHeader, RkBootHeaderBytes, RkBootHeaderEntry,
};
use rockusb::nusb::Device;
use rockusb::protocol::{Capability, ChipInfo, FlashId, FlashInfo, ResetOpcode, StorageIndex};

use crate::devices::{self, format_location_id_for, is_rockusb_device_info};
use crate::state::AppState;
use crate::upgrade_tool::{CurrentStorageInfo, LogPayload};

const EVENT_TOOL_LOG: &str = "tool-log";

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

fn emit_lines(app: &AppHandle, text: &str) {
    for line in text.lines() {
        emit_log(app, line);
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
            .ok_or_else(|| {
                format!("Selected device {location_id} is no longer connected")
            })?
    } else if infos.len() == 1 {
        infos.into_iter().next().unwrap()
    } else {
        return Err("Multiple devices connected; select one in the status bar".to_string());
    };

    Device::from_usb_device_info(info)
        .await
        .map_err(|e| format!("Failed to open RockUSB device: {e}"))
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
pub async fn read_chip_info(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
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

pub async fn read_flash_id(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
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

pub async fn read_flash_info(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
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

pub async fn read_capability(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<String, String> {
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

/// Download Boot / Loader into Maskrom (upgrade_tool `DB`).
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

    with_busy_device(&app, &state, &format!("download-boot {path}"), move |mut device| async move {
        let mut file =
            File::open(&boot_path).map_err(|e| format!("Open boot file failed: {e}"))?;
        let mut header_bytes: RkBootHeaderBytes = [0; 102];
        file.read_exact(&mut header_bytes)
            .map_err(|e| format!("Read boot header failed: {e}"))?;
        let header = RkBootHeader::from_bytes(&header_bytes)
            .ok_or_else(|| {
                "Failed to parse Loader/Boot header (use MiniLoaderAll.bin or download.bin)"
                    .to_string()
            })?;

        let mut log = String::from("Download Boot Start\n");
        download_boot_entry(&mut device, header.entry_471, 0x471, &mut file, &mut log).await?;
        download_boot_entry(&mut device, header.entry_472, 0x472, &mut file, &mut log).await?;
        log.push_str("Download Boot Success");
        Ok(((), log))
    })
    .await
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
    with_busy_device(&app, &state, &format!("reset-device {label}"), move |mut device| async move {
        device
            .reset_device(opcode)
            .await
            .map_err(|e| format!("Reset device failed: {e}"))?;
        let output = match opcode {
            ResetOpcode::Maskrom => "Enter Maskrom Success".to_string(),
            ResetOpcode::Reset => "Reset Device Success".to_string(),
            other => format!("Reset device ({other}) success"),
        };
        Ok((output.clone(), output))
    })
    .await
}

/// Handle rockusb-backed advanced actions. Returns `None` if action stays on upgrade_tool.
pub async fn try_run_action(
    app: AppHandle,
    state: State<'_, AppState>,
    action: &str,
    start_sector: Option<&str>,
) -> Result<Option<String>, String> {
    match action {
        "读取FlashID" => Ok(Some(read_flash_id(app, state).await?)),
        "读取Flash信息" => Ok(Some(read_flash_info(app, state).await?)),
        "读取Chip信息" => Ok(Some(read_chip_info(app, state).await?)),
        "读取Capability" => Ok(Some(read_capability(app, state).await?)),
        "测试设备" => Ok(Some(test_device(app, state).await?)),
        "重启设备" => Ok(Some(reset_device(app, state, ResetOpcode::Reset).await?)),
        "进入Maskrom" => Ok(Some(reset_device(app, state, ResetOpcode::Maskrom).await?)),
        "切换存储" => {
            let no: u32 = start_sector
                .filter(|s| !s.is_empty())
                .unwrap_or("1")
                .parse()
                .map_err(|_| "Invalid storage index".to_string())?;
            Ok(Some(switch_storage(app, state, no).await?))
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_storage_roundtrip_common() {
        for no in [1, 2, 3, 4, 5, 6, 7, 8, 9, 10] {
            let idx = storage_from_ui_no(no).unwrap();
            let info = storage_to_ui(idx);
            assert_eq!(info.no, no, "no={no} idx={idx:?}");
        }
    }

    #[test]
    fn mtd_spi_nand_maps_to_spinand_ui_slot() {
        let info = storage_to_ui(StorageIndex::MtdBlkSpiNand);
        assert_eq!(info.no, 6);
        assert_eq!(info.name, "SPINAND");
        assert_eq!(
            storage_from_ui_no(6).unwrap(),
            StorageIndex::MtdBlkSpiNand
        );
    }

    #[test]
    fn sata_pcie_map_to_protocol_bits() {
        assert_eq!(storage_from_ui_no(9).unwrap(), StorageIndex::Sata);
        assert_eq!(storage_from_ui_no(10).unwrap(), StorageIndex::Pcie);
        assert_eq!(storage_to_ui(StorageIndex::Sata).name, "SATA");
        assert_eq!(storage_to_ui(StorageIndex::SpiNand).no, 6);
    }
}

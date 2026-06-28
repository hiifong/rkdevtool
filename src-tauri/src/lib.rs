pub mod firmware;
mod state;
mod upgrade_tool;

use firmware::{extract_firmware_file, parse_firmware_info, FirmwareInfo};
use state::AppState;
use upgrade_tool::{
    download_boot, download_execute, get_tool_info, is_tool_busy, list_devices, partition_list,
    read_chip_info, run_action, select_device, upgrade_firmware,
};

#[tauri::command]
fn parse_firmware(path: String) -> Result<FirmwareInfo, String> {
    parse_firmware_info(&path)
}

#[tauri::command]
async fn extract_firmware(path: String, output_dir: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || extract_firmware_file(&path, &output_dir))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            get_tool_info,
            list_devices,
            select_device,
            partition_list,
            upgrade_firmware,
            download_boot,
            download_execute,
            parse_firmware,
            extract_firmware,
            read_chip_info,
            run_action,
            is_tool_busy,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

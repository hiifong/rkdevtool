//! RockUSB device enumeration and USB hotplug watching via `nusb`.
//!
//! LocationID and Maskrom/Loader detection follow the same rules as
//! Rockchip's `rkdeveloptool` / `upgrade_tool` (see RKScan.cpp):
//! - Mode: `(bcdUSB & 1) == 0` → Maskrom, else Loader
//! - Linux LocationID: `(busnum << 8) | port`
//! - macOS LocationID: IOKit `locationID`

use std::collections::HashMap;

use futures::StreamExt;
use nusb::hotplug::HotplugEvent;
use nusb::{DeviceId, DeviceInfo};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::{mpsc, oneshot};

use crate::state::AppState;
use crate::upgrade_tool::RockusbDevice;

pub const EVENT_DEVICES: &str = "devices-updated";

/// Rockchip vendor ID.
const ROCKCHIP_VID: u16 = 0x2207;

pub enum HotplugCmd {
    /// Re-enumerate USB devices and reply with the snapshot.
    Resync {
        reply: oneshot::Sender<Result<Vec<RockusbDevice>, String>>,
    },
}

/// Same filter as rkdeveloptool `IsRockusbDevice` for VID 0x2207:
/// MSC gadgets use PID with high byte 0 (e.g. 0x0010).
pub fn is_rockusb_device_info(info: &DeviceInfo) -> bool {
    info.vendor_id() == ROCKCHIP_VID && (info.product_id() >> 8) > 0
}

fn is_rockusb_device(info: &DeviceInfo) -> bool {
    is_rockusb_device_info(info)
}

/// Maskrom vs Loader from `bcdUSB` LSB (not PID).
fn detect_mode(info: &DeviceInfo) -> &'static str {
    if info.usb_version() & 1 == 0 {
        "Maskrom"
    } else {
        "Loader"
    }
}

/// Build LocationID string compatible with `upgrade_tool -s`.
pub fn format_location_id_for(info: &DeviceInfo) -> String {
    format_location_id(info)
}

fn format_location_id(info: &DeviceInfo) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("{}", info.location_id())
    }

    #[cfg(target_os = "linux")]
    {
        let port = info
            .port_chain()
            .first()
            .copied()
            .unwrap_or_else(|| info.device_address());
        let id = (u32::from(info.busnum()) << 8) | u32::from(port);
        format!("{id}")
    }

    #[cfg(target_os = "windows")]
    {
        // Best-effort stable id; multi-device `-s` may need tuning per upgrade_tool build.
        let port = info.port_number();
        let chain: String = info
            .port_chain()
            .iter()
            .map(|p| p.to_string())
            .collect::<Vec<_>>()
            .join(".");
        if chain.is_empty() {
            format!("{port}")
        } else {
            format!("{port}:{chain}")
        }
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        format!("{}:{}", info.bus_id(), info.device_address())
    }
}

fn device_from_info(info: &DeviceInfo) -> RockusbDevice {
    let location_id = format_location_id(info);
    let mode = detect_mode(info).to_string();
    RockusbDevice {
        label: format!("{location_id} : {}", mode.to_ascii_uppercase()),
        location_id,
        mode,
    }
}

fn snapshot_devices(map: &HashMap<DeviceId, RockusbDevice>) -> Vec<RockusbDevice> {
    let mut devices: Vec<_> = map.values().cloned().collect();
    devices.sort_by(|a, b| a.location_id.cmp(&b.location_id));
    devices
}

fn sync_selected(state: &AppState, devices: &[RockusbDevice]) -> Result<(), String> {
    let mut selected = state.selected_device.lock().map_err(|e| e.to_string())?;
    if devices.is_empty() {
        *selected = None;
    } else if !devices
        .iter()
        .any(|d| selected.as_deref() == Some(d.location_id.as_str()))
    {
        *selected = Some(devices[0].location_id.clone());
    }
    Ok(())
}

/// Publish device list to app state and notify the frontend.
pub fn publish_devices(
    app: &AppHandle,
    state: &AppState,
    devices: &[RockusbDevice],
) -> Result<(), String> {
    *state.last_devices.lock().map_err(|e| e.to_string())? = devices
        .iter()
        .map(|d| (d.location_id.clone(), d.mode.clone(), d.label.clone()))
        .collect();
    sync_selected(state, devices)?;
    let _ = app.emit(EVENT_DEVICES, devices);
    Ok(())
}

pub fn ensure_backend_not_busy(state: &AppState) -> Result<(), String> {
    let busy = state.busy.lock().map_err(|e| e.to_string())?;
    if *busy {
        return Err("Another task is already running; please wait".to_string());
    }
    Ok(())
}

pub fn set_backend_busy(state: &AppState, busy: bool) -> Result<(), String> {
    *state.busy.lock().map_err(|e| e.to_string())? = busy;
    Ok(())
}

pub fn cached_devices(state: &AppState) -> Result<Vec<RockusbDevice>, String> {
    let cache = state.last_devices.lock().map_err(|e| e.to_string())?;
    Ok(cache
        .iter()
        .map(|(location_id, mode, label)| RockusbDevice {
            location_id: location_id.clone(),
            mode: mode.clone(),
            label: label.clone(),
        })
        .collect())
}

async fn enumerate_rockusb_map() -> Result<HashMap<DeviceId, RockusbDevice>, String> {
    let iter = nusb::list_devices()
        .await
        .map_err(|e| format!("Failed to list USB devices: {e}"))?;

    let mut map = HashMap::new();
    for info in iter.filter(is_rockusb_device) {
        map.insert(info.id(), device_from_info(&info));
    }
    Ok(map)
}

fn publish_map(
    app: &AppHandle,
    state: &AppState,
    map: &HashMap<DeviceId, RockusbDevice>,
) -> Result<Vec<RockusbDevice>, String> {
    let devices = snapshot_devices(map);
    publish_devices(app, state, &devices)?;
    Ok(devices)
}

/// Ask the hotplug watcher to re-enumerate, or fall back to a direct scan.
pub async fn resync_devices(app: &AppHandle, state: &AppState) -> Result<Vec<RockusbDevice>, String> {
    let reply_rx = {
        let guard = state.hotplug_tx.lock().map_err(|e| e.to_string())?;
        if let Some(tx) = guard.as_ref() {
            let (reply_tx, reply_rx) = oneshot::channel();
            if tx.send(HotplugCmd::Resync { reply: reply_tx }).is_ok() {
                Some(reply_rx)
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(reply_rx) = reply_rx {
        return reply_rx
            .await
            .map_err(|_| "USB hotplug watcher is not running".to_string())?;
    }

    let map = enumerate_rockusb_map().await?;
    publish_map(app, state, &map)
}

/// Start background USB hotplug watching. Safe to call once from app setup.
pub fn start_hotplug_watcher(app: AppHandle) {
    let (tx, rx) = mpsc::unbounded_channel::<HotplugCmd>();
    {
        let state = app.state::<AppState>();
        let mut slot = state
            .hotplug_tx
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *slot = Some(tx);
    }

    tauri::async_runtime::spawn(async move {
        if let Err(err) = hotplug_loop(app.clone(), rx).await {
            eprintln!("[rkdevtool] USB hotplug watcher stopped: {err}");
            let state = app.state::<AppState>();
            let mut slot = state
                .hotplug_tx
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            *slot = None;
        }
    });
}

async fn hotplug_loop(
    app: AppHandle,
    mut cmd_rx: mpsc::UnboundedReceiver<HotplugCmd>,
) -> Result<(), String> {
    // Create the watch first, then list, so we do not miss connect races.
    let watch = nusb::watch_devices().map_err(|e| format!("Failed to watch USB devices: {e}"))?;
    let mut watch = std::pin::pin!(watch);

    let state = app.state::<AppState>();
    let mut map = enumerate_rockusb_map().await?;
    publish_map(&app, state.inner(), &map)?;

    loop {
        tokio::select! {
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(HotplugCmd::Resync { reply }) => {
                        let result = async {
                            map = enumerate_rockusb_map().await?;
                            publish_map(&app, state.inner(), &map)
                        }
                        .await;
                        let _ = reply.send(result);
                    }
                    None => break,
                }
            }
            event = watch.next() => {
                match event {
                    Some(HotplugEvent::Connected(info)) => {
                        if is_rockusb_device(&info) {
                            map.insert(info.id(), device_from_info(&info));
                            publish_map(&app, state.inner(), &map)?;
                        }
                    }
                    Some(HotplugEvent::Disconnected(id)) => {
                        if map.remove(&id).is_some() {
                            publish_map(&app, state.inner(), &map)?;
                        }
                    }
                    None => {
                        return Err("USB hotplug stream ended".to_string());
                    }
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn mode_from_bcd_usb_lsb() {
        // Mirrors RKScan.cpp: usbcdUsb & 0x1
        assert_eq!(0x0200u16 & 1, 0); // Maskrom
        assert_eq!(0x0201u16 & 1, 1); // Loader
    }

    #[test]
    fn linux_location_id_encoding() {
        let bus: u8 = 0x5e;
        let port: u8 = 0x31;
        let id = (u32::from(bus) << 8) | u32::from(port);
        assert_eq!(id, 24113);
    }

    #[test]
    fn rockusb_pid_filter() {
        assert!((0x330c_u16 >> 8) > 0);
        assert!((0x350a_u16 >> 8) > 0);
        assert_eq!(0x0010_u16 >> 8, 0); // MSC, excluded
    }
}

use std::sync::Mutex;

/// (location_id, mode, label)
pub type DeviceSnapshot = (String, String, String);

pub struct AppState {
    pub selected_device: Mutex<Option<String>>,
    pub busy: Mutex<bool>,
    pub last_devices: Mutex<Vec<DeviceSnapshot>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            selected_device: Mutex::new(None),
            busy: Mutex::new(false),
            last_devices: Mutex::new(Vec::new()),
        }
    }
}

use afptool_rs::{UpdateHeader, RKAF_SIGNATURE, RKFW_SIGNATURE};
use serde::Serialize;
use std::fs::File;
use std::io::Read;
use std::mem;
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
pub struct FirmwareInfo {
    pub format: String,
    pub firmware_version: String,
    pub loader_version: String,
    pub chip_family: String,
}

pub fn parse_firmware_info(path: &str) -> Result<FirmwareInfo, String> {
    let path = Path::new(path);
    if !path.exists() {
        return Err(format!("File not found: {}", path.display()));
    }

    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).map_err(|e| e.to_string())?;

    if buf.len() < 4 {
        return Err("File is too short to detect firmware format".to_string());
    }

    match &buf[0..4] {
        RKFW_SIGNATURE => parse_rkfw(&buf),
        RKAF_SIGNATURE => parse_rkaf(&buf, path),
        _ => Err("Unsupported firmware format (RKFW or RKAF/update.img required)".to_string()),
    }
}

fn parse_rkfw(buf: &[u8]) -> Result<FirmwareInfo, String> {
    if buf.len() < 0x29 {
        return Err("Incomplete RKFW header".to_string());
    }

    let mut firmware_version = format_rkfw_version(buf);
    if firmware_version == "0.0.0" {
        firmware_version = format_rkfw_build_date(buf);
    }

    let header_chip = rkfw_chip_name(buf[0x15]);
    let chip_family = parse_chip_from_embedded_rkaf(buf)
        .or_else(|| detect_chip_from_rkfw_boot(buf))
        .unwrap_or_else(|| rkfw_chip_fallback(buf, header_chip));

    let loader_version = parse_rkfw_loader_version(buf);

    Ok(FirmwareInfo {
        format: "RKFW".to_string(),
        firmware_version,
        loader_version,
        chip_family,
    })
}

fn parse_rkfw_loader_version(buf: &[u8]) -> String {
    if buf.len() >= 0x21 {
        let boot_off = u32::from_le_bytes([buf[0x19], buf[0x1a], buf[0x1b], buf[0x1c]]) as usize;
        let boot_size = u32::from_le_bytes([buf[0x1d], buf[0x1e], buf[0x1f], buf[0x20]]) as usize;
        if boot_off + boot_size <= buf.len() && boot_size > 0 {
            if let Some(ver) = scan_loader_version(&buf[boot_off..boot_off + boot_size]) {
                return ver;
            }
        }
    }

    if buf.len() >= 0x29 {
        let ioff = u32::from_le_bytes([buf[0x21], buf[0x22], buf[0x23], buf[0x24]]) as usize;
        let isize = u32::from_le_bytes([buf[0x25], buf[0x26], buf[0x27], buf[0x28]]) as usize;
        if ioff + isize <= buf.len() && isize > 4 && &buf[ioff..ioff + 4] == RKAF_SIGNATURE {
            if let Some(ver) = parse_loader_from_rkaf(&buf[ioff..ioff + isize]) {
                return ver;
            }
        }
    }

    "—".to_string()
}

fn parse_rkaf(buf: &[u8], path: &Path) -> Result<FirmwareInfo, String> {
    let header_size = mem::size_of::<UpdateHeader>();
    if buf.len() < header_size {
        return Err("Incomplete RKAF header".to_string());
    }

    let header = UpdateHeader::from_bytes(&buf[..header_size]);
    let manufacturer = cstr_field(&header.manufacturer);
    let model = cstr_field(&header.model);

    let chip_family = chip_from_rkaf_header(buf)
        .or_else(|| detect_chip_from_blob(buf))
        .unwrap_or_else(|| {
        if !manufacturer.is_empty() && manufacturer != "unknown" {
            if manufacturer == model || model.is_empty() {
                manufacturer.clone()
            } else {
                format!("{manufacturer} / {model}")
            }
        } else {
            model.clone()
        }
    });

    let firmware_version = if header.version > 0 {
        format_rkaf_version(header.version)
    } else {
        model.clone()
    };

    let loader_version = parse_loader_from_rkaf(buf).unwrap_or_else(|| "—".to_string());

    if loader_version == "—" {
        // Standalone loader image selected on the download page.
        if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
            let lower = name.to_ascii_lowercase();
            if lower.contains("loader") || lower.ends_with(".bin") {
                return Ok(FirmwareInfo {
                    format: "Loader".to_string(),
                    firmware_version: "—".to_string(),
                    loader_version: parse_loader_from_boot(buf),
                    chip_family,
                });
            }
        }
    }

    Ok(FirmwareInfo {
        format: "RKAF".to_string(),
        firmware_version,
        loader_version,
        chip_family,
    })
}

fn parse_loader_from_rkaf(buf: &[u8]) -> Option<String> {
    let header_size = mem::size_of::<UpdateHeader>();
    if buf.len() < header_size {
        return None;
    }

    let header = UpdateHeader::from_bytes(&buf[..header_size]);
    for i in 0..header.num_parts {
        let part = &header.parts[i as usize];
        let name = cstr_field(&part.name);
        let lower = name.to_ascii_lowercase();
        if !lower.contains("loader") && lower != "boot" {
            continue;
        }

        let offset = part.part_offset as usize;
        let len = part.part_byte_count as usize;
        if offset + len <= buf.len() && len > 0 {
            if let Some(ver) = scan_loader_version(&buf[offset..offset + len]) {
                return Some(ver);
            }
        }
    }
    None
}

fn parse_loader_from_boot(data: &[u8]) -> String {
    scan_loader_version(data).unwrap_or_else(|| "—".to_string())
}

fn scan_loader_version(data: &[u8]) -> Option<String> {
    let sample = &data[..data.len().min(256 * 1024)];
    let text = String::from_utf8_lossy(sample);

    for token in text.split(|c: char| !c.is_ascii_graphic()) {
        if looks_like_loader_version(token) {
            return Some(token.to_string());
        }
    }

    for token in text.split(|c: char| !c.is_ascii_graphic()) {
        if looks_like_build_id(token) {
            return Some(token.to_string());
        }
    }

    if data.len() >= 0x20 {
        let major = data[0x1e];
        let minor = data[0x1f];
        if major > 0 && major < 20 && minor < 100 {
            return Some(format!("{major}.{minor:02}"));
        }
    }

    None
}

fn looks_like_build_id(token: &str) -> bool {
    token.len() >= 5
        && token.len() <= 12
        && token.chars().all(|c| c.is_ascii_digit())
}

fn format_rkfw_build_date(buf: &[u8]) -> String {
    if buf.len() < 0x15 {
        return "0.0.0".to_string();
    }
    let year = ((buf[0x0f] as u16) << 8) | buf[0x0e] as u16;
    let month = buf[0x10];
    let day = buf[0x11];
    format!("{year}.{month:02}.{day:02}")
}

fn rkfw_embedded_slice(buf: &[u8]) -> Option<&[u8]> {
    if buf.len() < 0x29 {
        return None;
    }
    let embed_off =
        u32::from_le_bytes([buf[0x21], buf[0x22], buf[0x23], buf[0x24]]) as usize;
    let embed_size =
        u32::from_le_bytes([buf[0x25], buf[0x26], buf[0x27], buf[0x28]]) as usize;
    if embed_off + 4 > buf.len() || embed_size < 4 || &buf[embed_off..embed_off + 4] != RKAF_SIGNATURE
    {
        return None;
    }
    let end = (embed_off + embed_size).min(buf.len());
    Some(&buf[embed_off..end])
}

fn rkfw_chip_fallback(buf: &[u8], header_chip: String) -> String {
    if buf.get(0x15) == Some(&0x36) && rkfw_embedded_slice(buf).is_some_and(empty_rkaf_identity) {
        // Luckfox 等部分 RV1106 固件的 RKAF 头不含 model/manufacturer
        return "RV1106".into();
    }
    header_chip
}

fn empty_rkaf_identity(embed: &[u8]) -> bool {
    chip_from_rkaf_header(embed).is_none()
}

fn parse_chip_from_embedded_rkaf(buf: &[u8]) -> Option<String> {
    rkfw_embedded_slice(buf).and_then(chip_from_rkaf_header)
}

fn detect_chip_from_rkfw_boot(buf: &[u8]) -> Option<String> {
    if buf.len() < 0x21 {
        return None;
    }
    let boot_off =
        u32::from_le_bytes([buf[0x19], buf[0x1a], buf[0x1b], buf[0x1c]]) as usize;
    let boot_size =
        u32::from_le_bytes([buf[0x1d], buf[0x1e], buf[0x1f], buf[0x20]]) as usize;
    if boot_off + boot_size > buf.len() || boot_size == 0 {
        return None;
    }
    detect_chip_from_blob(&buf[boot_off..boot_off + boot_size])
}

fn chip_from_rkaf_header(buf: &[u8]) -> Option<String> {
    let header_size = mem::size_of::<UpdateHeader>();
    if buf.len() < header_size {
        return None;
    }
    let header = UpdateHeader::from_bytes(&buf[..header_size]);
    let manufacturer = cstr_field(&header.manufacturer).trim().to_string();
    let model = cstr_field(&header.model).trim().to_string();

    if !model.is_empty() && model != "unknown" {
        return Some(model);
    }
    if !manufacturer.is_empty() && manufacturer != "unknown" {
        return Some(manufacturer);
    }
    None
}
fn detect_chip_from_blob(buf: &[u8]) -> Option<String> {
    const CHIPS: &[&str] = &[
        "RK3588", "RK3576", "RK3568", "RK3566", "RK3562", "RK3399", "RK3326", "RV1106",
        "RV1126", "RV1103", "PX30",
    ];

    let sample = &buf[..buf.len().min(4 * 1024 * 1024)];
    let text = String::from_utf8_lossy(sample);
    for chip in CHIPS {
        if text.contains(chip) {
            return Some((*chip).to_string());
        }
    }
    None
}

fn looks_like_loader_version(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return false;
    }
    parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
        && token.len() <= 12
}

fn format_rkfw_version(buf: &[u8]) -> String {
    format!(
        "{}.{}.{}",
        buf[9],
        buf[8],
        ((buf[7] as u16) << 8) + buf[6] as u16
    )
}

fn format_rkaf_version(version: u32) -> String {
    format!(
        "{}.{}.{}",
        (version >> 16) & 0xff,
        (version >> 8) & 0xff,
        version & 0xff
    )
}

fn rkfw_chip_name(code: u8) -> String {
    match code {
        0x19 => "RV1109/RV1126".into(),
        0x30 => "PX30/RK3326".into(),
        0x32 => "RK3562".into(),
        0x33 => "RK3399/RK3399Pro".into(),
        0x35 => "RK3588/RK3588S".into(),
        0x36 => "RK3326".into(),
        0x38 => "RK3566/RK3568".into(),
        0x39 => "RK3528".into(),
        0x41 => "RK3368".into(),
        0x48 => "RK3308".into(),
        0x50 => "RK29xx".into(),
        0x51 => "RV1108".into(),
        0x60 => "RK30xx/RK3066".into(),
        0x70 => "RK31xx/RK3188".into(),
        0x80 => "RK32xx/RK3288".into(),
        other => format!("Unknown (0x{other:02x})"),
    }
}

fn cstr_field(bytes: &[u8]) -> String {
    std::ffi::CStr::from_bytes_until_nul(bytes)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rkfw_version_format() {
        let buf = [0u8; 16];
        let mut sample = buf.to_vec();
        sample[6] = 0;
        sample[7] = 1;
        sample[8] = 0;
        sample[9] = 1;
        assert_eq!(format_rkfw_version(&sample), "1.0.256");
    }
}

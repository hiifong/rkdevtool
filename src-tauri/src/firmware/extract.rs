use afptool_rs::{
    recover_true_size, UpdateHeader, RKAFP_MAGIC, RKAF_SIGNATURE, UPDATE_HEADER_SIZE,
};
use std::ffi::CStr;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path, PathBuf};

const RKAF_SECTOR_SIZE: u64 = 2048;
const U32_RANGE: u64 = 1u64 << 32;
const RKFW_HEADER_SIZE: usize = 0x29;

#[derive(Debug, Clone)]
pub struct FirmwareImage {
    pub name: String,
    pub path: PathBuf,
    pub flash_offset_sectors: u64,
    pub flash_size_sectors: u64,
    pub byte_count: u64,
}

#[derive(Debug, Clone)]
pub struct ExtractedFirmware {
    pub images: Vec<FirmwareImage>,
    pub loader_path: Option<PathBuf>,
    pub log: String,
}

pub fn extract_firmware_file(path: &str, output_dir: &str) -> Result<String, String> {
    Ok(extract_firmware(path, output_dir, false)?.log)
}

pub fn extract_firmware_for_upgrade(
    path: &str,
    output_dir: &str,
) -> Result<ExtractedFirmware, String> {
    extract_firmware(path, output_dir, true)
}

fn extract_firmware(
    path: &str,
    output_dir: &str,
    keep_boot_file: bool,
) -> Result<ExtractedFirmware, String> {
    let path = Path::new(path);
    if !path.is_file() {
        return Err(format!("File not found: {}", path.display()));
    }

    std::fs::create_dir_all(output_dir).map_err(|e| format!("Failed to create output directory: {e}"))?;

    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let mut signature = [0u8; 4];
    file.read_exact(&mut signature).map_err(|e| e.to_string())?;

    let mut log = String::new();

    let mut images = Vec::new();
    let mut loader_path = None;

    match signature {
        [b'R', b'K', b'A', b'F'] => {
            unpack_rkaf(path, output_dir, &mut log, &mut images, &mut loader_path)?
        }
        [b'R', b'K', b'F', b'W'] => {
            unpack_rkfw(path, output_dir, &mut log)?;

            let embedded = Path::new(output_dir).join("embedded-update.img");
            if embedded.exists() {
                unpack_rkaf(
                    &embedded,
                    output_dir,
                    &mut log,
                    &mut images,
                    &mut loader_path,
                )?;
                let _ = std::fs::remove_file(&embedded);
                let boot_path = Path::new(output_dir).join("BOOT");
                if loader_path.is_none() && boot_path.is_file() {
                    loader_path = Some(boot_path.clone());
                }
                if !keep_boot_file {
                    let _ = std::fs::remove_file(boot_path);
                }
            }
        }
        _ => return Err("Unsupported firmware format (RKFW or RKAF/update.img required)".to_string()),
    }

    if images.is_empty() {
        return Err("Firmware package contains no writable images".to_string());
    }

    Ok(ExtractedFirmware {
        images,
        loader_path,
        log,
    })
}

fn push_log(log: &mut String, line: impl AsRef<str>) {
    log.push_str(line.as_ref());
    log.push('\n');
}

fn unpack_rkfw(path: &Path, output_dir: &str, log: &mut String) -> Result<(), String> {
    let mut fp = File::open(path).map_err(|e| e.to_string())?;
    let filesize = fp.metadata().map_err(|e| e.to_string())?.len();
    let mut header = [0u8; RKFW_HEADER_SIZE];
    fp.read_exact(&mut header)
        .map_err(|_| "Incomplete RKFW header".to_string())?;

    push_log(log, "RKFW signature detected");

    let version = format!(
        "{}.{}.{}",
        header[9],
        header[8],
        u16::from_le_bytes([header[6], header[7]])
    );
    push_log(log, &format!("version: {version}"));

    let boot_offset = u64::from(u32::from_le_bytes(header[0x19..0x1d].try_into().unwrap()));
    let boot_size = u64::from(u32::from_le_bytes(header[0x1d..0x21].try_into().unwrap()));
    let update_offset = u64::from(u32::from_le_bytes(header[0x21..0x25].try_into().unwrap()));
    let stored_update_size = u32::from_le_bytes(header[0x25..0x29].try_into().unwrap());

    ensure_region_in_file(boot_offset, boot_size, filesize, "RKFW Boot")?;

    let boot_path = Path::new(output_dir).join("BOOT");
    push_log(
        log,
        format!(
            "{boot_offset:08x}-{end:08x} {path:26} (size: {boot_size})",
            end = boot_offset + boot_size.saturating_sub(1),
            path = boot_path.display(),
        ),
    );
    copy_file_region(&mut fp, boot_offset, boot_size, &boot_path)?;

    let data_end = rkfw_data_end(&mut fp, filesize, update_offset)?;
    let update_size = recover_rkfw_embedded_size(stored_update_size, data_end, update_offset)?;
    ensure_region_in_file(update_offset, update_size, data_end, "RKFW embedded update.img")?;

    let mut embedded_signature = [0u8; 4];
    fp.seek(SeekFrom::Start(update_offset))
        .map_err(|e| e.to_string())?;
    fp.read_exact(&mut embedded_signature).map_err(|e| e.to_string())?;
    if embedded_signature != *RKAF_SIGNATURE {
        return Err("RKFW does not contain embedded RKAF update.img".to_string());
    }

    if update_size != u64::from(stored_update_size) {
        push_log(
            log,
            format!(
                "Recovered embedded update.img size: {update_size} bytes (header stores low 32 bits 0x{stored_update_size:08x})"
            ),
        );
    }

    let embedded_path = Path::new(output_dir).join("embedded-update.img");
    push_log(
        log,
        format!(
            "{update_offset:08x}-{end:08x} {path:26} (size: {update_size})",
            end = update_offset + update_size.saturating_sub(1),
            path = embedded_path.display(),
        ),
    );
    copy_file_region(&mut fp, update_offset, update_size, &embedded_path)?;

    Ok(())
}

fn unpack_rkaf(
    path: &Path,
    output_dir: &str,
    log: &mut String,
    images: &mut Vec<FirmwareImage>,
    loader_path: &mut Option<PathBuf>,
) -> Result<(), String> {
    let mut fp = File::open(path).map_err(|e| e.to_string())?;
    let mut header_buf = vec![0u8; UPDATE_HEADER_SIZE];
    fp.read_exact(&mut header_buf).map_err(|e| e.to_string())?;

    let header =
        UpdateHeader::decode(&header_buf).map_err(|e| format!("Invalid RKAF header: {e}"))?;
    let magic_str = std::str::from_utf8(&header.magic).map_err(|e| e.to_string())?;
    if magic_str != RKAFP_MAGIC {
        return Err("Invalid RKAF header".to_string());
    }

    let filesize = fp.metadata().map_err(|e| e.to_string())?.len();
    let container_end = filesize
        .checked_sub(4)
        .ok_or_else(|| "RKAF container is missing its trailing CRC".to_string())?;
    let partition_layout = recover_rkaf_part_layout(&header, container_end)?;
    push_log(log, &format!("Filesize: {filesize}"));
    if container_end > u64::from(u32::MAX) {
        push_log(
            log,
            "RKAF container exceeds 4 GiB; recovering wrapped header offsets and sizes",
        );
    }

    let manufacturer = cstr_field(&header.manufacturer);
    let model = cstr_field(&header.model);
    push_log(log, &format!("manufacturer: {manufacturer}"));
    push_log(log, &format!("model: {model}"));

    let metadata_path = Path::new(output_dir).join("partition-metadata.txt");
    let mut metadata_file = File::create(&metadata_path).map_err(|e| e.to_string())?;

    let num_parts = header.num_parts;
    for index in 0..num_parts {
        let part = &header.parts[index as usize];
        let (part_offset, part_byte_count) = partition_layout[index as usize];
        let part_full_path = match CStr::from_bytes_until_nul(&part.full_path) {
            Ok(value) => value.to_string_lossy().into_owned(),
            Err(_) => continue,
        };

        if part_full_path.is_empty() {
            continue;
        }

        let part_name = CStr::from_bytes_until_nul(&part.name)
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let flash_size = part.flash_size;
        let flash_offset = part.flash_offset;
        let stored_part_offset = part.part_offset;
        let padded_size = part.padded_size;
        let stored_part_byte_count = part.part_byte_count;

        writeln!(
            metadata_file,
            "{},{},{:#010x},{:#010x},{:#010x},{:#010x},{:#010x}",
            part_name,
            part_full_path,
            flash_size,
            flash_offset,
            stored_part_offset,
            padded_size,
            stored_part_byte_count,
        )
        .map_err(|e| e.to_string())?;

        if part_full_path == "SELF" || part_full_path == "RESERVED" {
            continue;
        }

        if part_offset != u64::from(stored_part_offset) {
            push_log(
                log,
                format!(
                    "Recovered {} offset: {part_offset} bytes (header stores low 32 bits 0x{stored_part_offset:08x})",
                    part_full_path
                ),
            );
        }
        if part_byte_count != u64::from(stored_part_byte_count) {
            push_log(
                log,
                format!(
                    "Recovered {} size: {part_byte_count} bytes (header stores low 32 bits 0x{stored_part_byte_count:08x})",
                    part_full_path
                ),
            );
        }

        let relative_path = safe_relative_path(&part_full_path)?;
        let output_path = Path::new(output_dir).join(relative_path);
        extract_part(
            &mut fp,
            part_offset,
            part_byte_count,
            &output_path,
            log,
        )?;

        if is_loader_file_name(&part_full_path) {
            *loader_path = Some(output_path.clone());
        }

        if !has_flash_target(flash_offset) {
            continue;
        }

        images.push(FirmwareImage {
            name: if part_name.trim().is_empty() {
                part_full_path.clone()
            } else {
                part_name
            },
            path: output_path,
            flash_offset_sectors: u64::from(flash_offset),
            flash_size_sectors: u64::from(flash_size),
            byte_count: part_byte_count,
        });
    }

    push_log(
        log,
        format!("\nPartition metadata saved to: {}", metadata_path.display()),
    );

    Ok(())
}

/// Recover 64-bit RKAF member offsets and byte counts from their on-disk u32 fields.
/// `padded_size` is expressed in 2048-byte sectors and retains the allocation size.
fn recover_rkaf_part_layout(
    header: &UpdateHeader,
    container_end: u64,
) -> Result<Vec<(u64, u64)>, String> {
    let mut layout = Vec::with_capacity(header.num_parts as usize);
    let mut known_members = Vec::new();
    let mut previous_allocation_end = 0u64;

    for part in header.parts.iter().take(header.num_parts as usize) {
        let raw_offset = u64::from(part.part_offset);
        if raw_offset == 0 {
            layout.push((0, u64::from(part.part_byte_count)));
            continue;
        }

        if let Some((_, _, offset, byte_count)) = known_members.iter().find(
            |(stored_offset, stored_path, _, _)| {
                *stored_offset == part.part_offset && *stored_path == part.full_path
            },
        ) {
            layout.push((*offset, *byte_count));
            continue;
        }

        let padded_bytes = u64::from(part.padded_size)
            .checked_mul(RKAF_SECTOR_SIZE)
            .ok_or_else(|| "RKAF partition padded size overflows u64".to_string())?;
        let byte_count = if padded_bytes == 0 {
            u64::from(part.part_byte_count)
        } else {
            recover_true_size(part.part_byte_count, padded_bytes)
        };
        if padded_bytes != 0 && byte_count > padded_bytes {
            return Err(format!(
                "RKAF partition size {byte_count} exceeds its padded allocation {padded_bytes}"
            ));
        }

        let allocation_size = if padded_bytes == 0 {
            byte_count
                .checked_add(RKAF_SECTOR_SIZE - 1)
                .ok_or_else(|| "RKAF partition allocation size overflows u64".to_string())?
                / RKAF_SECTOR_SIZE
                * RKAF_SECTOR_SIZE
        } else {
            padded_bytes
        };

        let mut offset = raw_offset;
        if offset < previous_allocation_end {
            let distance = previous_allocation_end - offset;
            let wraps = distance / U32_RANGE + u64::from(distance % U32_RANGE != 0);
            offset = offset
                .checked_add(
                    wraps
                        .checked_mul(U32_RANGE)
                        .ok_or_else(|| "RKAF partition offset overflows u64".to_string())?,
                )
                .ok_or_else(|| "RKAF partition offset overflows u64".to_string())?;
        }

        let data_end = offset
            .checked_add(byte_count)
            .ok_or_else(|| "RKAF partition data range overflows u64".to_string())?;
        if data_end > container_end {
            return Err(format!(
                "RKAF partition range exceeds container: offset {offset}, size {byte_count}, container end {container_end}"
            ));
        }
        let allocation_end = offset
            .checked_add(allocation_size)
            .ok_or_else(|| "RKAF partition allocation range overflows u64".to_string())?;

        layout.push((offset, byte_count));
        known_members.push((part.part_offset, part.full_path, offset, byte_count));
        previous_allocation_end = allocation_end;
    }

    Ok(layout)
}

fn recover_rkfw_embedded_size(
    stored_size: u32,
    data_end: u64,
    update_offset: u64,
) -> Result<u64, String> {
    let available = data_end
        .checked_sub(update_offset)
        .ok_or_else(|| "RKFW embedded update.img offset is out of file range".to_string())?;
    let byte_count = recover_true_size(stored_size, available);
    if byte_count > available {
        return Err(format!(
            "RKFW embedded update.img size {byte_count} exceeds available data {available}"
        ));
    }
    Ok(byte_count)
}

fn rkfw_data_end(fp: &mut File, filesize: u64, update_offset: u64) -> Result<u64, String> {
    if update_offset > filesize {
        return Err("RKFW embedded update.img offset is out of file range".to_string());
    }
    if filesize < 32 || update_offset > filesize - 32 {
        return Ok(filesize);
    }

    let mut tail = [0u8; 32];
    fp.seek(SeekFrom::End(-32)).map_err(|e| e.to_string())?;
    fp.read_exact(&mut tail).map_err(|e| e.to_string())?;
    Ok(if tail.iter().all(u8::is_ascii_hexdigit) {
        filesize - 32
    } else {
        filesize
    })
}

fn ensure_region_in_file(offset: u64, len: u64, file_end: u64, label: &str) -> Result<(), String> {
    let end = offset
        .checked_add(len)
        .ok_or_else(|| format!("{label} region overflows u64"))?;
    if end > file_end {
        return Err(format!("{label} region is out of file range"));
    }
    Ok(())
}

fn safe_relative_path(full_path: &str) -> Result<PathBuf, String> {
    let normalized = full_path.replace('\\', "/");
    let mut output = PathBuf::new();

    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => output.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(format!("Unsafe firmware entry path: {full_path}"));
            }
        }
    }

    if output.as_os_str().is_empty() {
        return Err("Firmware entry path is empty".to_string());
    }
    Ok(output)
}

pub(crate) fn is_loader_file_name(path: &str) -> bool {
    let normalized = path.replace('\\', "/");
    let file_name = Path::new(&normalized)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    file_name.eq_ignore_ascii_case("download.bin")
        || file_name.eq_ignore_ascii_case("MiniLoaderAll.bin")
}

fn has_flash_target(flash_offset: u32) -> bool {
    flash_offset != u32::MAX
}

fn extract_part(
    fp: &mut File,
    offset: u64,
    len: u64,
    output_path: &Path,
    log: &mut String,
) -> Result<(), String> {
    push_log(
        log,
        format!(
            "{offset:08x}-{end:08x} {path}",
            end = offset + len.saturating_sub(1),
            path = output_path.display(),
        ),
    );

    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    copy_file_region(fp, offset, len, output_path)
}

fn copy_file_region(
    fp: &mut File,
    offset: u64,
    len: u64,
    output_path: &Path,
) -> Result<(), String> {
    if let Some(parent) = output_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
    }

    let mut buffer = vec![0u8; 1024 * 1024];
    let mut output = File::create(output_path).map_err(|e| e.to_string())?;
    fp.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;

    let mut remaining = len;
    while remaining > 0 {
        let read_len = remaining.min(buffer.len() as u64) as usize;
        fp.read_exact(&mut buffer[..read_len])
            .map_err(|e| e.to_string())?;
        output
            .write_all(&buffer[..read_len])
            .map_err(|e| e.to_string())?;
        remaining -= read_len as u64;
    }

    Ok(())
}

fn cstr_field(bytes: &[u8]) -> String {
    CStr::from_bytes_until_nul(bytes)
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(test)]
mod tests {
    use super::{
        has_flash_target, is_loader_file_name, recover_rkaf_part_layout,
        recover_rkfw_embedded_size, safe_relative_path,
    };
    use afptool_rs::{UpdateHeader, UpdatePart};
    use std::path::PathBuf;

    fn part(offset: u32, padded_sectors: u32, byte_count: u32) -> UpdatePart {
        let mut part = UpdatePart::default();
        part.part_offset = offset;
        part.padded_size = padded_sectors;
        part.part_byte_count = byte_count;
        part
    }

    #[test]
    fn recognizes_the_supported_loader_filenames() {
        assert!(is_loader_file_name("download.bin"));
        assert!(is_loader_file_name("MiniLoaderAll.bin"));
        assert!(is_loader_file_name("MINILOADERALL.BIN"));
        assert!(!is_loader_file_name("uboot.img"));
    }

    #[test]
    fn excludes_entries_without_a_flash_target_from_lba_writes() {
        assert!(has_flash_target(0x4000));
        assert!(!has_flash_target(u32::MAX));
    }

    #[test]
    fn rejects_firmware_paths_that_escape_the_output_directory() {
        assert!(safe_relative_path("../outside.img").is_err());
        assert!(safe_relative_path("/absolute.img").is_err());
        assert_eq!(safe_relative_path("Image/boot.img").unwrap(), PathBuf::from("Image/boot.img"));
    }

    #[test]
    fn recovers_layout_after_a_partition_crosses_four_gib() {
        const FIVE_GIB: u64 = 5 * 1024 * 1024 * 1024;
        const SECTOR_SIZE: u64 = 2048;
        let first_offset = 0x800u64;
        let second_offset = first_offset + FIVE_GIB;
        let mut header = UpdateHeader::default();
        header.num_parts = 2;
        header.parts[0] = part(
            first_offset as u32,
            (FIVE_GIB / SECTOR_SIZE) as u32,
            FIVE_GIB as u32,
        );
        header.parts[1] = part(second_offset as u32, 2, 4096);

        let layout = recover_rkaf_part_layout(&header, second_offset + 4096).unwrap();

        assert_eq!(layout[0], (first_offset, FIVE_GIB));
        assert_eq!(layout[1], (second_offset, 4096));
    }

    #[test]
    fn recovers_wrapped_rkfw_embedded_image_size() {
        const FIVE_GIB: u64 = 5 * 1024 * 1024 * 1024;
        let update_offset = 0x1000u64;

        assert_eq!(
            recover_rkfw_embedded_size(FIVE_GIB as u32, update_offset + FIVE_GIB, update_offset)
                .unwrap(),
            FIVE_GIB
        );
    }
}

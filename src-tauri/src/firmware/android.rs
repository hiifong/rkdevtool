const SECTOR_SIZE: usize = 512;
const GPT_ENTRY_SIZE: usize = 128;
const GPT_ENTRY_COUNT: usize = 128;
const GPT_ENTRY_SECTORS: u64 = (GPT_ENTRY_SIZE * GPT_ENTRY_COUNT / SECTOR_SIZE) as u64;
const GPT_PRIMARY_SECTORS: u64 = 2 + GPT_ENTRY_SECTORS;
const GPT_BACKUP_SECTORS: u64 = 1 + GPT_ENTRY_SECTORS;
const ROCKCHIP_PARAMETER_MAGIC: &[u8; 4] = b"PARM";

const BASIC_DATA_GUID: [u8; 16] = [
    0xa2, 0xa0, 0xd0, 0xeb, 0xe5, 0xb9, 0x33, 0x44, 0x87, 0xc0, 0x68, 0xb6, 0xb7, 0x26, 0x99, 0xc7,
];
const DISK_GUID: [u8; 16] = [
    0x3f, 0x83, 0x30, 0x5a, 0x43, 0x8c, 0xb1, 0x47, 0xa4, 0x1d, 0x84, 0xd4, 0x08, 0x83, 0xd0, 0xc6,
];

/// Rockchip per-partition GPT GUIDs (raw entry bytes: type GUID, unique GUID).
/// Byte-identical to upgrade_tool v2.44 output on RK3506 SPI-NAND and stable
/// across firmware builds that share the partition layout. Unknown names fall
/// back to the generic BASIC_DATA_GUID entry.
const ROCKCHIP_PARTITION_GUIDS: &[(&str, [u8; 16], [u8; 16])] = &[
    (
        "vnvm",
        [
            0x18, 0xe4, 0x24, 0x11, 0x08, 0x90, 0xf8, 0x41, 0x9e, 0x3d, 0x8d, 0x87, 0x22, 0x16,
            0xc8, 0xa1,
        ],
        [
            0x41, 0x77, 0xad, 0x67, 0x38, 0x46, 0xc6, 0x47, 0xa4, 0x88, 0xd3, 0x96, 0x58, 0x3e,
            0x58, 0x2a,
        ],
    ),
    (
        "uboot",
        [
            0x6a, 0x48, 0xd4, 0xfb, 0x2c, 0xe4, 0x8d, 0x44, 0xd4, 0xe9, 0x7f, 0x5c, 0x32, 0x4a,
            0x78, 0x43,
        ],
        [
            0x3d, 0xa0, 0xde, 0x3a, 0x12, 0xc6, 0xed, 0x46, 0xcf, 0xf9, 0xb7, 0x94, 0x39, 0xf1,
            0x15, 0x61,
        ],
    ),
    (
        "misc",
        [
            0x47, 0x4f, 0xaf, 0xbb, 0x0d, 0x23, 0xb6, 0x45, 0xfe, 0xae, 0xe4, 0xb3, 0x5f, 0x30,
            0x0b, 0x00,
        ],
        [
            0x4a, 0x48, 0x82, 0xe5, 0x10, 0x90, 0x4a, 0x40, 0x89, 0x9d, 0x47, 0xbd, 0x16, 0xba,
            0x3f, 0xa8,
        ],
    ),
    (
        "recovery",
        [
            0x54, 0xd3, 0xc5, 0xc5, 0x0b, 0x44, 0x3e, 0x45, 0x88, 0x32, 0xd3, 0x63, 0x49, 0xb6,
            0xee, 0x24,
        ],
        [
            0x7f, 0x27, 0x11, 0xdc, 0x7f, 0xeb, 0xd9, 0x42, 0x80, 0x27, 0xa8, 0x3b, 0x14, 0x22,
            0x0e, 0x50,
        ],
    ),
    (
        "boot",
        [
            0x2d, 0xb7, 0xc3, 0x5c, 0x57, 0xc1, 0x84, 0x49, 0xa6, 0x01, 0x30, 0xf5, 0x5f, 0x80,
            0xac, 0x1c,
        ],
        [
            0x6a, 0x72, 0x5a, 0x29, 0x7d, 0x35, 0x82, 0x43, 0xd5, 0xa6, 0xe7, 0xe8, 0x71, 0xda,
            0x99, 0xe0,
        ],
    ),
    (
        "rootfs",
        [
            0x50, 0x54, 0xea, 0x34, 0x2a, 0x9f, 0x1b, 0x47, 0x9c, 0x58, 0x28, 0x05, 0x23, 0xe6,
            0xfe, 0x2b,
        ],
        [
            0x00, 0x00, 0x4e, 0x61, 0x00, 0x00, 0x53, 0x4b, 0x80, 0x00, 0x1d, 0x28, 0x00, 0x00,
            0x54, 0xa9,
        ],
    ),
    (
        "oem",
        [
            0x5b, 0x5d, 0xe3, 0x9e, 0x34, 0x15, 0x8d, 0x49, 0xac, 0xa1, 0x2e, 0xa9, 0x5d, 0x08,
            0xaf, 0xb2,
        ],
        [
            0x75, 0xa6, 0xc6, 0x48, 0x44, 0x37, 0xf6, 0x45, 0xd7, 0xff, 0x47, 0x44, 0x49, 0xee,
            0xb9, 0x8e,
        ],
    ),
    (
        "userdata",
        [
            0x63, 0xe9, 0xb6, 0x9b, 0x7d, 0x75, 0xf3, 0x46, 0x98, 0xdf, 0x9d, 0xcd, 0x61, 0x7b,
            0x9e, 0x2d,
        ],
        [
            0x31, 0x1b, 0xe7, 0x47, 0x6e, 0x0d, 0x8c, 0x4c, 0xa6, 0xd4, 0x2e, 0xf6, 0x3f, 0xc0,
            0x36, 0x6c,
        ],
    ),
];

fn rockchip_partition_guids(name: &str) -> Option<([u8; 16], [u8; 16])> {
    let lower = name.to_ascii_lowercase();
    ROCKCHIP_PARTITION_GUIDS
        .iter()
        .find(|(entry_name, _, _)| *entry_name == lower)
        .map(|(_, type_guid, unique_guid)| (*type_guid, *unique_guid))
}
const SPARSE_MAGIC: u32 = 0xed26_ff3a;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SparseChunkKind {
    Raw,
    Fill,
    DontCare,
    Crc32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SparseHeader {
    pub file_header_size: usize,
    pub chunk_header_size: usize,
    pub block_size: u64,
    pub total_chunks: u32,
    pub output_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SparseChunk {
    pub kind: SparseChunkKind,
    pub output_bytes: u64,
    pub payload_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GptPartition {
    pub name: String,
    pub start_sector: u64,
    pub sector_count: Option<u64>,
    pub unique_guid: Option<[u8; 16]>,
}

#[derive(Debug, Clone)]
pub(crate) struct GptTables {
    pub primary: Vec<u8>,
    pub backup_start_sector: u32,
    pub backup: Vec<u8>,
}

fn parameter_payload(data: &[u8]) -> Result<&[u8], String> {
    if !data.starts_with(ROCKCHIP_PARAMETER_MAGIC) {
        return Ok(data);
    }
    if data.len() < 8 {
        return Err("Incomplete Rockchip PARM header".to_string());
    }

    let payload_len = u32::from_le_bytes(data[4..8].try_into().unwrap()) as usize;
    let payload_end = 8usize
        .checked_add(payload_len)
        .ok_or_else(|| "Rockchip PARM payload length overflows".to_string())?;
    if payload_end <= data.len() {
        return Ok(&data[8..payload_end]);
    }

    // Older raw parameter inputs use PARM as a NUL-terminated marker rather than a
    // length-prefixed header. Keep accepting that representation.
    if data[4] == 0 && data[5..8].iter().all(u8::is_ascii_uppercase) {
        return Ok(data);
    }

    Err(format!(
        "Rockchip PARM payload length {payload_len} exceeds available data ({})",
        data.len() - 8
    ))
}

pub(crate) fn parse_gpt_parameter(data: &[u8]) -> Result<Option<Vec<GptPartition>>, String> {
    let payload = parameter_payload(data)?;
    let text = String::from_utf8_lossy(payload);
    if !text.contains("TYPE: GPT") {
        return Ok(None);
    }

    parse_parameter_partitions(payload)?
        .ok_or_else(|| "GPT parameter has no CMDLINE".to_string())
        .map(Some)
}

/// Parse Rockchip's mtdparts command line from either a legacy or GPT parameter image.
pub(crate) fn parse_parameter_partitions(data: &[u8]) -> Result<Option<Vec<GptPartition>>, String> {
    let payload = parameter_payload(data)?;
    let text = String::from_utf8_lossy(payload);
    let Some(cmdline_start) = text.find("CMDLINE:") else {
        return Ok(None);
    };

    let cmdline = text[cmdline_start + "CMDLINE:".len()..]
        .split('\0')
        .next()
        .unwrap_or_default();
    let (_, entries) = cmdline
        .split_once(':')
        .ok_or_else(|| "GPT parameter CMDLINE has no mtdparts entries".to_string())?;

    let partition_guids = parse_parameter_partition_guids(&text)?;
    let mut partitions = Vec::new();
    for entry in entries.split(',') {
        let (size, rest) = entry
            .trim()
            .split_once('@')
            .ok_or_else(|| format!("Invalid GPT partition entry: {entry}"))?;
        let (offset, name) = rest
            .split_once('(')
            .ok_or_else(|| format!("Invalid GPT partition entry: {entry}"))?;
        let name = name
            .split_once(')')
            .map(|(name, _)| name)
            .ok_or_else(|| format!("Invalid GPT partition entry: {entry}"))?
            .split(':')
            .next()
            .unwrap_or_default()
            .trim();
        if name.is_empty() || !name.is_ascii() {
            return Err(format!("Invalid GPT partition name: {name}"));
        }

        let sector_count = if size.trim() == "-" {
            None
        } else {
            Some(parse_hex_sector(size.trim())?)
        };
        let normalized_name = name.to_ascii_lowercase();
        partitions.push(GptPartition {
            name: name.to_string(),
            start_sector: parse_hex_sector(offset.trim())?,
            sector_count,
            unique_guid: partition_guids.get(&normalized_name).copied(),
        });
    }

    if partitions.is_empty() {
        return Err("Parameter has no partitions".to_string());
    }
    Ok(Some(partitions))
}

/// Read Rockchip parameter entries such as `uuid:rootfs=614e0000-...`.
/// GPT stores the first three UUID fields as little-endian integers.
fn parse_parameter_partition_guids(
    text: &str,
) -> Result<std::collections::HashMap<String, [u8; 16]>, String> {
    let mut guids = std::collections::HashMap::new();

    for line in text.split(['\0', '\n', '\r']) {
        let Some((name, value)) = line
            .trim()
            .strip_prefix("uuid:")
            .and_then(|entry| entry.split_once('='))
        else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || !name.is_ascii() {
            return Err(format!("Invalid GPT partition UUID name: {name}"));
        }
        guids.insert(name.to_ascii_lowercase(), parse_gpt_guid(value.trim())?);
    }

    Ok(guids)
}

fn parse_gpt_guid(value: &str) -> Result<[u8; 16], String> {
    let groups: Vec<_> = value.split('-').collect();
    let expected_lengths = [8, 4, 4, 4, 12];
    if groups.len() != expected_lengths.len()
        || groups
            .iter()
            .zip(expected_lengths)
            .any(|(group, length)| group.len() != length)
    {
        return Err(format!("Invalid GPT partition UUID: {value}"));
    }

    let first = u32::from_str_radix(groups[0], 16)
        .map_err(|_| format!("Invalid GPT partition UUID: {value}"))?;
    let second = u16::from_str_radix(groups[1], 16)
        .map_err(|_| format!("Invalid GPT partition UUID: {value}"))?;
    let third = u16::from_str_radix(groups[2], 16)
        .map_err(|_| format!("Invalid GPT partition UUID: {value}"))?;

    let mut guid = [0u8; 16];
    guid[0..4].copy_from_slice(&first.to_le_bytes());
    guid[4..6].copy_from_slice(&second.to_le_bytes());
    guid[6..8].copy_from_slice(&third.to_le_bytes());
    for (index, pair) in format!("{}{}", groups[3], groups[4])
        .as_bytes()
        .chunks_exact(2)
        .enumerate()
    {
        let hex = std::str::from_utf8(pair)
            .map_err(|_| format!("Invalid GPT partition UUID: {value}"))?;
        guid[8 + index] = u8::from_str_radix(hex, 16)
            .map_err(|_| format!("Invalid GPT partition UUID: {value}"))?;
    }
    Ok(guid)
}

pub(crate) fn parse_sparse_header(data: &[u8]) -> Result<Option<SparseHeader>, String> {
    if data.len() < 4 || u32::from_le_bytes(data[..4].try_into().unwrap()) != SPARSE_MAGIC {
        return Ok(None);
    }
    if data.len() < 28 {
        return Err("Incomplete Android sparse header".to_string());
    }
    let major_version = u16::from_le_bytes(data[4..6].try_into().unwrap());
    let file_header_size = u16::from_le_bytes(data[8..10].try_into().unwrap()) as usize;
    let chunk_header_size = u16::from_le_bytes(data[10..12].try_into().unwrap()) as usize;
    let block_size = u64::from(u32::from_le_bytes(data[12..16].try_into().unwrap()));
    let total_blocks = u64::from(u32::from_le_bytes(data[16..20].try_into().unwrap()));
    let total_chunks = u32::from_le_bytes(data[20..24].try_into().unwrap());

    if major_version != 1 {
        return Err(format!(
            "Unsupported Android sparse major version: {major_version}"
        ));
    }
    if file_header_size < 28 || chunk_header_size < 12 {
        return Err("Invalid Android sparse header size".to_string());
    }
    if block_size == 0 || block_size % SECTOR_SIZE as u64 != 0 {
        return Err("Android sparse block size is not a multiple of 512".to_string());
    }
    if total_blocks == 0 || total_chunks == 0 {
        return Err("Android sparse image has no output blocks".to_string());
    }

    Ok(Some(SparseHeader {
        file_header_size,
        chunk_header_size,
        block_size,
        total_chunks,
        output_bytes: total_blocks
            .checked_mul(block_size)
            .ok_or_else(|| "Android sparse output size overflows".to_string())?,
    }))
}

pub(crate) fn parse_sparse_chunk_header(
    sparse: &SparseHeader,
    data: &[u8],
) -> Result<SparseChunk, String> {
    if data.len() < sparse.chunk_header_size {
        return Err("Incomplete Android sparse chunk header".to_string());
    }
    let chunk_type = u16::from_le_bytes(data[..2].try_into().unwrap());
    let block_count = u64::from(u32::from_le_bytes(data[4..8].try_into().unwrap()));
    let total_size = u64::from(u32::from_le_bytes(data[8..12].try_into().unwrap()));
    let output_bytes = block_count
        .checked_mul(sparse.block_size)
        .ok_or_else(|| "Android sparse chunk output size overflows".to_string())?;
    let header_size = sparse.chunk_header_size as u64;

    let (kind, expected_payload, output_bytes) = match chunk_type {
        0xcac1 => (SparseChunkKind::Raw, output_bytes, output_bytes),
        0xcac2 => (SparseChunkKind::Fill, 4, output_bytes),
        0xcac3 => (SparseChunkKind::DontCare, 0, output_bytes),
        0xcac4 => (SparseChunkKind::Crc32, 4, 0),
        _ => {
            return Err(format!(
                "Unsupported Android sparse chunk type: 0x{chunk_type:04x}"
            ))
        }
    };
    if total_size != header_size + expected_payload {
        return Err(format!(
            "Invalid Android sparse chunk size for 0x{chunk_type:04x}"
        ));
    }

    Ok(SparseChunk {
        kind,
        output_bytes,
        payload_bytes: expected_payload,
    })
}

pub(crate) fn build_gpt_tables(
    partitions: &[GptPartition],
    flash_sectors: u32,
) -> Result<GptTables, String> {
    let flash_sectors = u64::from(flash_sectors);
    if flash_sectors <= GPT_PRIMARY_SECTORS + GPT_BACKUP_SECTORS {
        return Err("Flash is too small for a GPT".to_string());
    }
    if partitions.len() > GPT_ENTRY_COUNT {
        return Err("GPT parameter has too many partitions".to_string());
    }

    let first_usable = GPT_PRIMARY_SECTORS;
    let last_usable = flash_sectors - GPT_BACKUP_SECTORS - 1;
    let mut resolved = Vec::with_capacity(partitions.len());
    for partition in partitions {
        let end_sector = match partition.sector_count {
            Some(count) if count > 0 => partition
                .start_sector
                .checked_add(count - 1)
                .ok_or_else(|| format!("GPT partition {} range overflows", partition.name))?,
            Some(_) => return Err(format!("GPT partition {} has zero size", partition.name)),
            None => last_usable,
        };
        if partition.start_sector < first_usable || end_sector > last_usable {
            return Err(format!(
                "GPT partition {} is outside the usable flash range",
                partition.name
            ));
        }
        resolved.push((partition, end_sector));
    }
    let mut ranges: Vec<_> = resolved
        .iter()
        .map(|(partition, end_sector)| {
            (partition.start_sector, *end_sector, partition.name.as_str())
        })
        .collect();
    ranges.sort_unstable_by_key(|range| range.0);
    for pair in ranges.windows(2) {
        if pair[0].1 >= pair[1].0 {
            return Err(format!(
                "GPT partitions {} and {} overlap",
                pair[0].2, pair[1].2
            ));
        }
    }

    let mut entries = vec![0u8; GPT_ENTRY_SIZE * GPT_ENTRY_COUNT];
    for (index, (partition, end_sector)) in resolved.iter().enumerate() {
        let entry = &mut entries[index * GPT_ENTRY_SIZE..(index + 1) * GPT_ENTRY_SIZE];
        let (type_guid, table_unique_guid) = match rockchip_partition_guids(&partition.name) {
            Some((type_guid, unique_guid)) => (type_guid, Some(unique_guid)),
            None => (BASIC_DATA_GUID, None),
        };
        entry[..16].copy_from_slice(&type_guid);
        let mut unique_guid = [0u8; 16];
        unique_guid[0] = (index + 1) as u8;
        entry[16..32]
            .copy_from_slice(&partition.unique_guid.or(table_unique_guid).unwrap_or(unique_guid));
        entry[32..40].copy_from_slice(&partition.start_sector.to_le_bytes());
        entry[40..48].copy_from_slice(&end_sector.to_le_bytes());
        for (offset, codepoint) in partition.name.encode_utf16().take(36).enumerate() {
            entry[56 + offset * 2..58 + offset * 2].copy_from_slice(&codepoint.to_le_bytes());
        }
    }
    let entries_crc = crc32(&entries);

    let mut primary = vec![0u8; GPT_PRIMARY_SECTORS as usize * SECTOR_SIZE];
    write_protective_mbr(&mut primary[..SECTOR_SIZE], flash_sectors);
    primary[SECTOR_SIZE..2 * SECTOR_SIZE].copy_from_slice(&gpt_header(
        1,
        flash_sectors - 1,
        2,
        first_usable,
        last_usable,
        entries_crc,
    ));
    primary[2 * SECTOR_SIZE..].copy_from_slice(&entries);

    let backup_start = flash_sectors - GPT_BACKUP_SECTORS;
    let mut backup = vec![0u8; GPT_BACKUP_SECTORS as usize * SECTOR_SIZE];
    backup[..entries.len()].copy_from_slice(&entries);
    backup[entries.len()..entries.len() + SECTOR_SIZE].copy_from_slice(&gpt_header(
        flash_sectors - 1,
        1,
        backup_start,
        first_usable,
        last_usable,
        entries_crc,
    ));

    Ok(GptTables {
        primary,
        backup_start_sector: u32::try_from(backup_start)
            .map_err(|_| "Flash exceeds the RockUSB LBA address range".to_string())?,
        backup,
    })
}

fn parse_hex_sector(value: &str) -> Result<u64, String> {
    u64::from_str_radix(value.trim_start_matches("0x"), 16)
        .map_err(|_| format!("Invalid GPT sector value: {value}"))
}

fn write_protective_mbr(mbr: &mut [u8], flash_sectors: u64) {
    mbr[446 + 4] = 0xee;
    mbr[446 + 8..446 + 12].copy_from_slice(&1u32.to_le_bytes());
    mbr[446 + 12..446 + 16].copy_from_slice(
        &u32::try_from(flash_sectors.saturating_sub(1))
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    mbr[510..512].copy_from_slice(&[0x55, 0xaa]);
}

fn gpt_header(
    current_lba: u64,
    backup_lba: u64,
    entries_lba: u64,
    first_usable: u64,
    last_usable: u64,
    entries_crc: u32,
) -> [u8; SECTOR_SIZE] {
    let mut header = [0u8; SECTOR_SIZE];
    header[..8].copy_from_slice(b"EFI PART");
    header[8..12].copy_from_slice(&0x0001_0000u32.to_le_bytes());
    header[12..16].copy_from_slice(&92u32.to_le_bytes());
    header[24..32].copy_from_slice(&current_lba.to_le_bytes());
    header[32..40].copy_from_slice(&backup_lba.to_le_bytes());
    header[40..48].copy_from_slice(&first_usable.to_le_bytes());
    header[48..56].copy_from_slice(&last_usable.to_le_bytes());
    header[56..72].copy_from_slice(&DISK_GUID);
    header[72..80].copy_from_slice(&entries_lba.to_le_bytes());
    header[80..84].copy_from_slice(&(GPT_ENTRY_COUNT as u32).to_le_bytes());
    header[84..88].copy_from_slice(&(GPT_ENTRY_SIZE as u32).to_le_bytes());
    header[88..92].copy_from_slice(&entries_crc.to_le_bytes());
    let header_crc = crc32(&header[..92]);
    header[16..20].copy_from_slice(&header_crc.to_le_bytes());
    header
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;

    const PARAMETER: &[u8] = b"PARM\0TYPE: GPT\0CMDLINE:mtdparts=rk29xxnand:0x00002000@0x00002000(security),0x00002000@0x00004000(uboot),0x00014000@0x0000c800(boot),-@0x00020800(userdata:grow)\0";
    const PARAMETER_WITH_ROOTFS_UUID: &[u8] = b"PARM\0TYPE: GPT\0CMDLINE:mtdparts=rk29xxnand:0x00020000@0x00008000(boot),0x00c00000@0x00078000(rootfs)\nuuid:rootfs=614e0000-0000-4b53-8000-1d28000054a9\0";

    #[test]
    fn parses_gpt_partition_ranges_from_rockchip_parameter() {
        let partitions = parse_gpt_parameter(PARAMETER).unwrap().unwrap();

        assert_eq!(partitions[2].name, "boot");
        assert_eq!(partitions[2].start_sector, 0xc800);
        assert_eq!(partitions[2].sector_count, Some(0x14000));
        assert_eq!(partitions[3].name, "userdata");
        assert_eq!(partitions[3].sector_count, None);
    }

    #[test]
    fn ignores_binary_data_after_the_final_gpt_partition_entry() {
        let parameter = b"TYPE: GPT\0CMDLINE:mtdparts=rk29xxnand:0x00002000@0x00002000(security),-@0x007ed000(userdata:grow)\x0b\x42";
        let partitions = parse_gpt_parameter(parameter).unwrap().unwrap();

        assert_eq!(partitions.len(), 2);
        assert_eq!(partitions[1].name, "userdata");
        assert_eq!(partitions[1].start_sector, 0x007e_d000);
        assert_eq!(partitions[1].sector_count, None);
    }

    #[test]
    fn parses_legacy_parameter_partition_ranges_without_gpt_marker() {
        let parameter = b"PARM\0CMDLINE:mtdparts=rk29xxnand:0x00002000@0x00002000(uboot),0x00014000@0x0000c800(boot),-@0x00020800(rootfs)\0";
        let partitions = parse_parameter_partitions(parameter).unwrap().unwrap();

        assert_eq!(partitions[1].name, "boot");
        assert_eq!(partitions[1].start_sector, 0xc800);
        assert_eq!(partitions[2].sector_count, None);
    }

    #[test]
    fn generated_gpt_contains_the_primary_header_and_named_partition() {
        let partitions = parse_gpt_parameter(PARAMETER).unwrap().unwrap();
        let tables = build_gpt_tables(&partitions, 0x0080_0000).unwrap();

        assert_eq!(&tables.primary[512..520], b"EFI PART");
        let boot_entry = 2 * 512 + 2 * 128;
        assert_eq!(
            u64::from_le_bytes(
                tables.primary[boot_entry + 32..boot_entry + 40]
                    .try_into()
                    .unwrap()
            ),
            0xc800
        );
        assert_eq!(
            u64::from_le_bytes(
                tables.primary[boot_entry + 40..boot_entry + 48]
                    .try_into()
                    .unwrap()
            ),
            0x207ff
        );
        assert_eq!(tables.backup_start_sector, 0x0080_0000 - 33);
    }

    #[test]
    fn generated_gpt_preserves_partition_uuid_from_parameter() {
        let partitions = parse_gpt_parameter(PARAMETER_WITH_ROOTFS_UUID)
            .unwrap()
            .unwrap();
        let tables = build_gpt_tables(&partitions, 0x0100_0000).unwrap();
        let rootfs_entry = 2 * SECTOR_SIZE + 128;

        assert_eq!(
            &tables.primary[rootfs_entry + 16..rootfs_entry + 32],
            &[
                0x00, 0x00, 0x4e, 0x61, 0x00, 0x00, 0x53, 0x4b, 0x80, 0x00, 0x1d, 0x28, 0x00, 0x00,
                0x54, 0xa9
            ]
        );
    }

    #[test]
    fn parm_payload_length_excludes_binary_data_after_final_uuid() {
        let payload = b"TYPE: GPT\0CMDLINE:mtdparts=rk29xxnand:0x00020000@0x00008000(boot),0x00c00000@0x00078000(rootfs)\nuuid:rootfs=614e0000-0000-4b53-8000-1d28000054a9";
        let mut parameter = b"PARM".to_vec();
        parameter.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        parameter.extend_from_slice(payload);
        parameter.extend_from_slice(&[0xd7, 0xbb, 0xf9, 0xa7]);

        let partitions = parse_gpt_parameter(&parameter).unwrap().unwrap();
        let rootfs = partitions
            .iter()
            .find(|partition| partition.name == "rootfs")
            .unwrap();

        assert_eq!(
            rootfs.unique_guid,
            Some([
                0x00, 0x00, 0x4e, 0x61, 0x00, 0x00, 0x53, 0x4b, 0x80, 0x00, 0x1d, 0x28, 0x00, 0x00,
                0x54, 0xa9,
            ])
        );
    }

    #[test]
    fn rejects_truncated_parm_payload() {
        let parameter = b"PARM\x20\x00\x00\x00TYPE: GPT";

        assert_eq!(
            parse_gpt_parameter(parameter).unwrap_err(),
            "Rockchip PARM payload length 32 exceeds available data (9)"
        );
    }

    #[test]
    fn grow_partition_ends_at_the_last_usable_gpt_sector() {
        let partitions = parse_gpt_parameter(PARAMETER).unwrap().unwrap();
        let tables = build_gpt_tables(&partitions, 0x0080_0000).unwrap();
        let userdata_entry = 2 * 512 + 3 * 128;

        assert_eq!(
            u64::from_le_bytes(
                tables.primary[userdata_entry + 32..userdata_entry + 40]
                    .try_into()
                    .unwrap()
            ),
            0x20800
        );
        assert_eq!(
            u64::from_le_bytes(
                tables.primary[userdata_entry + 40..userdata_entry + 48]
                    .try_into()
                    .unwrap()
            ),
            0x0080_0000 - 34
        );
    }

    #[test]
    fn gpt_builder_rejects_a_partition_that_exceeds_the_flash() {
        let partitions = parse_gpt_parameter(PARAMETER).unwrap().unwrap();

        assert!(build_gpt_tables(&partitions, 0x0002_0000).is_err());
    }

    #[test]
    fn parses_android_sparse_header_and_raw_chunk() {
        let mut header = [0u8; 28];
        header[..4].copy_from_slice(&0xed26_ff3au32.to_le_bytes());
        header[4..6].copy_from_slice(&1u16.to_le_bytes());
        header[8..10].copy_from_slice(&28u16.to_le_bytes());
        header[10..12].copy_from_slice(&12u16.to_le_bytes());
        header[12..16].copy_from_slice(&4096u32.to_le_bytes());
        header[16..20].copy_from_slice(&2u32.to_le_bytes());
        header[20..24].copy_from_slice(&1u32.to_le_bytes());
        let sparse = parse_sparse_header(&header).unwrap().unwrap();

        let mut chunk = [0u8; 12];
        chunk[..2].copy_from_slice(&0xcac1u16.to_le_bytes());
        chunk[4..8].copy_from_slice(&2u32.to_le_bytes());
        chunk[8..12].copy_from_slice(&(12u32 + 8192).to_le_bytes());
        let chunk = parse_sparse_chunk_header(&sparse, &chunk).unwrap();

        assert_eq!(sparse.output_bytes, 8192);
        assert_eq!(chunk.kind, SparseChunkKind::Raw);
        assert_eq!(chunk.output_bytes, 8192);
        assert_eq!(chunk.payload_bytes, 8192);
    }

    #[test]
    fn parses_sparse_fill_and_dont_care_chunks() {
        let sparse = SparseHeader {
            file_header_size: 28,
            chunk_header_size: 12,
            block_size: 4096,
            total_chunks: 2,
            output_bytes: 8192,
        };
        let mut fill = [0u8; 12];
        fill[..2].copy_from_slice(&0xcac2u16.to_le_bytes());
        fill[4..8].copy_from_slice(&1u32.to_le_bytes());
        fill[8..12].copy_from_slice(&16u32.to_le_bytes());
        let fill = parse_sparse_chunk_header(&sparse, &fill).unwrap();

        let mut dont_care = [0u8; 12];
        dont_care[..2].copy_from_slice(&0xcac3u16.to_le_bytes());
        dont_care[4..8].copy_from_slice(&1u32.to_le_bytes());
        dont_care[8..12].copy_from_slice(&12u32.to_le_bytes());
        let dont_care = parse_sparse_chunk_header(&sparse, &dont_care).unwrap();

        assert_eq!(fill.kind, SparseChunkKind::Fill);
        assert_eq!(fill.payload_bytes, 4);
        assert_eq!(dont_care.kind, SparseChunkKind::DontCare);
        assert_eq!(dont_care.output_bytes, 4096);
    }

    fn decode_hex(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).unwrap())
            .collect()
    }

    const RK3506_PARAMETER_HEX: &str = "5041524de00100004649524d574152455f5645523a382e310a4d414348494e455f4d4f44454c3a524b333530360a4d414348494e455f49443a3030370a4d414e5546414354555245523a20524b333530360a4d414749433a20307835303431353234420a415441473a20307830303230303830300a4d414348494e453a20333530360a434845434b5f4d41534b3a20307838300a5057525f484c443a20302c302c412c302c310a545950453a204750540a47524f575f414c49474e3a20300a434d444c494e453a6d746470617274733d3a30783030303031303030403078303030303038303028766e766d292c3078303030303430303040307830303030313830302875626f6f74292c307830303030313030304030783030303035383030286d697363292c307830303030663030304030783030303036383030287265636f76657279292c30783030303035303030403078303030313538303028626f6f74292c30783030303530303030403078303030316138303028726f6f746673292c307830303030383030304030783030303661383030286f656d292c2d40307830303037323830302875736572646174613a67726f77290a757569643a726f6f7466733d36313465303030302d303030302d346235332d383030302d3164323830303030353461390a0d9b8ec6";

    #[test]
    fn rk3506_gpt_matches_official_upgrade_tool_output() {
        let official_guids: [(&str, &str, &str, u64, u64); 8] = [
            (
                "vnvm",
                "18e424110890f8419e3d8d872216c8a1",
                "4177ad673846c647a488d396583e582a",
                0x800,
                0x17ff,
            ),
            (
                "uboot",
                "6a48d4fb2ce48d44d4e97f5c324a7843",
                "3da0de3a12c6ed46cff9b79439f11561",
                0x1800,
                0x57ff,
            ),
            (
                "misc",
                "474fafbb0d23b645feaee4b35f300b00",
                "4a4882e510904a40899d47bd16ba3fa8",
                0x5800,
                0x67ff,
            ),
            (
                "recovery",
                "54d3c5c50b443e458832d36349b6ee24",
                "7f2711dc7febd9428027a83b14220e50",
                0x6800,
                0x157ff,
            ),
            (
                "boot",
                "2db7c35c57c18449a60130f55f80ac1c",
                "6a725a297d358243d5a6e7e871da99e0",
                0x15800,
                0x1a7ff,
            ),
            (
                "rootfs",
                "5054ea342a9f1b479c58280523e6fe2b",
                "00004e610000534b80001d28000054a9",
                0x1a800,
                0x6a7ff,
            ),
            (
                "oem",
                "5b5de39e34158d49aca12ea95d08afb2",
                "75a6c6484437f645d7ff474449eeb98e",
                0x6a800,
                0x727ff,
            ),
            (
                "userdata",
                "63e9b69b7d75f34698df9dcd617b9e2d",
                "311be7476e0d8c4ca6d42ef63fc0366c",
                0x72800,
                0x7fbde,
            ),
        ];

        let parameter = decode_hex(RK3506_PARAMETER_HEX);
        let partitions = parse_gpt_parameter(&parameter).unwrap().unwrap();
        let tables = build_gpt_tables(&partitions, 523_264).unwrap();

        let header = &tables.primary[SECTOR_SIZE..2 * SECTOR_SIZE];
        assert_eq!(
            &header[56..72],
            &decode_hex("3f83305a438cb147a41d84d40883d0c6")[..]
        );
        assert_eq!(
            u64::from_le_bytes(header[32..40].try_into().unwrap()),
            523_263
        );
        assert_eq!(
            u64::from_le_bytes(header[48..56].try_into().unwrap()),
            523_230
        );

        for (index, (name, type_guid, unique_guid, start, end)) in official_guids.iter().enumerate()
        {
            let entry = 2 * SECTOR_SIZE + index * 128;
            assert_eq!(
                &tables.primary[entry..entry + 16],
                &decode_hex(type_guid)[..],
                "type GUID of {name}"
            );
            assert_eq!(
                &tables.primary[entry + 16..entry + 32],
                &decode_hex(unique_guid)[..],
                "unique GUID of {name}"
            );
            assert_eq!(
                u64::from_le_bytes(tables.primary[entry + 32..entry + 40].try_into().unwrap()),
                *start,
                "start sector of {name}"
            );
            assert_eq!(
                u64::from_le_bytes(tables.primary[entry + 40..entry + 48].try_into().unwrap()),
                *end,
                "end sector of {name}"
            );
            let mut utf16 = [0u16; 36];
            for (offset, chunk) in tables.primary[entry + 56..entry + 128]
                .chunks_exact(2)
                .enumerate()
            {
                utf16[offset] = u16::from_le_bytes([chunk[0], chunk[1]]);
            }
            let decoded_name = String::from_utf16_lossy(&utf16);
            assert_eq!(decoded_name.trim_end_matches('\0'), *name);
        }
    }
}

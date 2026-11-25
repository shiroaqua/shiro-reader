use std::{
    borrow::Cow,
    collections::{BTreeMap, HashMap},
    io::Read as _,
};

use bytes::Bytes;
use flate2::{Crc, CrcReader, read::DeflateDecoder};
use shiro_error_catalog::ErrorCode;

use super::{
    MAX_CENTRAL_DIR_BYTES, MAX_DEFLATE_PREALLOC_BYTES, MAX_ENTRY_COMPRESSED_BYTES,
    MAX_ENTRY_UNCOMPRESSED_BYTES, MAX_ZIP_ENTRIES, OFFLOAD_COMPRESSED_THRESHOLD,
    OFFLOAD_UNCOMPRESSED_THRESHOLD, byte_source::BookByteSource,
};

const EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const ZIP64_LOCATOR_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x07];
const ZIP64_EOCD_SIG: [u8; 4] = [0x50, 0x4b, 0x06, 0x06];
const CD_SIG: [u8; 4] = [0x50, 0x4b, 0x01, 0x02];
const LFH_SIG: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];

const MAX_TAIL_WINDOW: u64 = 65_536 + 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZipCompression {
    Stored,
    Deflate,
    Unsupported(u16),
}

impl From<u16> for ZipCompression {
    fn from(raw: u16) -> Self {
        match raw {
            0 => Self::Stored,
            8 => Self::Deflate,
            other => Self::Unsupported(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ZipEntryMeta {
    pub compression: ZipCompression,
    pub compressed_size: u64,
    pub uncompressed_size: u64,
    pub crc32: u32,
    pub local_header_offset: u64,
}

/// Read-only, lock-free concurrent ZIP/ZIP64 central directory index.
#[derive(Debug)]
pub struct ZipIndex {
    entries: HashMap<Box<str>, ZipEntryMeta>,
}

impl ZipIndex {
    /// Exports a sorted `path -> uncompressed_size` table once for `open_epub` responses.
    pub fn export_entry_sizes(&self) -> BTreeMap<String, u64> {
        self.entries
            .iter()
            .map(|(path, meta)| (path.as_ref().to_owned(), meta.uncompressed_size))
            .collect()
    }

    pub(crate) async fn parse(source: &BookByteSource) -> Result<Self, ErrorCode> {
        let total_size = source.total_size();
        if total_size < 22 {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }

        let tail_len = total_size.min(MAX_TAIL_WINDOW) as usize;
        let tail_start = total_size - tail_len as u64;
        let tail = source.read_exact_at(tail_start, tail_len).await?;

        let (eocd_rel, eocd) = find_eocd(&tail).ok_or(ErrorCode::ReaderCorruptedEpub)?;
        let eocd_offset = tail_start + eocd_rel as u64;

        let mut disk_number = u64::from(read_u16_le(&eocd[4..6]));
        let mut cd_disk = u64::from(read_u16_le(&eocd[6..8]));
        let mut disk_entries = u64::from(read_u16_le(&eocd[8..10]));
        let mut total_entries = u64::from(read_u16_le(&eocd[10..12]));
        let mut cd_size = u64::from(read_u32_le(&eocd[12..16]));
        let mut cd_offset = u64::from(read_u32_le(&eocd[16..20]));
        let mut cd_upper_bound = eocd_offset;

        let needs_zip64 = disk_number == 0xFFFF
            || cd_disk == 0xFFFF
            || disk_entries == 0xFFFF
            || total_entries == 0xFFFF
            || cd_size == 0xFFFF_FFFF
            || cd_offset == 0xFFFF_FFFF;

        if needs_zip64 {
            if eocd_offset < 20 {
                return Err(ErrorCode::ReaderCorruptedEpub);
            }
            let locator_offset = eocd_offset - 20;
            let locator = source.read_exact_at(locator_offset, 20).await?;
            if locator[0..4] != ZIP64_LOCATOR_SIG {
                return Err(ErrorCode::ReaderCorruptedEpub);
            }
            let zip64_eocd_disk = read_u32_le(&locator[4..8]);
            let zip64_eocd_offset = read_u64_le(&locator[8..16]);
            let total_disks = read_u32_le(&locator[16..20]);
            if zip64_eocd_disk != 0 || total_disks > 1 {
                return Err(ErrorCode::ReaderCorruptedEpub);
            }
            if zip64_eocd_offset
                .checked_add(56)
                .is_none_or(|end| end > locator_offset)
            {
                return Err(ErrorCode::ReaderCorruptedEpub);
            }

            let z64 = source.read_exact_at(zip64_eocd_offset, 56).await?;
            if z64[0..4] != ZIP64_EOCD_SIG {
                return Err(ErrorCode::ReaderCorruptedEpub);
            }
            let record_size = read_u64_le(&z64[4..12]);
            if record_size < 44
                || zip64_eocd_offset
                    .checked_add(12)
                    .and_then(|o| o.checked_add(record_size))
                    .is_none_or(|end| end > locator_offset)
            {
                return Err(ErrorCode::ReaderCorruptedEpub);
            }

            disk_number = u64::from(read_u32_le(&z64[16..20]));
            cd_disk = u64::from(read_u32_le(&z64[20..24]));
            disk_entries = read_u64_le(&z64[24..32]);
            total_entries = read_u64_le(&z64[32..40]);
            cd_size = read_u64_le(&z64[40..48]);
            cd_offset = read_u64_le(&z64[48..56]);
            cd_upper_bound = zip64_eocd_offset;
        }

        // Reject multi-volume archives strictly AFTER ZIP64 overrides.
        if disk_number != 0 || cd_disk != 0 || disk_entries != total_entries {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }

        if cd_size > MAX_CENTRAL_DIR_BYTES as u64 || total_entries > MAX_ZIP_ENTRIES as u64 {
            return Err(ErrorCode::ReaderEntryTooLarge);
        }
        if cd_offset
            .checked_add(cd_size)
            .is_none_or(|end| end > cd_upper_bound)
        {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }

        let cd_bytes = source.read_exact_at(cd_offset, cd_size as usize).await?;
        let entries = parse_central_directory(&cd_bytes, total_entries as usize)?;
        Ok(Self { entries })
    }

    pub(crate) async fn read_entry(
        &self,
        source: &BookByteSource,
        raw_path: &str,
    ) -> Result<Bytes, ErrorCode> {
        let meta = self
            .lookup_entry(raw_path)
            .ok_or(ErrorCode::ReaderEntryNotFound)?;

        if meta.compressed_size > MAX_ENTRY_COMPRESSED_BYTES as u64
            || meta.uncompressed_size > MAX_ENTRY_UNCOMPRESSED_BYTES as u64
        {
            return Err(ErrorCode::ReaderEntryTooLarge);
        }
        if matches!(meta.compression, ZipCompression::Unsupported(_)) {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }

        let lfh = source.read_exact_at(meta.local_header_offset, 30).await?;
        if lfh[0..4] != LFH_SIG {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }
        let lfh_flags = read_u16_le(&lfh[6..8]);
        if lfh_flags & 0x0001 != 0 {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }
        let lfh_name_len = u64::from(read_u16_le(&lfh[26..28]));
        let lfh_extra_len = u64::from(read_u16_le(&lfh[28..30]));
        let data_offset = meta
            .local_header_offset
            .checked_add(30)
            .and_then(|o| o.checked_add(lfh_name_len))
            .and_then(|o| o.checked_add(lfh_extra_len))
            .ok_or(ErrorCode::ReaderCorruptedEpub)?;

        let compressed_len = usize::try_from(meta.compressed_size)
            .map_err(|_| ErrorCode::ReaderEntryTooLarge)?;
        let compressed = source.read_exact_at(data_offset, compressed_len).await?;

        match meta.compression {
            ZipCompression::Stored => {
                if meta.compressed_size != meta.uncompressed_size {
                    return Err(ErrorCode::ReaderCorruptedEpub);
                }
                let mut crc = Crc::new();
                crc.update(&compressed);
                if crc.sum() != meta.crc32 {
                    return Err(ErrorCode::ReaderCorruptedEpub);
                }
                Ok(compressed)
            }
            ZipCompression::Deflate => {
                if meta.compressed_size > OFFLOAD_COMPRESSED_THRESHOLD
                    || meta.uncompressed_size > OFFLOAD_UNCOMPRESSED_THRESHOLD
                {
                    let uncompressed_size = meta.uncompressed_size;
                    let crc32 = meta.crc32;
                    tokio::task::spawn_blocking(move || {
                        inflate_and_verify(compressed, uncompressed_size, crc32)
                    })
                    .await
                    .map_err(|_| ErrorCode::ReaderCorruptedEpub)?
                } else {
                    inflate_and_verify(compressed, meta.uncompressed_size, meta.crc32)
                }
            }
            ZipCompression::Unsupported(_) => Err(ErrorCode::ReaderCorruptedEpub),
        }
    }

    fn lookup_entry(&self, raw_path: &str) -> Option<ZipEntryMeta> {
        if let Some(norm) = normalize_archive_path(raw_path, false)
            && let Some(meta) = self.entries.get(norm.as_str())
        {
            return Some(*meta);
        }

        let stripped = raw_path.split(['?', '#']).next().unwrap_or(raw_path);
        if stripped != raw_path
            && let Some(norm) = normalize_archive_path(stripped, false)
            && let Some(meta) = self.entries.get(norm.as_str())
        {
            return Some(*meta);
        }

        if let Some(decoded) = percent_decode_utf8_strict(stripped)
            && let Some(norm) = normalize_archive_path(&decoded, false)
            && let Some(meta) = self.entries.get(norm.as_str())
        {
            return Some(*meta);
        }

        None
    }
}

fn find_eocd(tail: &[u8]) -> Option<(usize, &[u8])> {
    if tail.len() < 22 {
        return None;
    }
    for i in (0..=tail.len() - 22).rev() {
        if tail[i..i + 4] == EOCD_SIG {
            let comment_len = usize::from(read_u16_le(&tail[i + 20..i + 22]));
            if i + 22 + comment_len == tail.len() {
                return Some((i, &tail[i..]));
            }
        }
    }
    None
}

fn parse_central_directory(
    cd: &[u8],
    total_entries: usize,
) -> Result<HashMap<Box<str>, ZipEntryMeta>, ErrorCode> {
    let mut entries = HashMap::with_capacity(total_entries.min(1024));
    let mut cursor = 0usize;

    for _ in 0..total_entries {
        let header_end = cursor
            .checked_add(46)
            .filter(|&end| end <= cd.len())
            .ok_or(ErrorCode::ReaderCorruptedEpub)?;
        let hdr = &cd[cursor..header_end];
        if hdr[0..4] != CD_SIG {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }

        let flags = read_u16_le(&hdr[8..10]);
        if flags & 0x0001 != 0 {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }
        let method = read_u16_le(&hdr[10..12]);
        let crc32 = read_u32_le(&hdr[16..20]);
        let raw_comp_size = read_u32_le(&hdr[20..24]);
        let raw_uncomp_size = read_u32_le(&hdr[24..28]);
        let name_len = usize::from(read_u16_le(&hdr[28..30]));
        let extra_len = usize::from(read_u16_le(&hdr[30..32]));
        let comment_len = usize::from(read_u16_le(&hdr[32..34]));
        let raw_disk_start = read_u16_le(&hdr[34..36]);
        let raw_lh_offset = read_u32_le(&hdr[42..46]);

        let name_start = header_end;
        let extra_start = name_start
            .checked_add(name_len)
            .filter(|&end| end <= cd.len())
            .ok_or(ErrorCode::ReaderCorruptedEpub)?;
        let comment_start = extra_start
            .checked_add(extra_len)
            .filter(|&end| end <= cd.len())
            .ok_or(ErrorCode::ReaderCorruptedEpub)?;
        let next_cursor = comment_start
            .checked_add(comment_len)
            .filter(|&end| end <= cd.len())
            .ok_or(ErrorCode::ReaderCorruptedEpub)?;

        let name_bytes = &cd[name_start..extra_start];
        let extra_bytes = &cd[extra_start..comment_start];

        let mut uncompressed_size = u64::from(raw_uncomp_size);
        let mut compressed_size = u64::from(raw_comp_size);
        let mut local_header_offset = u64::from(raw_lh_offset);

        let need_uncomp = raw_uncomp_size == 0xFFFF_FFFF;
        let need_comp = raw_comp_size == 0xFFFF_FFFF;
        let need_lh = raw_lh_offset == 0xFFFF_FFFF;
        let need_disk = raw_disk_start == 0xFFFF;

        if need_uncomp || need_comp || need_lh || need_disk {
            let z64_extra = find_zip64_extra_field(extra_bytes)
                .ok_or(ErrorCode::ReaderCorruptedEpub)?;
            let mut pos = 0usize;
            if need_uncomp {
                uncompressed_size = take_u64_le(z64_extra, &mut pos)?;
            }
            if need_comp {
                compressed_size = take_u64_le(z64_extra, &mut pos)?;
            }
            if need_lh {
                local_header_offset = take_u64_le(z64_extra, &mut pos)?;
            }
            if need_disk {
                let disk_start = take_u32_le(z64_extra, &mut pos)?;
                if disk_start != 0 {
                    return Err(ErrorCode::ReaderCorruptedEpub);
                }
            }
        } else if raw_disk_start != 0 {
            return Err(ErrorCode::ReaderCorruptedEpub);
        }

        let decoded = decode_zip_filename(name_bytes, flags & 0x0800 != 0)?;
        if !decoded.ends_with('/')
            && !decoded.ends_with('\\')
            && let Some(norm) = normalize_archive_path(&decoded, true)
        {
            entries.insert(
                norm.into_boxed_str(),
                ZipEntryMeta {
                    compression: ZipCompression::from(method),
                    compressed_size,
                    uncompressed_size,
                    crc32,
                    local_header_offset,
                },
            );
        }

        cursor = next_cursor;
    }

    Ok(entries)
}

fn find_zip64_extra_field(extra: &[u8]) -> Option<&[u8]> {
    let mut cursor = 0usize;
    while cursor + 4 <= extra.len() {
        let header_id = read_u16_le(&extra[cursor..cursor + 2]);
        let field_len = usize::from(read_u16_le(&extra[cursor + 2..cursor + 4]));
        let data_start = cursor + 4;
        let data_end = data_start.checked_add(field_len)?;
        if data_end > extra.len() {
            return None;
        }
        if header_id == 0x0001 {
            return Some(&extra[data_start..data_end]);
        }
        cursor = data_end;
    }
    None
}

fn decode_zip_filename(bytes: &[u8], utf8_flag: bool) -> Result<String, ErrorCode> {
    if utf8_flag {
        std::str::from_utf8(bytes)
            .map(str::to_owned)
            .map_err(|_| ErrorCode::ReaderCorruptedEpub)
    } else if let Ok(s) = std::str::from_utf8(bytes) {
        Ok(s.to_owned())
    } else {
        Ok(bytes.iter().map(|&b| char::from(b)).collect())
    }
}

fn normalize_archive_path(raw: &str, allow_backslash_sep: bool) -> Option<String> {
    if raw.contains('\0') {
        return None;
    }
    if !allow_backslash_sep && raw.contains('\\') {
        return None;
    }
    let normalized_seps: Cow<'_, str> = if allow_backslash_sep && raw.contains('\\') {
        Cow::Owned(raw.replace('\\', "/"))
    } else {
        Cow::Borrowed(raw)
    };

    let mut trimmed = normalized_seps.as_ref();
    loop {
        if let Some(rest) = trimmed.strip_prefix('/') {
            trimmed = rest;
        } else if let Some(rest) = trimmed.strip_prefix("./") {
            trimmed = rest;
        } else {
            break;
        }
    }
    if trimmed.is_empty() || trimmed.ends_with('/') {
        return None;
    }

    let mut out = String::with_capacity(trimmed.len());
    for seg in trimmed.split('/') {
        if seg.is_empty() {
            continue;
        }
        if seg == "." || seg == ".." {
            return None;
        }
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(seg);
    }

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn percent_decode_utf8_strict(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = hex_nibble(bytes[i + 1])?;
            let lo = hex_nibble(bytes[i + 2])?;
            out.push((hi << 4) | lo);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn inflate_and_verify(
    compressed: Bytes,
    uncompressed_size: u64,
    expected_crc: u32,
) -> Result<Bytes, ErrorCode> {
    let expected_usize =
        usize::try_from(uncompressed_size).map_err(|_| ErrorCode::ReaderEntryTooLarge)?;
    if expected_usize > MAX_ENTRY_UNCOMPRESSED_BYTES {
        return Err(ErrorCode::ReaderEntryTooLarge);
    }

    let mut out = Vec::with_capacity(expected_usize.min(MAX_DEFLATE_PREALLOC_BYTES));
    let mut crc_reader = CrcReader::new(DeflateDecoder::new(compressed.as_ref()));
    let limit = uncompressed_size
        .checked_add(1)
        .ok_or(ErrorCode::ReaderEntryTooLarge)?;

    if crc_reader.by_ref().take(limit).read_to_end(&mut out).is_err() {
        return Err(ErrorCode::ReaderCorruptedEpub);
    }
    if (out.len() as u64) > uncompressed_size {
        return Err(ErrorCode::ReaderEntryTooLarge);
    }
    if (out.len() as u64) != uncompressed_size {
        return Err(ErrorCode::ReaderCorruptedEpub);
    }
    if crc_reader.crc().sum() != expected_crc {
        return Err(ErrorCode::ReaderCorruptedEpub);
    }
    Ok(Bytes::from(out))
}

fn read_u16_le(buf: &[u8]) -> u16 {
    u16::from_le_bytes([buf[0], buf[1]])
}

fn read_u32_le(buf: &[u8]) -> u32 {
    u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]])
}

fn read_u64_le(buf: &[u8]) -> u64 {
    u64::from_le_bytes([
        buf[0], buf[1], buf[2], buf[3], buf[4], buf[5], buf[6], buf[7],
    ])
}

fn take_u32_le(buf: &[u8], pos: &mut usize) -> Result<u32, ErrorCode> {
    let end = pos
        .checked_add(4)
        .filter(|&e| e <= buf.len())
        .ok_or(ErrorCode::ReaderCorruptedEpub)?;
    let val = read_u32_le(&buf[*pos..end]);
    *pos = end;
    Ok(val)
}

fn take_u64_le(buf: &[u8], pos: &mut usize) -> Result<u64, ErrorCode> {
    let end = pos
        .checked_add(8)
        .filter(|&e| e <= buf.len())
        .ok_or(ErrorCode::ReaderCorruptedEpub)?;
    let val = read_u64_le(&buf[*pos..end]);
    *pos = end;
    Ok(val)
}

#[cfg(test)]
mod tests {
    use std::{fs, io::Write as _, path::Path};

    use flate2::{Compression, write::DeflateEncoder};

    use super::*;

    struct RawZipEntrySpec<'a> {
        name: &'a [u8],
        flags: u16,
        method: u16,
        payload: Vec<u8>,
        crc32: u32,
        declared_comp_size: u64,
        declared_uncomp_size: u64,
        lfh_zero_sizes: bool,
        lfh_extra: &'a [u8],
        cd_extra: &'a [u8],
        force_zip64_lh_offset: bool,
    }

    impl<'a> RawZipEntrySpec<'a> {
        fn stored(name: &'a [u8], data: &[u8]) -> Self {
            let mut crc = Crc::new();
            crc.update(data);
            let len = data.len() as u64;
            Self {
                name,
                flags: 0x0800,
                method: 0,
                payload: data.to_vec(),
                crc32: crc.sum(),
                declared_comp_size: len,
                declared_uncomp_size: len,
                lfh_zero_sizes: false,
                lfh_extra: b"",
                cd_extra: b"",
                force_zip64_lh_offset: false,
            }
        }

        fn deflate(name: &'a [u8], raw: &[u8]) -> Self {
            let mut crc = Crc::new();
            crc.update(raw);
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(raw).unwrap();
            let compressed = encoder.finish().unwrap();
            Self {
                name,
                flags: 0x0800,
                method: 8,
                declared_comp_size: compressed.len() as u64,
                declared_uncomp_size: raw.len() as u64,
                payload: compressed,
                crc32: crc.sum(),
                lfh_zero_sizes: false,
                lfh_extra: b"",
                cd_extra: b"",
                force_zip64_lh_offset: false,
            }
        }
    }

    fn build_raw_zip(entries: &[RawZipEntrySpec<'_>], use_zip64_eocd_sentinel: bool) -> Vec<u8> {
        let mut out = Vec::new();
        let mut cd = Vec::new();

        for spec in entries {
            let lh_offset = out.len() as u64;
            let (lfh_comp, lfh_uncomp, lfh_crc) = if spec.lfh_zero_sizes {
                (0u32, 0u32, 0u32)
            } else {
                (
                    u32::try_from(spec.declared_comp_size).unwrap_or(0xFFFF_FFFF),
                    u32::try_from(spec.declared_uncomp_size).unwrap_or(0xFFFF_FFFF),
                    spec.crc32,
                )
            };

            out.extend_from_slice(&LFH_SIG);
            out.extend_from_slice(&20u16.to_le_bytes());
            out.extend_from_slice(&spec.flags.to_le_bytes());
            out.extend_from_slice(&spec.method.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&lfh_crc.to_le_bytes());
            out.extend_from_slice(&lfh_comp.to_le_bytes());
            out.extend_from_slice(&lfh_uncomp.to_le_bytes());
            out.extend_from_slice(&(spec.name.len() as u16).to_le_bytes());
            out.extend_from_slice(&(spec.lfh_extra.len() as u16).to_le_bytes());
            out.extend_from_slice(spec.name);
            out.extend_from_slice(spec.lfh_extra);
            out.extend_from_slice(&spec.payload);

            if spec.lfh_zero_sizes {
                out.extend_from_slice(&[0x50, 0x4b, 0x07, 0x08]);
                out.extend_from_slice(&spec.crc32.to_le_bytes());
                out.extend_from_slice(&(spec.declared_comp_size as u32).to_le_bytes());
                out.extend_from_slice(&(spec.declared_uncomp_size as u32).to_le_bytes());
            }

            let mut cd_extra = spec.cd_extra.to_vec();
            let raw_lh_offset = if spec.force_zip64_lh_offset {
                let mut z64_field = Vec::new();
                z64_field.extend_from_slice(&0x0001u16.to_le_bytes());
                z64_field.extend_from_slice(&8u16.to_le_bytes());
                z64_field.extend_from_slice(&lh_offset.to_le_bytes());
                cd_extra.extend_from_slice(&z64_field);
                0xFFFF_FFFFu32
            } else {
                lh_offset as u32
            };

            cd.extend_from_slice(&CD_SIG);
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes());
            cd.extend_from_slice(&spec.flags.to_le_bytes());
            cd.extend_from_slice(&spec.method.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&spec.crc32.to_le_bytes());
            cd.extend_from_slice(&(spec.declared_comp_size as u32).to_le_bytes());
            cd.extend_from_slice(&(spec.declared_uncomp_size as u32).to_le_bytes());
            cd.extend_from_slice(&(spec.name.len() as u16).to_le_bytes());
            cd.extend_from_slice(&(cd_extra.len() as u16).to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes());
            cd.extend_from_slice(&0u32.to_le_bytes());
            cd.extend_from_slice(&raw_lh_offset.to_le_bytes());
            cd.extend_from_slice(spec.name);
            cd.extend_from_slice(&cd_extra);
        }

        let cd_offset = out.len() as u64;
        let cd_size = cd.len() as u64;
        let entry_count = entries.len() as u64;
        out.extend_from_slice(&cd);

        if use_zip64_eocd_sentinel {
            let z64_eocd_offset = out.len() as u64;
            out.extend_from_slice(&ZIP64_EOCD_SIG);
            out.extend_from_slice(&44u64.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&45u16.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&entry_count.to_le_bytes());
            out.extend_from_slice(&entry_count.to_le_bytes());
            out.extend_from_slice(&cd_size.to_le_bytes());
            out.extend_from_slice(&cd_offset.to_le_bytes());

            out.extend_from_slice(&ZIP64_LOCATOR_SIG);
            out.extend_from_slice(&0u32.to_le_bytes());
            out.extend_from_slice(&z64_eocd_offset.to_le_bytes());
            out.extend_from_slice(&1u32.to_le_bytes());

            out.extend_from_slice(&EOCD_SIG);
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            out.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
        } else {
            out.extend_from_slice(&EOCD_SIG);
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(&(entry_count as u16).to_le_bytes());
            out.extend_from_slice(&(entry_count as u16).to_le_bytes());
            out.extend_from_slice(&(cd_size as u32).to_le_bytes());
            out.extend_from_slice(&(cd_offset as u32).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
        }

        out
    }

    #[tokio::test]
    async fn parses_real_smoke_cover_epub_and_reads_every_entry() {
        let epub_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/packaging/fixtures/smoke_cover.epub");
        let epub_bytes = Bytes::from(fs::read(epub_path).expect("read smoke_cover.epub"));
        let source = BookByteSource::Memory(epub_bytes);

        let index = ZipIndex::parse(&source).await.expect("parse smoke_cover.epub");
        let sizes = index.export_entry_sizes();
        assert!(!sizes.is_empty());
        assert_eq!(sizes.get("mimetype").copied(), Some(20));

        let mimetype = index.read_entry(&source, "mimetype").await.unwrap();
        assert_eq!(mimetype.as_ref(), b"application/epub+zip");

        let container = index
            .read_entry(&source, "./META-INF/container.xml")
            .await
            .unwrap();
        assert!(std::str::from_utf8(&container).unwrap().contains("rootfile"));

        for (path, expected_len) in sizes {
            let entry_bytes = index.read_entry(&source, &path).await.unwrap();
            assert_eq!(
                entry_bytes.len() as u64,
                expected_len,
                "size mismatch for {path}"
            );
        }
    }

    #[tokio::test]
    async fn supports_stored_deflate_data_descriptor_lfh_extra_and_zip64_variants() {
        let mut dd = RawZipEntrySpec::deflate(b"OEBPS/ch1.xhtml", b"<html>hello world!</html>");
        dd.flags |= 0x0008;
        dd.lfh_zero_sizes = true;
        dd.lfh_extra = b"\xaa\xbb\xcc\xdd\xee\xff";
        dd.cd_extra = b"\x11\x22";

        let mut z64_lh = RawZipEntrySpec::stored(b"OEBPS/hello world.txt", b"stored-zip64-entry");
        z64_lh.force_zip64_lh_offset = true;
        z64_lh.cd_extra = b"\x99\x99\x02\x00\xab\xcd";

        let zip_bytes = build_raw_zip(&[dd, z64_lh], true);
        let source = BookByteSource::Memory(Bytes::from(zip_bytes));
        let index = ZipIndex::parse(&source).await.expect("parse ZIP64 archive");

        assert_eq!(
            index
                .read_entry(&source, "OEBPS/ch1.xhtml#sec-2")
                .await
                .unwrap()
                .as_ref(),
            b"<html>hello world!</html>"
        );
        assert_eq!(
            index
                .read_entry(&source, "OEBPS/hello%20world.txt?v=1#top")
                .await
                .unwrap()
                .as_ref(),
            b"stored-zip64-entry"
        );
    }

    #[tokio::test]
    async fn normalizes_backslashes_applies_last_entry_wins_and_blocks_traversal() {
        let dir_entry = RawZipEntrySpec::stored(b"OEBPS\\", b"");
        let first = RawZipEntrySpec::stored(b"OEBPS\\Text\\page.xhtml", b"old-content");
        let second = RawZipEntrySpec::stored(b"OEBPS/Text/page.xhtml", b"new-content");
        let malformed = RawZipEntrySpec::stored(b"../escape.txt", b"bad");

        let zip_bytes = build_raw_zip(&[dir_entry, first, second, malformed], false);
        let source = BookByteSource::Memory(Bytes::from(zip_bytes));
        let index = ZipIndex::parse(&source).await.unwrap();

        let sizes = index.export_entry_sizes();
        assert_eq!(sizes.len(), 1);
        assert_eq!(sizes.get("OEBPS/Text/page.xhtml").copied(), Some(11));
        assert_eq!(
            index
                .read_entry(&source, "OEBPS/Text/page.xhtml")
                .await
                .unwrap()
                .as_ref(),
            b"new-content"
        );

        for bad in [
            "",
            "/",
            ".",
            "..",
            "OEBPS/../OEBPS/Text/page.xhtml",
            "OEBPS\\Text\\page.xhtml",
            "OEBPS/%2e%2e/OEBPS/Text/page.xhtml",
            "OEBPS%5cText%5cpage.xhtml",
            "OEBPS/Text/page.xhtml\0",
            "OEBPS/Text/page%00.xhtml",
            "escape.txt",
        ] {
            assert_eq!(
                index.read_entry(&source, bad).await.unwrap_err(),
                ErrorCode::ReaderEntryNotFound,
                "expected ReaderEntryNotFound for {bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_encrypted_unsupported_corrupt_crc_forged_size_and_deflate_bombs() {
        // 1. Encrypted entry bit 0
        let mut enc = RawZipEntrySpec::stored(b"secret.txt", b"abc");
        enc.flags |= 0x0001;
        let enc_src = BookByteSource::Memory(Bytes::from(build_raw_zip(&[enc], false)));
        assert_eq!(
            ZipIndex::parse(&enc_src).await.unwrap_err(),
            ErrorCode::ReaderCorruptedEpub
        );

        // 2. Unsupported compression method (e.g. 99)
        let mut unsup = RawZipEntrySpec::stored(b"unsup.bin", b"abc");
        unsup.method = 99;
        let unsup_src = BookByteSource::Memory(Bytes::from(build_raw_zip(&[unsup], false)));
        let unsup_idx = ZipIndex::parse(&unsup_src).await.unwrap();
        assert_eq!(
            unsup_idx.read_entry(&unsup_src, "unsup.bin").await.unwrap_err(),
            ErrorCode::ReaderCorruptedEpub
        );

        // 3. Corrupted CRC32
        let mut bad_crc = RawZipEntrySpec::deflate(b"badcrc.txt", b"hello crc");
        bad_crc.crc32 ^= 0xDEAD_BEEF;
        let bad_crc_src = BookByteSource::Memory(Bytes::from(build_raw_zip(&[bad_crc], false)));
        let bad_crc_idx = ZipIndex::parse(&bad_crc_src).await.unwrap();
        assert_eq!(
            bad_crc_idx.read_entry(&bad_crc_src, "badcrc.txt").await.unwrap_err(),
            ErrorCode::ReaderCorruptedEpub
        );

        // 4. Forged 64 MiB uncompressed_size header on a tiny deflate payload (must NOT OOM or allocate 64 MiB)
        let mut forged = RawZipEntrySpec::deflate(b"forged.txt", b"tiny");
        forged.declared_uncomp_size = MAX_ENTRY_UNCOMPRESSED_BYTES as u64;
        let forged_src = BookByteSource::Memory(Bytes::from(build_raw_zip(&[forged], false)));
        let forged_idx = ZipIndex::parse(&forged_src).await.unwrap();
        assert_eq!(
            forged_idx.read_entry(&forged_src, "forged.txt").await.unwrap_err(),
            ErrorCode::ReaderCorruptedEpub
        );

        // 5. Deflate bomb outputting more bytes than declared uncompressed_size -> ReaderEntryTooLarge
        let mut bomb = RawZipEntrySpec::deflate(b"bomb.txt", &[b'A'; 4096]);
        bomb.declared_uncomp_size = 100;
        let bomb_src = BookByteSource::Memory(Bytes::from(build_raw_zip(&[bomb], false)));
        let bomb_idx = ZipIndex::parse(&bomb_src).await.unwrap();
        assert_eq!(
            bomb_idx.read_entry(&bomb_src, "bomb.txt").await.unwrap_err(),
            ErrorCode::ReaderEntryTooLarge
        );

        // 6. Entry declaring > 64 MiB -> ReaderEntryTooLarge immediately
        let mut huge = RawZipEntrySpec::stored(b"huge.bin", b"x");
        huge.declared_uncomp_size = (MAX_ENTRY_UNCOMPRESSED_BYTES as u64) + 1;
        let huge_src = BookByteSource::Memory(Bytes::from(build_raw_zip(&[huge], false)));
        let huge_idx = ZipIndex::parse(&huge_src).await.unwrap();
        assert_eq!(
            huge_idx.read_entry(&huge_src, "huge.bin").await.unwrap_err(),
            ErrorCode::ReaderEntryTooLarge
        );

        // 7. EOCD declaring > 50_000 entries or > 16 MiB CD rejects before reading CD
        let mut fake_many_eocd = vec![0u8; 22];
        fake_many_eocd[0..4].copy_from_slice(&EOCD_SIG);
        fake_many_eocd[8..10].copy_from_slice(&50_001u16.to_le_bytes());
        fake_many_eocd[10..12].copy_from_slice(&50_001u16.to_le_bytes());
        let many_src = BookByteSource::Memory(Bytes::from(fake_many_eocd));
        assert_eq!(
            ZipIndex::parse(&many_src).await.unwrap_err(),
            ErrorCode::ReaderEntryTooLarge
        );

        let mut fake_big_cd_eocd = vec![0u8; 22];
        fake_big_cd_eocd[0..4].copy_from_slice(&EOCD_SIG);
        fake_big_cd_eocd[12..16].copy_from_slice(&((MAX_CENTRAL_DIR_BYTES as u32) + 1).to_le_bytes());
        let big_cd_src = BookByteSource::Memory(Bytes::from(fake_big_cd_eocd));
        assert_eq!(
            ZipIndex::parse(&big_cd_src).await.unwrap_err(),
            ErrorCode::ReaderEntryTooLarge
        );
    }
}

use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use shiro_error_catalog::ErrorCode;
use tempfile::NamedTempFile;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::OnceCell,
};
use tracing::warn;
use ttf_parser::{Language, name_id};

use crate::reader_config::{
    CustomFontFormat, CustomFontMetaDto, ReaderConfigStore, normalize_book_hash,
};

pub const MAX_CUSTOM_FONT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemFontFamilyDto {
    pub family: String,
}

#[cfg(not(target_os = "android"))]
static SYSTEM_FONT_FAMILIES_CACHE: OnceCell<Vec<SystemFontFamilyDto>> = OnceCell::const_new();

/// Enumerates installed system font families on desktop platforms and caches them in memory.
pub async fn list_system_fonts(can_enumerate: bool) -> Vec<SystemFontFamilyDto> {
    if !can_enumerate {
        return Vec::new();
    }

    #[cfg(target_os = "android")]
    {
        Vec::new()
    }

    #[cfg(not(target_os = "android"))]
    {
        SYSTEM_FONT_FAMILIES_CACHE
            .get_or_try_init(|| async {
                tokio::task::spawn_blocking(scan_system_fonts_blocking)
                    .await
                    .map_err(|_| ())
            })
            .await
            .cloned()
            .unwrap_or_default()
    }
}

#[cfg(not(target_os = "android"))]
fn scan_system_fonts_blocking() -> Vec<SystemFontFamilyDto> {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();

    let mut families = BTreeSet::new();
    for face in db.faces() {
        for (family, _) in &face.families {
            let trimmed = family.trim();
            if !trimmed.is_empty() {
                families.insert(trimmed.to_owned());
            }
        }
    }

    let mut result: Vec<SystemFontFamilyDto> = families
        .into_iter()
        .map(|family| SystemFontFamilyDto { family })
        .collect();
    result.sort_by(|a, b| {
        a.family
            .to_lowercase()
            .cmp(&b.family.to_lowercase())
            .then_with(|| a.family.cmp(&b.family))
    });
    result
}

/// Validates a standalone single-font SFNT payload (`.ttf` / `.otf`) and parses its family name.
///
/// Rejects `.ttc` font collections, `.woff` / `.woff2` containers, and oversized payloads.
pub fn validate_and_parse_single_font(
    bytes: &[u8],
    fallback_file_name: &str,
) -> Result<(CustomFontFormat, String), ErrorCode> {
    if bytes.len() as u64 > MAX_CUSTOM_FONT_BYTES {
        return Err(ErrorCode::ReaderFontTooLarge);
    }
    if bytes.len() < 12 {
        return Err(ErrorCode::ReaderInvalidFontFile);
    }

    let format = match &bytes[..4] {
        b"ttcf" | b"wOFF" | b"wOF2" => return Err(ErrorCode::ReaderInvalidFontFile),
        [0x00, 0x01, 0x00, 0x00] | b"true" => CustomFontFormat::Ttf,
        b"OTTO" => CustomFontFormat::Otf,
        _ => return Err(ErrorCode::ReaderInvalidFontFile),
    };

    if ttf_parser::fonts_in_collection(bytes).is_some() {
        return Err(ErrorCode::ReaderInvalidFontFile);
    }

    let face = ttf_parser::Face::parse(bytes, 0).map_err(|_| ErrorCode::ReaderInvalidFontFile)?;
    let family_name = extract_family_name(&face, fallback_file_name);
    Ok((format, family_name))
}

fn extract_family_name(face: &ttf_parser::Face<'_>, fallback_file_name: &str) -> String {
    for target_id in [name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY] {
        let mut fallback_unicode: Option<String> = None;
        for name in face.names() {
            if name.name_id != target_id {
                continue;
            }
            let Some(decoded) = name.to_string() else {
                continue;
            };
            let trimmed = decoded.trim();
            if trimmed.is_empty() {
                continue;
            }
            if name.language() == Language::English_UnitedStates {
                return trimmed.to_owned();
            }
            if fallback_unicode.is_none() {
                fallback_unicode = Some(trimmed.to_owned());
            }
        }
        if let Some(found) = fallback_unicode {
            return found;
        }
    }

    Path::new(fallback_file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(str::trim)
        .filter(|stem| !stem.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| "Custom Font".to_owned())
}

fn sanitize_display_file_name(raw_name: &str, format: CustomFontFormat) -> String {
    let candidate = Path::new(raw_name)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty());
    match candidate {
        Some(name) => name.to_owned(),
        None => format!("custom-font.{}", format.extension()),
    }
}

fn write_custom_font_blob(
    fonts_dir: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<CustomFontMetaDto, ErrorCode> {
    let (format, family_name) = validate_and_parse_single_font(bytes, file_name)?;
    let font_id = blake3::hash(bytes).to_hex().to_string();
    fs::create_dir_all(fonts_dir).map_err(|err| {
        warn!(dir = %fonts_dir.display(), error = %err, "failed to create reader_fonts directory");
        ErrorCode::ConfigSaveFailed
    })?;

    let target_path = fonts_dir.join(format!("{font_id}.{}", format.extension()));
    if !target_path.exists() {
        let mut temp = NamedTempFile::new_in(fonts_dir).map_err(|err| {
            warn!(dir = %fonts_dir.display(), error = %err, "failed to create temp font file");
            ErrorCode::ConfigSaveFailed
        })?;
        temp.write_all(bytes).map_err(|err| {
            warn!(error = %err, "failed to write temp custom font file");
            ErrorCode::ConfigSaveFailed
        })?;
        temp.as_file().sync_all().map_err(|err| {
            warn!(error = %err, "failed to sync temp custom font file");
            ErrorCode::ConfigSaveFailed
        })?;
        if let Err(err) = temp.persist(&target_path) {
            warn!(
                path = %target_path.display(),
                error = %err.error,
                "failed to persist custom font file"
            );
            return Err(ErrorCode::ConfigSaveFailed);
        }
    }

    Ok(CustomFontMetaDto {
        font_id,
        family_name,
        file_name: sanitize_display_file_name(file_name, format),
        format,
        size_bytes: bytes.len() as u64,
    })
}

/// Validates and imports raw `.ttf` / `.otf` bytes into content-addressed `reader_fonts/`.
pub fn import_custom_font_bytes(
    store: &ReaderConfigStore,
    file_name: &str,
    bytes: &[u8],
) -> Result<CustomFontMetaDto, ErrorCode> {
    let meta = write_custom_font_blob(store.fonts_dir(), file_name, bytes)?;

    store.upsert_custom_font(meta.clone()).map_err(|err| {
        warn!(error = %err, "failed to record custom font metadata");
        ErrorCode::ConfigSaveFailed
    })?;

    Ok(meta)
}

/// Streams a custom font from any async reader (including Android SAF URIs/FIFOs) with strict
/// `MAX_CUSTOM_FONT_BYTES` enforcement.
pub async fn import_custom_font_from_reader<R: AsyncRead + Unpin>(
    store: &ReaderConfigStore,
    file_name: &str,
    reader: R,
    expected_size: Option<u64>,
) -> Result<CustomFontMetaDto, ErrorCode> {
    import_custom_font_from_reader_with_limit(
        store,
        file_name,
        reader,
        expected_size,
        MAX_CUSTOM_FONT_BYTES,
    )
    .await
}

async fn import_custom_font_from_reader_with_limit<R: AsyncRead + Unpin>(
    store: &ReaderConfigStore,
    file_name: &str,
    reader: R,
    expected_size: Option<u64>,
    max_bytes: u64,
) -> Result<CustomFontMetaDto, ErrorCode> {
    if let Some(size) = expected_size
        && size > max_bytes
    {
        return Err(ErrorCode::ReaderFontTooLarge);
    }

    let mut limited = reader.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::new();
    limited
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| ErrorCode::ImportReadFailed)?;

    if bytes.len() as u64 > max_bytes {
        return Err(ErrorCode::ReaderFontTooLarge);
    }

    let fonts_dir: PathBuf = store.fonts_dir().to_path_buf();
    let owned_name = file_name.to_owned();
    let meta = tokio::task::spawn_blocking(move || {
        write_custom_font_blob(&fonts_dir, &owned_name, &bytes)
    })
    .await
    .map_err(|_| ErrorCode::ConfigSaveFailed)??;

    store.upsert_custom_font(meta.clone()).map_err(|err| {
        warn!(error = %err, "failed to record custom font metadata");
        ErrorCode::ConfigSaveFailed
    })?;

    Ok(meta)
}

/// Reads an imported custom font payload only if `font_id` is valid hex and registered in manifest.
pub async fn read_custom_font_bytes(
    store: &ReaderConfigStore,
    font_id: &str,
) -> Result<(Vec<u8>, CustomFontFormat), ErrorCode> {
    let normalized = normalize_book_hash(font_id).map_err(|_| ErrorCode::ReaderFontNotFound)?;
    let meta = store
        .find_custom_font(&normalized)
        .ok_or(ErrorCode::ReaderFontNotFound)?;
    let path = store
        .fonts_dir()
        .join(format!("{}.{}", meta.font_id, meta.format.extension()));
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| ErrorCode::ReaderFontNotFound)?;
    Ok((bytes, meta.format))
}

/// Removes a registered custom font, falls back any referencing global/book settings to `publisher`,
/// and deletes the physical file from `reader_fonts/`.
pub fn delete_custom_font(store: &ReaderConfigStore, font_id: &str) -> Result<(), ErrorCode> {
    let normalized = normalize_book_hash(font_id).map_err(|_| ErrorCode::ReaderFontNotFound)?;
    let removed = store.remove_custom_font_and_fallback_references(&normalized)?;
    let path = store
        .fonts_dir()
        .join(format!("{}.{}", removed.font_id, removed.format.extension()));
    if let Err(err) = fs::remove_file(&path)
        && err.kind() != std::io::ErrorKind::NotFound
    {
        warn!(
            path = %path.display(),
            error = %err,
            "failed to remove deleted custom font file from disk"
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader_config::{
        ReaderFontChoiceDto, ReaderProfilePatchDto, ReaderTypographyPatchDto,
    };

    fn build_minimal_sfnt(sfnt_magic: [u8; 4], family_name: &str) -> Vec<u8> {
        let mut head = vec![0u8; 54];
        head[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes()); // major/minor
        head[12..16].copy_from_slice(&0x5F0F_3CF5u32.to_be_bytes()); // magicNumber
        head[18..20].copy_from_slice(&1000u16.to_be_bytes()); // unitsPerEm
        head[50..52].copy_from_slice(&0i16.to_be_bytes()); // indexToLocFormat = short

        let mut hhea = vec![0u8; 36];
        hhea[0..4].copy_from_slice(&0x0001_0000u32.to_be_bytes());
        hhea[4..6].copy_from_slice(&800i16.to_be_bytes());
        hhea[6..8].copy_from_slice(&(-200i16).to_be_bytes());
        hhea[34..36].copy_from_slice(&1u16.to_be_bytes());

        let mut maxp = Vec::new();
        maxp.extend_from_slice(&0x0000_5000u32.to_be_bytes()); // version 0.5
        maxp.extend_from_slice(&1u16.to_be_bytes()); // numGlyphs = 1

        let utf16_be: Vec<u8> = family_name
            .encode_utf16()
            .flat_map(|unit| unit.to_be_bytes())
            .collect();
        let mut name_table = Vec::new();
        name_table.extend_from_slice(&0u16.to_be_bytes()); // format 0
        name_table.extend_from_slice(&1u16.to_be_bytes()); // count = 1
        name_table.extend_from_slice(&18u16.to_be_bytes()); // stringOffset = 6 + 12
        name_table.extend_from_slice(&3u16.to_be_bytes()); // platformID = Windows
        name_table.extend_from_slice(&1u16.to_be_bytes()); // encodingID = Unicode BMP
        name_table.extend_from_slice(&0x0409u16.to_be_bytes()); // languageID = en-US
        name_table.extend_from_slice(&1u16.to_be_bytes()); // nameID = FAMILY
        name_table.extend_from_slice(&(utf16_be.len() as u16).to_be_bytes());
        name_table.extend_from_slice(&0u16.to_be_bytes()); // offset
        name_table.extend_from_slice(&utf16_be);

        let mut tables: Vec<([u8; 4], Vec<u8>)> = vec![
            (*b"head", head),
            (*b"hhea", hhea),
            (*b"maxp", maxp),
            (*b"name", name_table),
        ];
        tables.sort_by_key(|(tag, _)| *tag);

        let num_tables = tables.len() as u16;
        let header_len = 12 + tables.len() * 16;
        let mut out = Vec::new();
        out.extend_from_slice(&sfnt_magic);
        out.extend_from_slice(&num_tables.to_be_bytes());
        out.extend_from_slice(&0u16.to_be_bytes()); // searchRange
        out.extend_from_slice(&0u16.to_be_bytes()); // entrySelector
        out.extend_from_slice(&0u16.to_be_bytes()); // rangeShift

        let mut current_offset = header_len;
        let mut directory = Vec::with_capacity(tables.len() * 16);
        let mut body = Vec::new();
        for (tag, data) in tables {
            directory.extend_from_slice(&tag);
            directory.extend_from_slice(&0u32.to_be_bytes()); // checksum
            directory.extend_from_slice(&(current_offset as u32).to_be_bytes());
            directory.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(&data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
            current_offset = header_len + body.len();
        }
        out.extend_from_slice(&directory);
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn imports_valid_ttf_and_otf_deduplicates_and_rejects_collections_or_woff2() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReaderConfigStore::load_or_default(dir.path(), false);

        let ttf_bytes = build_minimal_sfnt([0x00, 0x01, 0x00, 0x00], "Shiro Serif Test");
        let meta = import_custom_font_bytes(&store, "ignored-name.ttf", &ttf_bytes).unwrap();
        assert_eq!(meta.format, CustomFontFormat::Ttf);
        assert_eq!(meta.family_name, "Shiro Serif Test");
        assert_eq!(meta.size_bytes, ttf_bytes.len() as u64);
        assert!(store.fonts_dir().join(format!("{}.ttf", meta.font_id)).is_file());

        // Re-importing identical bytes is idempotent.
        let meta_dup = import_custom_font_bytes(&store, "another.ttf", &ttf_bytes).unwrap();
        assert_eq!(meta_dup.font_id, meta.font_id);
        assert_eq!(store.snapshot("u".into(), true).custom_fonts.len(), 1);

        let otf_bytes = build_minimal_sfnt(*b"OTTO", "Shiro OTF Test");
        let otf_meta = import_custom_font_bytes(&store, "display.otf", &otf_bytes).unwrap();
        assert_eq!(otf_meta.format, CustomFontFormat::Otf);
        assert_eq!(otf_meta.family_name, "Shiro OTF Test");
        assert_eq!(store.snapshot("u".into(), true).custom_fonts.len(), 2);

        // Reject .ttc collection, .woff2, and corrupted bytes.
        let mut ttc = b"ttcf\x00\x01\x00\x00\x00\x00\x00\x01\x00\x00\x00\x10".to_vec();
        ttc.resize(64, 0);
        assert_eq!(
            import_custom_font_bytes(&store, "CJK.ttc", &ttc).unwrap_err(),
            ErrorCode::ReaderInvalidFontFile
        );

        let mut woff2 = b"wOF2".to_vec();
        woff2.resize(64, 0);
        assert_eq!(
            import_custom_font_bytes(&store, "Web.woff2", &woff2).unwrap_err(),
            ErrorCode::ReaderInvalidFontFile
        );

        assert_eq!(
            import_custom_font_bytes(&store, "fake.ttf", b"not a font at all").unwrap_err(),
            ErrorCode::ReaderInvalidFontFile
        );
    }

    #[tokio::test]
    async fn streaming_import_rejects_oversized_and_delete_resets_references() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReaderConfigStore::load_or_default(dir.path(), false);

        // Reject by expected_size before reading.
        let err_expected = import_custom_font_from_reader(
            &store,
            "huge.ttf",
            tokio::io::empty(),
            Some(MAX_CUSTOM_FONT_BYTES + 1),
        )
        .await
        .unwrap_err();
        assert_eq!(err_expected, ErrorCode::ReaderFontTooLarge);

        // Reject when actual stream exceeds limit even without expected_size.
        let err_stream = import_custom_font_from_reader_with_limit(
            &store,
            "huge-stream.ttf",
            tokio::io::repeat(0).take(256),
            None,
            128,
        )
        .await
        .unwrap_err();
        assert_eq!(err_stream, ErrorCode::ReaderFontTooLarge);

        // Import valid font, bind in global + book override, read back, then delete.
        let ttf_bytes = build_minimal_sfnt([0x00, 0x01, 0x00, 0x00], "Fallback Test Font");
        let meta = import_custom_font_from_reader(
            &store,
            "valid.ttf",
            std::io::Cursor::new(ttf_bytes.clone()),
            Some(ttf_bytes.len() as u64),
        )
        .await
        .unwrap();

        let (read_bytes, read_fmt) = read_custom_font_bytes(&store, &meta.font_id).await.unwrap();
        assert_eq!(read_fmt, CustomFontFormat::Ttf);
        assert_eq!(read_bytes, ttf_bytes);

        let mut global = store.snapshot("u".into(), true).global;
        global.typography.font = ReaderFontChoiceDto::Custom {
            font_id: meta.font_id.clone(),
        };
        store.update_global(global, "u".into(), true).unwrap();

        let book_hash = blake3::hash(b"font-ref-book").to_hex().to_string();
        store
            .set_book_override(
                &book_hash,
                ReaderProfilePatchDto {
                    typography: Some(ReaderTypographyPatchDto {
                        font: Some(ReaderFontChoiceDto::Custom {
                            font_id: meta.font_id.clone(),
                        }),
                        font_size_px: Some(20.0),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                "u".into(),
                true,
            )
            .unwrap();

        delete_custom_font(&store, &meta.font_id).unwrap();
        assert!(!store.fonts_dir().join(format!("{}.ttf", meta.font_id)).exists());

        let after = store.snapshot("u".into(), true);
        assert!(after.custom_fonts.is_empty());
        assert_eq!(after.global.typography.font, ReaderFontChoiceDto::Publisher);
        assert_eq!(
            after
                .book_overrides
                .get(&book_hash)
                .and_then(|p| p.typography.as_ref())
                .and_then(|t| t.font.clone()),
            Some(ReaderFontChoiceDto::Publisher)
        );
        assert_eq!(
            read_custom_font_bytes(&store, &meta.font_id)
                .await
                .unwrap_err(),
            ErrorCode::ReaderFontNotFound
        );

        // I/O read failure maps to ImportReadFailed rather than ReaderInvalidFontFile.
        struct FailingReader;
        impl AsyncRead for FailingReader {
            fn poll_read(
                self: std::pin::Pin<&mut Self>,
                _cx: &mut std::task::Context<'_>,
                _buf: &mut tokio::io::ReadBuf<'_>,
            ) -> std::task::Poll<std::io::Result<()>> {
                std::task::Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "simulated read error",
                )))
            }
        }
        let read_err = import_custom_font_from_reader(&store, "broken.ttf", FailingReader, None)
            .await
            .unwrap_err();
        assert_eq!(read_err, ErrorCode::ImportReadFailed);
    }

    #[tokio::test]
    async fn list_system_fonts_respects_capability_flag_and_caches_results() {
        assert!(list_system_fonts(false).await.is_empty());

        let first = list_system_fonts(true).await;
        assert!(first.iter().all(|item| !item.family.trim().is_empty()));
        let second = list_system_fonts(true).await;
        assert_eq!(first, second);
    }
}

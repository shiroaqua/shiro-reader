use std::{
    collections::{HashMap, HashSet},
    sync::Mutex,
};

use anyhow::Context;
use pdfium_render::prelude::{
    PdfAction, PdfBookmark, PdfDestination as PdfiumDestination, PdfDestinationViewSettings,
    PdfPageIndex, Pdfium,
};
use rbook::{Epub, epub::toc::EpubTocEntry};
use shiro_domain::library::book::{
    navigation::{BookNavigation, ReadingUnitDraft, TocEntryDraft},
    value_objects::{
        BookLocator, EpubResourcePath, PdfDestination, ReadingUnitOffset, TocEntryFlags,
    },
};

use super::book_file_storage::BookFileContentReader;

const MAX_TOC_DEPTH: usize = 256;
const MAX_TOC_NODES: usize = 100_000;

struct XhtmlMetrics {
    text_chars: u64,
    anchors: HashMap<String, u64>,
}

pub(super) fn extract_epub_navigation(
    content: BookFileContentReader,
    hash_hex: &str,
) -> anyhow::Result<BookNavigation> {
    let epub = Epub::read(content).context("failed to parse EPUB archive")?;
    let spine_len = epub.spine().len();
    let mut unit_specs = Vec::with_capacity(spine_len);
    let mut spine_metrics = Vec::with_capacity(spine_len);
    let mut id_to_spine: HashMap<String, u32> = HashMap::with_capacity(spine_len);
    let mut href_to_spine: HashMap<String, u32> = HashMap::with_capacity(spine_len);
    let mut decoded_href_to_spine: HashMap<String, u32> = HashMap::with_capacity(spine_len);

    for (idx, entry) in epub.spine().iter().enumerate() {
        let spine_idx =
            u32::try_from(idx).map_err(|_| anyhow::anyhow!("spine index overflow"))?;
        let is_linear = entry.is_linear();

        if let Some(manifest_entry) = entry.manifest_entry() {
            id_to_spine
                .entry(entry.idref().to_owned())
                .or_insert(spine_idx);

            let raw_path = manifest_entry.href().path().as_str();
            let normalized_href = match EpubResourcePath::normalize(raw_path) {
                Ok(href) => {
                    href_to_spine
                        .entry(href.as_str().to_owned())
                        .or_insert(spine_idx);
                    Some(href)
                }
                Err(err) => {
                    tracing::warn!(
                        hash = hash_hex,
                        spine_index = spine_idx,
                        href = %raw_path,
                        error = %err,
                        "failed to normalize EPUB spine href"
                    );
                    None
                }
            };

            let decoded_path = manifest_entry.href().path().decode();
            if let Some(decoded_key) = canonical_decoded_key(decoded_path.as_ref()) {
                decoded_href_to_spine
                    .entry(decoded_key)
                    .or_insert(spine_idx);
            }

            let media_type = manifest_entry.media_type();
            let is_markup = media_type.contains("html")
                || media_type.contains("xml")
                || media_type.starts_with("text/");

            let (weight, metrics) = match manifest_entry.read_str() {
                Ok(text) => {
                    let metrics = scan_xhtml_text_metrics(&text);
                    let weight = metrics.text_chars.max(1);
                    (weight, Some(metrics))
                }
                Err(_) if is_markup => match manifest_entry.read_bytes() {
                    Ok(bytes) => {
                        let text = String::from_utf8_lossy(&bytes);
                        let metrics = scan_xhtml_text_metrics(&text);
                        let weight = metrics.text_chars.max(1);
                        (weight, Some(metrics))
                    }
                    Err(err) => {
                        tracing::warn!(
                            hash = hash_hex,
                            spine_index = spine_idx,
                            href = %raw_path,
                            error = %err,
                            "failed to read EPUB spine markup entry"
                        );
                        (0, None)
                    }
                },
                Err(_) => match manifest_entry.read_bytes() {
                    Ok(_) => (1, None),
                    Err(err) => {
                        tracing::warn!(
                            hash = hash_hex,
                            spine_index = spine_idx,
                            href = %raw_path,
                            error = %err,
                            "failed to read EPUB spine resource entry"
                        );
                        (0, None)
                    }
                },
            };

            unit_specs.push(ReadingUnitDraft {
                is_linear,
                weight,
                display_label: normalized_href.map(String::from),
            });
            spine_metrics.push(metrics);
        } else {
            tracing::warn!(
                hash = hash_hex,
                spine_index = spine_idx,
                idref = entry.idref(),
                "EPUB spine entry references missing manifest item"
            );
            unit_specs.push(ReadingUnitDraft {
                is_linear,
                weight: 0,
                display_label: None,
            });
            spine_metrics.push(None);
        }
    }

    let toc_roots = if let Some(root) = epub.toc().contents() {
        let mut node_count = 0usize;
        let mut truncated_warned = false;
        collect_epub_toc_entries(
            &root,
            0,
            &mut node_count,
            &mut truncated_warned,
            hash_hex,
            &id_to_spine,
            &href_to_spine,
            &decoded_href_to_spine,
            &spine_metrics,
        )
    } else {
        Vec::new()
    };

    BookNavigation::build(toc_roots, unit_specs).context("failed to build EPUB navigation index")
}

#[allow(clippy::too_many_arguments)]
fn collect_epub_toc_entries(
    parent: &EpubTocEntry<'_>,
    depth: usize,
    node_count: &mut usize,
    truncated_warned: &mut bool,
    hash_hex: &str,
    id_to_spine: &HashMap<String, u32>,
    href_to_spine: &HashMap<String, u32>,
    decoded_href_to_spine: &HashMap<String, u32>,
    spine_metrics: &[Option<XhtmlMetrics>],
) -> Vec<TocEntryDraft> {
    let mut nodes = Vec::new();
    for entry in parent.iter() {
        if *node_count >= MAX_TOC_NODES {
            if !*truncated_warned {
                *truncated_warned = true;
                tracing::warn!(
                    hash = hash_hex,
                    limit = MAX_TOC_NODES,
                    "EPUB TOC node count exceeded limit, truncating remaining entries"
                );
            }
            break;
        }
        *node_count += 1;

        let children = if depth + 1 < MAX_TOC_DEPTH {
            collect_epub_toc_entries(
                &entry,
                depth + 1,
                node_count,
                truncated_warned,
                hash_hex,
                id_to_spine,
                href_to_spine,
                decoded_href_to_spine,
                spine_metrics,
            )
        } else {
            if !entry.is_empty() && !*truncated_warned {
                *truncated_warned = true;
                tracing::warn!(
                    hash = hash_hex,
                    limit = MAX_TOC_DEPTH,
                    "EPUB TOC depth exceeded limit, truncating nested entries"
                );
            }
            Vec::new()
        };

        let matched_manifest = entry.manifest_entry();
        let is_external = matched_manifest.is_none()
            && (entry
                .href_raw()
                .is_some_and(|href| is_external_href(href.as_str()))
                || entry
                    .href()
                    .is_some_and(|href| is_external_href(href.as_str())));

        let (flags, locator, unit_offset) = if is_external {
            (TocEntryFlags::EXTERNAL_LINK, None, ReadingUnitOffset::ZERO)
        } else if let Some(href) = entry.href() {
            let raw_path = href.path().as_str();
            match EpubResourcePath::normalize(raw_path) {
                Ok(normalized_href) => {
                    let spine_index = matched_manifest
                        .and_then(|m| id_to_spine.get(m.id()).copied())
                        .or_else(|| href_to_spine.get(normalized_href.as_str()).copied())
                        .or_else(|| {
                            let decoded = href.path().decode();
                            canonical_decoded_key(decoded.as_ref())
                                .and_then(|key| decoded_href_to_spine.get(&key).copied())
                        });
                    let fragment = href
                        .fragment()
                        .map(percent_decode_str)
                        .map(|s| s.trim().to_owned())
                        .filter(|s| !s.is_empty());

                    let unit_offset = spine_index
                        .and_then(|idx| spine_metrics.get(idx as usize)?.as_ref())
                        .and_then(|metrics| {
                            let frag = fragment.as_deref()?;
                            let pos = metrics.anchors.get(frag).copied()?;
                            if metrics.text_chars > 0 {
                                ReadingUnitOffset::clamped(pos as f32 / metrics.text_chars as f32)
                            } else {
                                None
                            }
                        })
                        .unwrap_or(ReadingUnitOffset::ZERO);

                    match BookLocator::epub(spine_index, normalized_href, fragment, None) {
                        Ok(locator) => (TocEntryFlags::empty(), Some(locator), unit_offset),
                        Err(err) => {
                            tracing::warn!(
                                hash = hash_hex,
                                title = entry.label(),
                                href = href.as_str(),
                                error = %err,
                                "failed to construct EPUB TOC locator"
                            );
                            (TocEntryFlags::empty(), None, ReadingUnitOffset::ZERO)
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(
                        hash = hash_hex,
                        title = entry.label(),
                        href = href.as_str(),
                        error = %err,
                        "failed to normalize EPUB TOC href"
                    );
                    (TocEntryFlags::empty(), None, ReadingUnitOffset::ZERO)
                }
            }
        } else {
            (TocEntryFlags::empty(), None, ReadingUnitOffset::ZERO)
        };

        nodes.push(TocEntryDraft {
            title: entry.label().to_owned(),
            locator,
            unit_offset,
            flags,
            children,
        });
    }
    nodes
}

fn canonical_decoded_key(raw: &str) -> Option<String> {
    if raw.ends_with('/') || raw.chars().any(|c| matches!(c, '\\' | '\0')) {
        return None;
    }
    let mut segments = Vec::new();
    for seg in raw.trim_start_matches('/').split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            _ => segments.push(seg),
        }
    }
    (!segments.is_empty()).then(|| segments.join("/"))
}

fn is_external_href(raw: &str) -> bool {
    const KNOWN_NON_HIERARCHICAL_SCHEMES: &[&str] = &[
        "mailto",
        "tel",
        "sms",
        "data",
        "javascript",
        "about",
        "urn",
        "blob",
        "geo",
    ];

    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.contains("://") {
        return true;
    }
    let before_delim = trimmed.split(['/', '?', '#']).next().unwrap_or("");
    let Some((scheme, _)) = before_delim.split_once(':') else {
        return false;
    };
    KNOWN_NON_HIERARCHICAL_SCHEMES
        .iter()
        .any(|known| scheme.eq_ignore_ascii_case(known))
}

fn percent_decode_str(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex_val(bytes[i + 1]), hex_val(bytes[i + 2]))
        {
            out.push((hi << 4) | lo);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn scan_xhtml_text_metrics(html: &str) -> XhtmlMetrics {
    let bytes = html.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;
    let mut visible_chars = 0u64;
    let mut has_emitted_char = false;
    let mut pending_space = false;
    let mut anchors = HashMap::new();

    let mut head_depth = 0usize;
    let mut script_depth = 0usize;
    let mut style_depth = 0usize;

    while i < len {
        if bytes[i] == b'<' {
            let rest = &bytes[i..];
            if rest.starts_with(b"<!--") {
                if let Some(end_rel) = find_subslice(rest, 4, b"-->") {
                    i += end_rel + 3;
                } else {
                    i = len;
                }
                continue;
            }
            if rest.starts_with(b"<![CDATA[") {
                let (cdata_text, next_i) = if let Some(end_rel) = find_subslice(rest, 9, b"]]>") {
                    (&html[i + 9..i + end_rel], i + end_rel + 3)
                } else {
                    (&html[i + 9..], len)
                };
                if head_depth == 0 && script_depth == 0 && style_depth == 0 {
                    count_visible_text(
                        cdata_text,
                        &mut visible_chars,
                        &mut has_emitted_char,
                        &mut pending_space,
                    );
                }
                i = next_i;
                continue;
            }
            if rest.starts_with(b"<?") {
                if let Some(end_rel) = find_subslice(rest, 2, b"?>") {
                    i += end_rel + 2;
                } else {
                    i = len;
                }
                continue;
            }
            if rest.starts_with(b"<!") {
                let decl_end = find_declaration_end(bytes, i + 2);
                i = if decl_end < len { decl_end + 1 } else { len };
                continue;
            }

            let tag_end = find_tag_end(bytes, i + 1);
            let tag_inner = &html[i + 1..tag_end];
            i = if tag_end < len { tag_end + 1 } else { len };

            let trimmed = tag_inner.trim_ascii_start();
            if let Some(close_rest) = trimmed.strip_prefix('/') {
                let name_end = close_rest
                    .find(|c: char| c.is_ascii_whitespace() || c == '/')
                    .unwrap_or(close_rest.len());
                let tag_name = &close_rest[..name_end];
                if tag_name.eq_ignore_ascii_case("head") {
                    head_depth = head_depth.saturating_sub(1);
                } else if tag_name.eq_ignore_ascii_case("script") {
                    script_depth = script_depth.saturating_sub(1);
                } else if tag_name.eq_ignore_ascii_case("style") {
                    style_depth = style_depth.saturating_sub(1);
                }
            } else if !trimmed.is_empty() {
                let self_closing = trimmed.trim_ascii_end().ends_with('/');
                let name_end = trimmed
                    .find(|c: char| c.is_ascii_whitespace() || c == '/')
                    .unwrap_or(trimmed.len());
                let tag_name = &trimmed[..name_end];

                if !self_closing {
                    if tag_name.eq_ignore_ascii_case("head") {
                        head_depth += 1;
                    } else if tag_name.eq_ignore_ascii_case("script") {
                        script_depth += 1;
                    } else if tag_name.eq_ignore_ascii_case("style") {
                        style_depth += 1;
                    }
                }

                if head_depth == 0 && script_depth == 0 && style_depth == 0 {
                    extract_id_and_name_attrs(
                        &trimmed[name_end..],
                        visible_chars,
                        &mut anchors,
                    );
                }
            }
            continue;
        }

        if head_depth > 0 || script_depth > 0 || style_depth > 0 {
            let ch = html[i..].chars().next().expect("valid utf-8 boundary");
            i += ch.len_utf8();
            continue;
        }

        if bytes[i] == b'&'
            && let Some(entity_len) = match_xml_entity(&bytes[i..])
        {
            record_non_whitespace(
                &mut visible_chars,
                &mut has_emitted_char,
                &mut pending_space,
            );
            i += entity_len;
            continue;
        }

        let ch = html[i..].chars().next().expect("valid utf-8 boundary");
        i += ch.len_utf8();
        push_visible_char(
            ch,
            &mut visible_chars,
            &mut has_emitted_char,
            &mut pending_space,
        );
    }

    XhtmlMetrics {
        text_chars: visible_chars,
        anchors,
    }
}

fn record_non_whitespace(
    visible_chars: &mut u64,
    has_emitted_char: &mut bool,
    pending_space: &mut bool,
) {
    if *pending_space {
        *visible_chars += 1;
        *pending_space = false;
    }
    *visible_chars += 1;
    *has_emitted_char = true;
}

fn push_visible_char(
    ch: char,
    visible_chars: &mut u64,
    has_emitted_char: &mut bool,
    pending_space: &mut bool,
) {
    if ch.is_whitespace() {
        if *has_emitted_char {
            *pending_space = true;
        }
    } else {
        record_non_whitespace(visible_chars, has_emitted_char, pending_space);
    }
}

fn count_visible_text(
    text: &str,
    visible_chars: &mut u64,
    has_emitted_char: &mut bool,
    pending_space: &mut bool,
) {
    for ch in text.chars() {
        push_visible_char(ch, visible_chars, has_emitted_char, pending_space);
    }
}

fn find_subslice(haystack: &[u8], start: usize, needle: &[u8]) -> Option<usize> {
    if haystack.len() < start + needle.len() {
        return None;
    }
    haystack[start..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|pos| start + pos)
}

fn find_declaration_end(bytes: &[u8], mut i: usize) -> usize {
    let mut quote: Option<u8> = None;
    let mut bracket_depth = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => {
                if bracket_depth > 0 && bytes[i..].starts_with(b"<!--") {
                    if let Some(end_rel) = find_subslice(&bytes[i..], 4, b"-->") {
                        i += end_rel + 3;
                        continue;
                    }
                    return bytes.len();
                }
                match b {
                    b'"' | b'\'' => quote = Some(b),
                    b'[' => bracket_depth += 1,
                    b']' => bracket_depth = bracket_depth.saturating_sub(1),
                    b'>' if bracket_depth == 0 => return i,
                    _ => {}
                }
            }
        }
        i += 1;
    }
    bytes.len()
}

fn find_tag_end(bytes: &[u8], mut i: usize) -> usize {
    let mut quote: Option<u8> = None;
    while i < bytes.len() {
        let b = bytes[i];
        match quote {
            Some(q) => {
                if b == q {
                    quote = None;
                }
            }
            None => match b {
                b'"' | b'\'' => quote = Some(b),
                b'>' => return i,
                _ => {}
            },
        }
        i += 1;
    }
    bytes.len()
}

fn match_xml_entity(bytes: &[u8]) -> Option<usize> {
    if bytes.first().copied() != Some(b'&') {
        return None;
    }
    let max_scan = bytes.len().min(18);
    let semi_idx = (1..max_scan).find(|&idx| {
        let b = bytes[idx];
        b == b';' || b.is_ascii_whitespace() || b == b'<' || b == b'&'
    })?;
    if bytes[semi_idx] != b';' || semi_idx <= 1 {
        return None;
    }
    let inner = &bytes[1..semi_idx];
    let valid = if let Some(num) = inner.strip_prefix(b"#") {
        if let Some(hex) = num.strip_prefix(b"x").or_else(|| num.strip_prefix(b"X")) {
            !hex.is_empty() && hex.iter().all(u8::is_ascii_hexdigit)
        } else {
            !num.is_empty() && num.iter().all(u8::is_ascii_digit)
        }
    } else {
        inner
            .first()
            .is_some_and(|b| b.is_ascii_alphabetic())
            && inner
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'_' || *b == b'-')
    };
    valid.then_some(semi_idx + 1)
}

fn extract_id_and_name_attrs(
    attrs: &str,
    visible_chars: u64,
    anchors: &mut HashMap<String, u64>,
) {
    let bytes = attrs.as_bytes();
    let len = bytes.len();
    let mut i = 0usize;

    while i < len {
        while i < len && (bytes[i].is_ascii_whitespace() || bytes[i] == b'/') {
            i += 1;
        }
        if i >= len {
            break;
        }

        let name_start = i;
        while i < len
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'='
            && bytes[i] != b'/'
        {
            i += 1;
        }
        let attr_name = &attrs[name_start..i];

        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len || bytes[i] != b'=' {
            continue;
        }
        i += 1;
        while i < len && bytes[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= len {
            break;
        }

        let raw_val = if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let val_start = i;
            while i < len && bytes[i] != quote {
                i += 1;
            }
            let val = &attrs[val_start..i];
            if i < len {
                i += 1;
            }
            val
        } else {
            let val_start = i;
            while i < len && !bytes[i].is_ascii_whitespace() && bytes[i] != b'/' {
                i += 1;
            }
            &attrs[val_start..i]
        };

        if attr_name.eq_ignore_ascii_case("id") || attr_name.eq_ignore_ascii_case("name") {
            let decoded = unescape_basic_xml_entities(raw_val).trim().to_owned();
            if !decoded.is_empty() {
                anchors.entry(decoded).or_insert(visible_chars);
            }
        }
    }
}

fn unescape_basic_xml_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_owned();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        if let Some(stripped) = after.strip_prefix("&amp;") {
            out.push('&');
            rest = stripped;
        } else if let Some(stripped) = after.strip_prefix("&lt;") {
            out.push('<');
            rest = stripped;
        } else if let Some(stripped) = after.strip_prefix("&gt;") {
            out.push('>');
            rest = stripped;
        } else if let Some(stripped) = after.strip_prefix("&quot;") {
            out.push('"');
            rest = stripped;
        } else if let Some(stripped) = after.strip_prefix("&apos;") {
            out.push('\'');
            rest = stripped;
        } else {
            out.push('&');
            rest = &after[1..];
        }
    }
    out.push_str(rest);
    out
}

pub(super) fn extract_pdf_navigation(
    content: BookFileContentReader,
    pdfium: &Mutex<Pdfium>,
    hash_hex: &str,
) -> anyhow::Result<BookNavigation> {
    let pdfium = pdfium
        .lock()
        .map_err(|_| anyhow::anyhow!("Pdfium lock poisoned"))?;
    let doc = pdfium
        .load_pdf_from_reader(content, None)
        .context("failed to load PDF document with Pdfium")?;

    let page_count_i32 = doc.pages().len();
    let page_count = usize::try_from(page_count_i32)
        .map_err(|_| anyhow::anyhow!("Pdfium returned negative page count: {page_count_i32}"))?;
    let mut unit_specs = Vec::with_capacity(page_count);
    let mut page_heights = Vec::with_capacity(page_count);

    for idx in 0..page_count {
        let page_idx = idx as PdfPageIndex;
        let (height, label) = match doc.pages().get(page_idx) {
            Ok(page) => (
                Some(page.height().value),
                page.label().map(str::to_owned),
            ),
            Err(_) => (
                doc.pages()
                    .page_size(page_idx)
                    .ok()
                    .map(|rect| rect.height().value),
                None,
            ),
        };
        page_heights.push(height);
        unit_specs.push(ReadingUnitDraft {
            is_linear: true,
            weight: 1,
            display_label: label,
        });
    }

    let mut visited = HashSet::new();
    let mut node_count = 0usize;
    let mut truncated_warned = false;
    let toc_roots = collect_pdf_bookmarks(
        doc.bookmarks().root(),
        page_count_i32,
        &page_heights,
        &mut visited,
        0,
        &mut node_count,
        &mut truncated_warned,
        hash_hex,
    );

    BookNavigation::build(toc_roots, unit_specs).context("failed to build PDF navigation index")
}

#[allow(clippy::too_many_arguments)]
fn collect_pdf_bookmarks<'a>(
    first: Option<PdfBookmark<'a>>,
    page_count: PdfPageIndex,
    page_heights: &[Option<f32>],
    visited: &mut HashSet<PdfBookmark<'a>>,
    depth: usize,
    node_count: &mut usize,
    truncated_warned: &mut bool,
    hash_hex: &str,
) -> Vec<TocEntryDraft> {
    let mut nodes = Vec::new();
    let mut current = first;

    while let Some(bookmark) = current {
        if *node_count >= MAX_TOC_NODES {
            if !*truncated_warned {
                *truncated_warned = true;
                tracing::warn!(
                    hash = hash_hex,
                    limit = MAX_TOC_NODES,
                    "PDF bookmark count exceeded limit, truncating remaining entries"
                );
            }
            break;
        }
        if !visited.insert(bookmark.clone()) {
            tracing::warn!(
                hash = hash_hex,
                "cycle detected in PDF bookmark outline, stopping branch traversal"
            );
            break;
        }
        *node_count += 1;

        let next = bookmark.next_sibling();
        let first_child = bookmark.first_child();
        let children = if depth + 1 < MAX_TOC_DEPTH {
            collect_pdf_bookmarks(
                first_child,
                page_count,
                page_heights,
                visited,
                depth + 1,
                node_count,
                truncated_warned,
                hash_hex,
            )
        } else {
            if first_child.is_some() && !*truncated_warned {
                *truncated_warned = true;
                tracing::warn!(
                    hash = hash_hex,
                    limit = MAX_TOC_DEPTH,
                    "PDF bookmark depth exceeded limit, truncating nested entries"
                );
            }
            Vec::new()
        };

        nodes.push(build_pdf_toc_entry(
            &bookmark,
            page_count,
            page_heights,
            children,
            hash_hex,
        ));
        current = next;
    }

    nodes
}

fn build_pdf_toc_entry(
    bookmark: &PdfBookmark<'_>,
    page_count: PdfPageIndex,
    page_heights: &[Option<f32>],
    children: Vec<TocEntryDraft>,
    hash_hex: &str,
) -> TocEntryDraft {
    let title = bookmark.title().unwrap_or_default();
    let (locator, unit_offset, flags) =
        resolve_pdf_bookmark_target(bookmark, &title, page_count, page_heights, hash_hex);

    TocEntryDraft {
        title,
        locator,
        unit_offset,
        flags,
        children,
    }
}

fn resolve_pdf_bookmark_target(
    bookmark: &PdfBookmark<'_>,
    title: &str,
    page_count: PdfPageIndex,
    page_heights: &[Option<f32>],
    hash_hex: &str,
) -> (Option<BookLocator>, ReadingUnitOffset, TocEntryFlags) {
    if let Some(dest) = bookmark.destination() {
        let (locator, unit_offset) =
            build_pdf_locator(&dest, title, page_count, page_heights, hash_hex);
        return (locator, unit_offset, TocEntryFlags::empty());
    }
    match bookmark.action() {
        Some(PdfAction::LocalDestination(local)) => match local.destination() {
            Ok(dest) => {
                let (locator, unit_offset) =
                    build_pdf_locator(&dest, title, page_count, page_heights, hash_hex);
                (locator, unit_offset, TocEntryFlags::empty())
            }
            Err(err) => {
                tracing::warn!(
                    hash = hash_hex,
                    title = title,
                    error = %err,
                    "failed to resolve PDF bookmark local destination"
                );
                (None, ReadingUnitOffset::ZERO, TocEntryFlags::empty())
            }
        },
        Some(
            PdfAction::Uri(_) | PdfAction::RemoteDestination(_) | PdfAction::Launch(_),
        ) => (None, ReadingUnitOffset::ZERO, TocEntryFlags::EXTERNAL_LINK),
        _ => (None, ReadingUnitOffset::ZERO, TocEntryFlags::empty()),
    }
}

fn build_pdf_locator(
    dest: &PdfiumDestination<'_>,
    title: &str,
    page_count: PdfPageIndex,
    page_heights: &[Option<f32>],
    hash_hex: &str,
) -> (Option<BookLocator>, ReadingUnitOffset) {
    match dest.page_index() {
        Ok(idx) if idx >= 0 && idx < page_count => {
            let page_index = idx as u32;
            let page_height = page_heights.get(idx as usize).copied().flatten();
            let pdf_dest = map_pdf_dest(dest, page_height);
            let unit_offset = pdf_dest.normalized_top().unwrap_or(ReadingUnitOffset::ZERO);
            match BookLocator::pdf(page_index, pdf_dest) {
                Ok(locator) => (Some(locator), unit_offset),
                Err(err) => {
                    tracing::warn!(
                        hash = hash_hex,
                        title = title,
                        page_index,
                        error = %err,
                        "failed to construct PDF TOC locator"
                    );
                    (None, ReadingUnitOffset::ZERO)
                }
            }
        }
        Ok(idx) => {
            tracing::warn!(
                hash = hash_hex,
                title = title,
                page_index = idx,
                page_count,
                "PDF bookmark points to out-of-bounds page index"
            );
            (None, ReadingUnitOffset::ZERO)
        }
        Err(err) => {
            tracing::warn!(
                hash = hash_hex,
                title = title,
                error = %err,
                "failed to resolve PDF bookmark target page index"
            );
            (None, ReadingUnitOffset::ZERO)
        }
    }
}

fn map_pdf_dest(dest: &PdfiumDestination<'_>, page_height: Option<f32>) -> PdfDestination {
    match dest.view_settings() {
        Ok(PdfDestinationViewSettings::SpecificCoordinatesAndZoom(left, top, zoom)) => {
            let left = left.map(|p| p.value).filter(|v| v.is_finite());
            let top = top.map(|p| p.value).filter(|v| v.is_finite());
            let zoom = zoom.filter(|v| v.is_finite() && *v > 0.0);
            let normalized_top = pdf_normalized_top(top, page_height);
            PdfDestination::xyz(left, top, zoom, normalized_top)
                .unwrap_or(PdfDestination::PageOnly)
        }
        Ok(
            PdfDestinationViewSettings::FitPageHorizontallyToWindow(top)
            | PdfDestinationViewSettings::FitBoundsHorizontallyToWindow(top),
        ) => {
            let top = top.map(|p| p.value).filter(|v| v.is_finite());
            let normalized_top = pdf_normalized_top(top, page_height);
            PdfDestination::fit_h(top, normalized_top).unwrap_or(PdfDestination::PageOnly)
        }
        Ok(
            PdfDestinationViewSettings::FitPageToWindow
            | PdfDestinationViewSettings::FitBoundsToWindow
            | PdfDestinationViewSettings::FitPageVerticallyToWindow(_)
            | PdfDestinationViewSettings::FitBoundsVerticallyToWindow(_)
            | PdfDestinationViewSettings::FitPageToRectangle(_),
        ) => PdfDestination::FitPage,
        Ok(PdfDestinationViewSettings::Unknown) | Err(_) => PdfDestination::PageOnly,
    }
}

fn pdf_normalized_top(top: Option<f32>, page_height: Option<f32>) -> Option<ReadingUnitOffset> {
    let top = top?;
    let height = page_height?;
    if !top.is_finite() || !height.is_finite() || height <= 0.0 {
        return None;
    }
    ReadingUnitOffset::clamped(1.0 - (top / height))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_xhtml_strips_tags_head_script_style_and_tracks_anchors() {
        let xhtml = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head>
  <title>Ignored Title</title>
  <style>body { color: red; }</style>
  <script>console.log("ignore > me");</script>
</head>
<body>
  <!-- comment should not count -->
  <h1 id="ch1" data-expr="a > b">  Hello   world  </h1>
  <p id="sec&amp;2">Next&nbsp;section</p>
  <a name="anchor-3">End</a>
</body>
</html>"#;

        let metrics = scan_xhtml_text_metrics(xhtml);
        // "Hello worldNext&sectionEnd" -> Wait: between </h1> and <p> there is "\n  " whitespace!
        // Visible sequence: "Hello world" (11) + " " (1) + "Next&section" (12) + " " (1) + "End" (3) = 28
        assert_eq!(metrics.text_chars, 28);
        assert_eq!(metrics.anchors.get("ch1"), Some(&0));
        assert_eq!(metrics.anchors.get("sec&2"), Some(&11));
        assert_eq!(metrics.anchors.get("anchor-3"), Some(&24));
    }

    #[test]
    fn percent_decode_decodes_utf8_and_preserves_invalid_or_plus() {
        assert_eq!(percent_decode_str("sec%20one+two"), "sec one+two");
        assert_eq!(percent_decode_str("%E7%AC%AC1%E7%AB%A0"), "第1章");
        assert_eq!(percent_decode_str("bad%ZZ%2"), "bad%ZZ%2");
    }

    #[test]
    fn external_href_detection_distinguishes_uri_schemes_from_epub_paths() {
        assert!(is_external_href("https://example.com/page"));
        assert!(is_external_href("http://example.com"));
        assert!(is_external_href("mailto:test@example.com"));
        assert!(is_external_href("TEL:+123456789"));
        assert!(is_external_href("javascript:void(0)"));
        assert!(is_external_href("ftp://files.example.com/a.zip"));
        assert!(is_external_href("/EPUB/https://example.com"));
        assert!(!is_external_href("OEBPS/Text/ch01.xhtml#sec1"));
        assert!(!is_external_href("../chapter2.xhtml"));
        assert!(!is_external_href("dir/chapter:1.xhtml#x"));
        assert!(!is_external_href("Ch01:Intro.xhtml"));
        assert!(!is_external_href("Part1:Overview.xhtml#s1"));
    }

    #[test]
    fn scan_xhtml_handles_doctype_internal_subset_without_leaking_text() {
        let xhtml = r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html PUBLIC "-//W3C//DTD XHTML 1.1//EN" "http://www.w3.org/TR/xhtml11/DTD/xhtml11.dtd" [
  <!ENTITY nbsp "&#160;">
  <!-- ] > comment inside doctype -->
  <!ENTITY gt ">">
]>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Ignored Title</title></head>
<body><h1 id="first">Hello</h1><p id="second">World</p></body>
</html>"#;

        let metrics = scan_xhtml_text_metrics(xhtml);
        assert_eq!(metrics.text_chars, 10);
        assert_eq!(metrics.anchors.get("first"), Some(&0));
        assert_eq!(metrics.anchors.get("second"), Some(&5));
    }

    #[test]
    fn scan_xhtml_counts_body_cdata_and_ignores_script_cdata() {
        let xhtml = r#"<html xmlns="http://www.w3.org/1999/xhtml">
<head><script><![CDATA[ignored script text]]></script></head>
<body><p id="start"><![CDATA[a < b & c]]></p><p id="after">end</p></body>
</html>"#;

        let metrics = scan_xhtml_text_metrics(xhtml);
        // "a < b & c" (9) + "end" (3) = 12
        assert_eq!(metrics.text_chars, 12);
        assert_eq!(metrics.anchors.get("start"), Some(&0));
        assert_eq!(metrics.anchors.get("after"), Some(&9));
    }
}

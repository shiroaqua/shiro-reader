use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::RwLock,
};

use anyhow::{Context, bail};
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use shiro_error_catalog::ErrorCode;
use tempfile::NamedTempFile;
use tracing::warn;

pub const READER_SETTINGS_SCHEMA_VERSION: u32 = 1;
pub const READER_SETTINGS_FILE_NAME: &str = "reader_settings.json";
pub const READER_FONTS_DIR_NAME: &str = "reader_fonts";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderSurfacePreset {
    #[default]
    FollowApp,
    PaperLight,
    SepiaWarm,
    EyeGreen,
    NightDark,
    AmoledBlack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderFontPreset {
    Sans,
    Serif,
    Mono,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum ReaderFontChoiceDto {
    #[default]
    Publisher,
    Preset {
        preset: ReaderFontPreset,
    },
    System {
        family: String,
    },
    Custom {
        font_id: String,
    },
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderTextAlign {
    #[default]
    Auto,
    Start,
    Justify,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderHyphenation {
    #[default]
    Auto,
    None,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderFlowMode {
    #[default]
    Paginated,
    Scrolled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReaderFontWeightDto {
    #[default]
    Auto,
    Weight(u16),
}

impl Serialize for ReaderFontWeightDto {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Weight(w) => serializer.serialize_u16(*w),
        }
    }
}

impl<'de> Deserialize<'de> for ReaderFontWeightDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = ReaderFontWeightDto;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(r#""auto" or a numeric font weight"#)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value == "auto" {
                    Ok(ReaderFontWeightDto::Auto)
                } else {
                    Err(E::custom(format!("unexpected fontWeight string: {value}")))
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                let clamped = u16::try_from(value).unwrap_or(u16::MAX);
                Ok(ReaderFontWeightDto::Weight(clamped))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                if value <= 0 {
                    Ok(ReaderFontWeightDto::Weight(0))
                } else {
                    let clamped = u16::try_from(value).unwrap_or(u16::MAX);
                    Ok(ReaderFontWeightDto::Weight(clamped))
                }
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                if !value.is_finite() {
                    Ok(ReaderFontWeightDto::Auto)
                } else if value <= 0.0 {
                    Ok(ReaderFontWeightDto::Weight(0))
                } else {
                    Ok(ReaderFontWeightDto::Weight(
                        value.round().clamp(0.0, f64::from(u16::MAX)) as u16,
                    ))
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum ReaderParagraphIndentDto {
    #[default]
    Auto,
    Em(f64),
}

impl Serialize for ReaderParagraphIndentDto {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::Em(em) => serializer.serialize_f64(*em),
        }
    }
}

impl<'de> Deserialize<'de> for ReaderParagraphIndentDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = ReaderParagraphIndentDto;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(r#""auto" or a numeric paragraph indent in em"#)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value == "auto" {
                    Ok(ReaderParagraphIndentDto::Auto)
                } else {
                    Err(E::custom(format!(
                        "unexpected paragraphIndentEm string: {value}"
                    )))
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(ReaderParagraphIndentDto::Em(value as f64))
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(ReaderParagraphIndentDto::Em(value as f64))
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                Ok(ReaderParagraphIndentDto::Em(value))
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ReaderColumnCountDto {
    #[default]
    Auto,
    One,
    Two,
}

impl Serialize for ReaderColumnCountDto {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Auto => serializer.serialize_str("auto"),
            Self::One => serializer.serialize_u8(1),
            Self::Two => serializer.serialize_u8(2),
        }
    }
}

impl<'de> Deserialize<'de> for ReaderColumnCountDto {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = ReaderColumnCountDto;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str(r#""auto", 1, or 2"#)
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
                if value == "auto" {
                    Ok(ReaderColumnCountDto::Auto)
                } else {
                    Err(E::custom(format!("unexpected columnCount string: {value}")))
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
                match value {
                    1 => Ok(ReaderColumnCountDto::One),
                    2 => Ok(ReaderColumnCountDto::Two),
                    _ => Err(E::custom(format!("invalid columnCount number: {value}"))),
                }
            }

            fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
                match value {
                    1 => Ok(ReaderColumnCountDto::One),
                    2 => Ok(ReaderColumnCountDto::Two),
                    _ => Err(E::custom(format!("invalid columnCount number: {value}"))),
                }
            }

            fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
                if value.is_finite() && value.fract() == 0.0 {
                    match value as i64 {
                        1 => Ok(ReaderColumnCountDto::One),
                        2 => Ok(ReaderColumnCountDto::Two),
                        _ => Err(E::custom(format!("invalid columnCount number: {value}"))),
                    }
                } else {
                    Err(E::custom(format!("invalid columnCount number: {value}")))
                }
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CustomFontFormat {
    Ttf,
    Otf,
}

impl CustomFontFormat {
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Ttf => "ttf",
            Self::Otf => "otf",
        }
    }

    pub const fn mime_type(self) -> &'static str {
        match self {
            Self::Ttf => "font/ttf",
            Self::Otf => "font/otf",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFontMetaDto {
    pub font_id: String,
    pub family_name: String,
    pub file_name: String,
    pub format: CustomFontFormat,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderAppearanceSettingsDto {
    pub surface: ReaderSurfacePreset,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderTypographySettingsDto {
    pub font: ReaderFontChoiceDto,
    pub font_size_px: f64,
    pub font_weight: ReaderFontWeightDto,
    pub line_height: f64,
    pub letter_spacing_em: f64,
    pub word_spacing_em: f64,
    pub text_align: ReaderTextAlign,
    pub hyphenation: ReaderHyphenation,
    pub override_publisher_styles: bool,
}

impl Default for ReaderTypographySettingsDto {
    fn default() -> Self {
        Self {
            font: ReaderFontChoiceDto::Publisher,
            font_size_px: 18.0,
            font_weight: ReaderFontWeightDto::Auto,
            line_height: 1.75,
            letter_spacing_em: 0.0,
            word_spacing_em: 0.0,
            text_align: ReaderTextAlign::Auto,
            hyphenation: ReaderHyphenation::Auto,
            override_publisher_styles: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderLayoutSettingsDto {
    pub paragraph_spacing_em: f64,
    pub paragraph_indent_em: ReaderParagraphIndentDto,
    pub page_margin_inline_px: f64,
    pub page_margin_block_px: f64,
    pub max_inline_size_px: f64,
    pub column_count: ReaderColumnCountDto,
    pub column_gap_percent: f64,
}

impl ReaderLayoutSettingsDto {
    pub fn default_for_platform(is_mobile: bool) -> Self {
        Self {
            paragraph_spacing_em: 0.75,
            paragraph_indent_em: ReaderParagraphIndentDto::Auto,
            page_margin_inline_px: 8.0,
            page_margin_block_px: 8.0,
            max_inline_size_px: 760.0,
            column_count: if is_mobile {
                ReaderColumnCountDto::One
            } else {
                ReaderColumnCountDto::Auto
            },
            column_gap_percent: 6.0,
        }
    }
}

impl Default for ReaderLayoutSettingsDto {
    fn default() -> Self {
        Self::default_for_platform(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderBehaviorSettingsDto {
    pub flow: ReaderFlowMode,
    pub animated_page_turn: bool,
    pub tap_zones_enabled: bool,
    pub show_page_number: bool,
    pub seamless_scroll: bool,
}

impl Default for ReaderBehaviorSettingsDto {
    fn default() -> Self {
        Self {
            flow: ReaderFlowMode::Paginated,
            animated_page_turn: true,
            tap_zones_enabled: false,
            show_page_number: true,
            seamless_scroll: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderProfileDto {
    pub appearance: ReaderAppearanceSettingsDto,
    pub typography: ReaderTypographySettingsDto,
    pub layout: ReaderLayoutSettingsDto,
    pub behavior: ReaderBehaviorSettingsDto,
}

impl ReaderProfileDto {
    pub fn default_for_platform(is_mobile: bool) -> Self {
        Self {
            appearance: ReaderAppearanceSettingsDto::default(),
            typography: ReaderTypographySettingsDto::default(),
            layout: ReaderLayoutSettingsDto::default_for_platform(is_mobile),
            behavior: ReaderBehaviorSettingsDto::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderAppearancePatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub surface: Option<ReaderSurfacePreset>,
}

impl ReaderAppearancePatchDto {
    pub fn is_empty(&self) -> bool {
        self.surface.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderTypographyPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font: Option<ReaderFontChoiceDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size_px: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_weight: Option<ReaderFontWeightDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub letter_spacing_em: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub word_spacing_em: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_align: Option<ReaderTextAlign>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyphenation: Option<ReaderHyphenation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_publisher_styles: Option<bool>,
}

impl ReaderTypographyPatchDto {
    pub fn is_empty(&self) -> bool {
        self.font.is_none()
            && self.font_size_px.is_none()
            && self.font_weight.is_none()
            && self.line_height.is_none()
            && self.letter_spacing_em.is_none()
            && self.word_spacing_em.is_none()
            && self.text_align.is_none()
            && self.hyphenation.is_none()
            && self.override_publisher_styles.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderLayoutPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_spacing_em: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_indent_em: Option<ReaderParagraphIndentDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_margin_inline_px: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_margin_block_px: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_inline_size_px: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_count: Option<ReaderColumnCountDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column_gap_percent: Option<f64>,
}

impl ReaderLayoutPatchDto {
    pub fn is_empty(&self) -> bool {
        self.paragraph_spacing_em.is_none()
            && self.paragraph_indent_em.is_none()
            && self.page_margin_inline_px.is_none()
            && self.page_margin_block_px.is_none()
            && self.max_inline_size_px.is_none()
            && self.column_count.is_none()
            && self.column_gap_percent.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderBehaviorPatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flow: Option<ReaderFlowMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animated_page_turn: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tap_zones_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_page_number: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seamless_scroll: Option<bool>,
}

impl ReaderBehaviorPatchDto {
    pub fn is_empty(&self) -> bool {
        self.flow.is_none()
            && self.animated_page_turn.is_none()
            && self.tap_zones_enabled.is_none()
            && self.show_page_number.is_none()
            && self.seamless_scroll.is_none()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderProfilePatchDto {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub appearance: Option<ReaderAppearancePatchDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub typography: Option<ReaderTypographyPatchDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout: Option<ReaderLayoutPatchDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<ReaderBehaviorPatchDto>,
}

impl ReaderProfilePatchDto {
    pub fn is_empty(&self) -> bool {
        self.appearance
            .as_ref()
            .is_none_or(ReaderAppearancePatchDto::is_empty)
            && self
                .typography
                .as_ref()
                .is_none_or(ReaderTypographyPatchDto::is_empty)
            && self
                .layout
                .as_ref()
                .is_none_or(ReaderLayoutPatchDto::is_empty)
            && self
                .behavior
                .as_ref()
                .is_none_or(ReaderBehaviorPatchDto::is_empty)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderSettingsConfig {
    pub schema_version: u32,
    #[serde(default)]
    pub global: ReaderProfileDto,
    #[serde(default)]
    pub book_overrides: BTreeMap<String, ReaderProfilePatchDto>,
    #[serde(default)]
    pub custom_fonts: Vec<CustomFontMetaDto>,
}

impl ReaderSettingsConfig {
    pub fn initial(is_mobile: bool) -> Self {
        Self {
            schema_version: READER_SETTINGS_SCHEMA_VERSION,
            global: ReaderProfileDto::default_for_platform(is_mobile),
            book_overrides: BTreeMap::new(),
            custom_fonts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReaderSettingsSnapshotDto {
    pub global: ReaderProfileDto,
    pub book_overrides: BTreeMap<String, ReaderProfilePatchDto>,
    pub custom_fonts: Vec<CustomFontMetaDto>,
    pub font_url_base: String,
    pub can_enumerate_system_fonts: bool,
}

/// Validates and normalizes a BLAKE3 hex hash string into lowercase 64-char hex.
pub fn normalize_book_hash(raw: &str) -> Result<String, ErrorCode> {
    let trimmed = raw.trim();
    let parsed = blake3::Hash::from_hex(trimmed.as_bytes())
        .map_err(|_| ErrorCode::BookFileInvalidHashFormat)?;
    Ok(parsed.to_hex().to_string())
}

fn clamp_finite_f64(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if value.is_finite() {
        value.clamp(min, max)
    } else {
        fallback
    }
}

fn clamp_optional_finite_f64(value: Option<f64>, min: f64, max: f64) -> Option<f64> {
    value.and_then(|v| v.is_finite().then(|| v.clamp(min, max)))
}

fn sanitize_font_weight(weight: ReaderFontWeightDto) -> ReaderFontWeightDto {
    match weight {
        ReaderFontWeightDto::Auto => ReaderFontWeightDto::Auto,
        ReaderFontWeightDto::Weight(raw) => {
            let clamped = raw.clamp(300, 700);
            let snapped = ((clamped + 50) / 100) * 100;
            ReaderFontWeightDto::Weight(snapped.clamp(300, 700))
        }
    }
}

fn sanitize_paragraph_indent(
    indent: ReaderParagraphIndentDto,
    fallback: ReaderParagraphIndentDto,
) -> ReaderParagraphIndentDto {
    match indent {
        ReaderParagraphIndentDto::Auto => ReaderParagraphIndentDto::Auto,
        ReaderParagraphIndentDto::Em(em) => {
            if em.is_finite() {
                ReaderParagraphIndentDto::Em(em.clamp(0.0, 4.0))
            } else {
                fallback
            }
        }
    }
}

fn sanitize_font_choice(
    choice: ReaderFontChoiceDto,
    valid_font_ids: Option<&HashSet<&str>>,
) -> ReaderFontChoiceDto {
    match choice {
        ReaderFontChoiceDto::Publisher => ReaderFontChoiceDto::Publisher,
        ReaderFontChoiceDto::Preset { preset } => ReaderFontChoiceDto::Preset { preset },
        ReaderFontChoiceDto::System { family } => {
            let trimmed = family.trim();
            if trimmed.is_empty() {
                ReaderFontChoiceDto::Publisher
            } else {
                ReaderFontChoiceDto::System {
                    family: trimmed.to_owned(),
                }
            }
        }
        ReaderFontChoiceDto::Custom { font_id } => {
            let Ok(normalized) = normalize_book_hash(&font_id) else {
                return ReaderFontChoiceDto::Publisher;
            };
            if let Some(valid) = valid_font_ids
                && !valid.contains(normalized.as_str())
            {
                return ReaderFontChoiceDto::Publisher;
            }
            ReaderFontChoiceDto::Custom {
                font_id: normalized,
            }
        }
    }
}

pub fn sanitize_profile(
    profile: ReaderProfileDto,
    defaults: &ReaderProfileDto,
    valid_font_ids: Option<&HashSet<&str>>,
) -> ReaderProfileDto {
    ReaderProfileDto {
        appearance: profile.appearance,
        typography: ReaderTypographySettingsDto {
            font: sanitize_font_choice(profile.typography.font, valid_font_ids),
            font_size_px: clamp_finite_f64(
                profile.typography.font_size_px,
                12.0,
                48.0,
                defaults.typography.font_size_px,
            ),
            font_weight: sanitize_font_weight(profile.typography.font_weight),
            line_height: clamp_finite_f64(
                profile.typography.line_height,
                1.2,
                2.4,
                defaults.typography.line_height,
            ),
            letter_spacing_em: clamp_finite_f64(
                profile.typography.letter_spacing_em,
                -0.05,
                0.3,
                defaults.typography.letter_spacing_em,
            ),
            word_spacing_em: clamp_finite_f64(
                profile.typography.word_spacing_em,
                0.0,
                0.5,
                defaults.typography.word_spacing_em,
            ),
            text_align: profile.typography.text_align,
            hyphenation: profile.typography.hyphenation,
            override_publisher_styles: profile.typography.override_publisher_styles,
        },
        layout: ReaderLayoutSettingsDto {
            paragraph_spacing_em: clamp_finite_f64(
                profile.layout.paragraph_spacing_em,
                0.0,
                2.5,
                defaults.layout.paragraph_spacing_em,
            ),
            paragraph_indent_em: sanitize_paragraph_indent(
                profile.layout.paragraph_indent_em,
                defaults.layout.paragraph_indent_em,
            ),
            page_margin_inline_px: clamp_finite_f64(
                profile.layout.page_margin_inline_px,
                8.0,
                120.0,
                defaults.layout.page_margin_inline_px,
            ),
            page_margin_block_px: clamp_finite_f64(
                profile.layout.page_margin_block_px,
                8.0,
                96.0,
                defaults.layout.page_margin_block_px,
            ),
            max_inline_size_px: clamp_finite_f64(
                profile.layout.max_inline_size_px,
                480.0,
                1400.0,
                defaults.layout.max_inline_size_px,
            ),
            column_count: profile.layout.column_count,
            column_gap_percent: clamp_finite_f64(
                profile.layout.column_gap_percent,
                3.0,
                12.0,
                defaults.layout.column_gap_percent,
            ),
        },
        behavior: profile.behavior,
    }
}

pub fn sanitize_patch(
    patch: ReaderProfilePatchDto,
    valid_font_ids: Option<&HashSet<&str>>,
) -> ReaderProfilePatchDto {
    let appearance = patch.appearance.filter(|a| !a.is_empty());

    let typography = patch
        .typography
        .map(|t| ReaderTypographyPatchDto {
            font: t.font.map(|choice| sanitize_font_choice(choice, valid_font_ids)),
            font_size_px: clamp_optional_finite_f64(t.font_size_px, 12.0, 48.0),
            font_weight: t.font_weight.map(sanitize_font_weight),
            line_height: clamp_optional_finite_f64(t.line_height, 1.2, 2.4),
            letter_spacing_em: clamp_optional_finite_f64(t.letter_spacing_em, -0.05, 0.3),
            word_spacing_em: clamp_optional_finite_f64(t.word_spacing_em, 0.0, 0.5),
            text_align: t.text_align,
            hyphenation: t.hyphenation,
            override_publisher_styles: t.override_publisher_styles,
        })
        .filter(|t| !t.is_empty());

    let layout = patch
        .layout
        .map(|l| ReaderLayoutPatchDto {
            paragraph_spacing_em: clamp_optional_finite_f64(l.paragraph_spacing_em, 0.0, 2.5),
            paragraph_indent_em: l.paragraph_indent_em.and_then(|indent| match indent {
                ReaderParagraphIndentDto::Auto => Some(ReaderParagraphIndentDto::Auto),
                ReaderParagraphIndentDto::Em(em) => {
                    em.is_finite().then(|| ReaderParagraphIndentDto::Em(em.clamp(0.0, 4.0)))
                }
            }),
            page_margin_inline_px: clamp_optional_finite_f64(l.page_margin_inline_px, 8.0, 120.0),
            page_margin_block_px: clamp_optional_finite_f64(l.page_margin_block_px, 8.0, 96.0),
            max_inline_size_px: clamp_optional_finite_f64(l.max_inline_size_px, 480.0, 1400.0),
            column_count: l.column_count,
            column_gap_percent: clamp_optional_finite_f64(l.column_gap_percent, 3.0, 12.0),
        })
        .filter(|l| !l.is_empty());

    let behavior = patch.behavior.filter(|b| !b.is_empty());

    ReaderProfilePatchDto {
        appearance,
        typography,
        layout,
        behavior,
    }
}

/// Pure-local client reader settings and custom font manifest store (`reader_settings.json`).
///
/// Kept completely isolated from library SQLite databases and cloud credentials so reader
/// preferences survive mode switches, library root repairs, and cloud logouts.
pub struct ReaderConfigStore {
    config_dir: PathBuf,
    settings_path: PathBuf,
    fonts_dir: PathBuf,
    default_profile: ReaderProfileDto,
    state: RwLock<ReaderSettingsConfig>,
}

impl ReaderConfigStore {
    pub fn load_or_default(config_dir: impl Into<PathBuf>, is_mobile: bool) -> Self {
        let config_dir = config_dir.into();
        let settings_path = config_dir.join(READER_SETTINGS_FILE_NAME);
        let fonts_dir = config_dir.join(READER_FONTS_DIR_NAME);
        let default_profile = ReaderProfileDto::default_for_platform(is_mobile);
        let initial = Self::read_and_sanitize_or_default(&settings_path, is_mobile, &default_profile);

        Self {
            config_dir,
            settings_path,
            fonts_dir,
            default_profile,
            state: RwLock::new(initial),
        }
    }

    pub fn settings_path(&self) -> &Path {
        &self.settings_path
    }

    pub fn fonts_dir(&self) -> &Path {
        &self.fonts_dir
    }

    pub fn snapshot(
        &self,
        font_url_base: String,
        can_enumerate_system_fonts: bool,
    ) -> ReaderSettingsSnapshotDto {
        let guard = self.state.read().unwrap_or_else(|e| e.into_inner());
        Self::snapshot_from_config(&guard, font_url_base, can_enumerate_system_fonts)
    }

    pub fn update_global(
        &self,
        profile: ReaderProfileDto,
        font_url_base: String,
        can_enumerate_system_fonts: bool,
    ) -> anyhow::Result<ReaderSettingsSnapshotDto> {
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        let valid_font_ids: HashSet<&str> = next
            .custom_fonts
            .iter()
            .map(|font| font.font_id.as_str())
            .collect();
        next.global = sanitize_profile(profile, &self.default_profile, Some(&valid_font_ids));
        self.persist_config(&next)?;
        *guard = next;
        Ok(Self::snapshot_from_config(
            &guard,
            font_url_base,
            can_enumerate_system_fonts,
        ))
    }

    pub fn set_book_override(
        &self,
        book_hash: &str,
        patch: ReaderProfilePatchDto,
        font_url_base: String,
        can_enumerate_system_fonts: bool,
    ) -> Result<ReaderSettingsSnapshotDto, ErrorCode> {
        let normalized_hash = normalize_book_hash(book_hash)?;
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        let valid_font_ids: HashSet<&str> = next
            .custom_fonts
            .iter()
            .map(|font| font.font_id.as_str())
            .collect();
        let sanitized = sanitize_patch(patch, Some(&valid_font_ids));

        if sanitized.is_empty() {
            next.book_overrides.remove(&normalized_hash);
        } else {
            next.book_overrides.insert(normalized_hash, sanitized);
        }

        if let Err(err) = self.persist_config(&next) {
            warn!(error = %err, "failed to persist reader book override");
            return Err(ErrorCode::ConfigSaveFailed);
        }
        *guard = next;
        Ok(Self::snapshot_from_config(
            &guard,
            font_url_base,
            can_enumerate_system_fonts,
        ))
    }

    pub fn reset_book_override(
        &self,
        book_hash: &str,
        font_url_base: String,
        can_enumerate_system_fonts: bool,
    ) -> Result<ReaderSettingsSnapshotDto, ErrorCode> {
        let normalized_hash = normalize_book_hash(book_hash)?;
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        let removed = next.book_overrides.remove(&normalized_hash).is_some();
        if removed && let Err(err) = self.persist_config(&next) {
            warn!(error = %err, "failed to persist reader book override reset");
            return Err(ErrorCode::ConfigSaveFailed);
        }
        *guard = next;
        Ok(Self::snapshot_from_config(
            &guard,
            font_url_base,
            can_enumerate_system_fonts,
        ))
    }

    pub fn find_custom_font(&self, font_id: &str) -> Option<CustomFontMetaDto> {
        let normalized = normalize_book_hash(font_id).ok()?;
        let guard = self.state.read().unwrap_or_else(|e| e.into_inner());
        guard
            .custom_fonts
            .iter()
            .find(|font| font.font_id == normalized)
            .cloned()
    }

    pub fn upsert_custom_font(&self, meta: CustomFontMetaDto) -> anyhow::Result<()> {
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        if let Some(existing) = next
            .custom_fonts
            .iter_mut()
            .find(|font| font.font_id == meta.font_id)
        {
            *existing = meta;
        } else {
            next.custom_fonts.push(meta);
        }
        self.persist_config(&next)?;
        *guard = next;
        Ok(())
    }

    pub fn remove_custom_font_and_fallback_references(
        &self,
        font_id: &str,
    ) -> Result<CustomFontMetaDto, ErrorCode> {
        let normalized = normalize_book_hash(font_id)?;
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        let mut next = guard.clone();
        let Some(pos) = next
            .custom_fonts
            .iter()
            .position(|font| font.font_id == normalized)
        else {
            return Err(ErrorCode::ReaderFontNotFound);
        };
        let removed = next.custom_fonts.remove(pos);

        if let ReaderFontChoiceDto::Custom { font_id } = &next.global.typography.font
            && font_id == &normalized
        {
            next.global.typography.font = ReaderFontChoiceDto::Publisher;
        }

        let empty_keys: Vec<String> = next
            .book_overrides
            .iter_mut()
            .filter_map(|(key, patch)| {
                if let Some(typography) = patch.typography.as_mut()
                    && let Some(ReaderFontChoiceDto::Custom { font_id }) = &typography.font
                    && font_id == &normalized
                {
                    typography.font = Some(ReaderFontChoiceDto::Publisher);
                }
                if patch.is_empty() {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();
        for key in empty_keys {
            next.book_overrides.remove(&key);
        }

        if let Err(err) = self.persist_config(&next) {
            warn!(error = %err, "failed to persist custom font deletion");
            return Err(ErrorCode::ConfigSaveFailed);
        }
        *guard = next;
        Ok(removed)
    }

    fn read_and_sanitize_or_default(
        settings_path: &Path,
        is_mobile: bool,
        default_profile: &ReaderProfileDto,
    ) -> ReaderSettingsConfig {
        let raw = match fs::read(settings_path) {
            Ok(bytes) => bytes,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return ReaderSettingsConfig::initial(is_mobile);
            }
            Err(err) => {
                warn!(
                    path = %settings_path.display(),
                    error = %err,
                    "failed to read reader settings file; falling back to defaults"
                );
                return ReaderSettingsConfig::initial(is_mobile);
            }
        };

        let parsed: ReaderSettingsConfig = match serde_json::from_slice(&raw) {
            Ok(cfg) => cfg,
            Err(err) => {
                warn!(
                    path = %settings_path.display(),
                    error = %err,
                    "failed to parse reader settings file; falling back to defaults"
                );
                return ReaderSettingsConfig::initial(is_mobile);
            }
        };

        if parsed.schema_version != READER_SETTINGS_SCHEMA_VERSION {
            warn!(
                path = %settings_path.display(),
                schema_version = parsed.schema_version,
                "unsupported reader settings schema version; falling back to defaults"
            );
            return ReaderSettingsConfig::initial(is_mobile);
        }

        let mut custom_fonts = Vec::new();
        let mut seen_fonts = HashSet::new();
        for mut font in parsed.custom_fonts {
            if let Ok(normalized) = normalize_book_hash(&font.font_id)
                && seen_fonts.insert(normalized.clone())
            {
                font.font_id = normalized;
                custom_fonts.push(font);
            }
        }
        let valid_font_ids: HashSet<&str> =
            custom_fonts.iter().map(|f| f.font_id.as_str()).collect();

        let global = sanitize_profile(parsed.global, default_profile, Some(&valid_font_ids));

        let mut book_overrides = BTreeMap::new();
        for (raw_hash, patch) in parsed.book_overrides {
            if let Ok(normalized_hash) = normalize_book_hash(&raw_hash) {
                let sanitized = sanitize_patch(patch, Some(&valid_font_ids));
                if !sanitized.is_empty() {
                    book_overrides.insert(normalized_hash, sanitized);
                }
            }
        }

        ReaderSettingsConfig {
            schema_version: READER_SETTINGS_SCHEMA_VERSION,
            global,
            book_overrides,
            custom_fonts,
        }
    }

    fn snapshot_from_config(
        config: &ReaderSettingsConfig,
        font_url_base: String,
        can_enumerate_system_fonts: bool,
    ) -> ReaderSettingsSnapshotDto {
        ReaderSettingsSnapshotDto {
            global: config.global.clone(),
            book_overrides: config.book_overrides.clone(),
            custom_fonts: config.custom_fonts.clone(),
            font_url_base,
            can_enumerate_system_fonts,
        }
    }

    fn persist_config(&self, config: &ReaderSettingsConfig) -> anyhow::Result<()> {
        fs::create_dir_all(&self.config_dir).with_context(|| {
            format!(
                "failed to create reader config directory {}",
                self.config_dir.display()
            )
        })?;

        let serialized = serde_json::to_vec_pretty(config)
            .context("failed to serialize reader configuration")?;

        let mut temp = NamedTempFile::new_in(&self.config_dir).with_context(|| {
            format!(
                "failed to create temp reader config file in {}",
                self.config_dir.display()
            )
        })?;
        temp.write_all(&serialized)
            .context("failed to write temporary reader config file")?;
        temp.as_file()
            .sync_all()
            .context("failed to sync temporary reader config file")?;
        if let Err(err) = temp.persist(&self.settings_path) {
            bail!(
                "failed to persist reader config file {}: {}",
                self.settings_path.display(),
                err.error
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font_url() -> String {
        "shiro-font://localhost/font/".to_owned()
    }

    #[test]
    fn default_profile_differs_between_desktop_and_mobile() {
        let desktop = ReaderProfileDto::default_for_platform(false);
        let mobile = ReaderProfileDto::default_for_platform(true);
        assert_eq!(desktop.layout.page_margin_inline_px, 8.0);
        assert_eq!(desktop.layout.page_margin_block_px, 8.0);
        assert_eq!(desktop.layout.column_count, ReaderColumnCountDto::Auto);
        assert!(!desktop.behavior.tap_zones_enabled);
        assert!(desktop.behavior.show_page_number);
        assert!(!desktop.behavior.seamless_scroll);

        assert_eq!(mobile.layout.page_margin_inline_px, 8.0);
        assert_eq!(mobile.layout.page_margin_block_px, 8.0);
        assert_eq!(mobile.layout.column_count, ReaderColumnCountDto::One);
        assert!(!mobile.behavior.tap_zones_enabled);
        assert!(mobile.behavior.show_page_number);
        assert!(!mobile.behavior.seamless_scroll);
    }

    #[test]
    fn sanitize_clamps_ranges_and_handles_non_finite_floats() {
        let defaults = ReaderProfileDto::default_for_platform(false);
        let input = ReaderProfileDto {
            appearance: ReaderAppearanceSettingsDto {
                surface: ReaderSurfacePreset::NightDark,
            },
            typography: ReaderTypographySettingsDto {
                font: ReaderFontChoiceDto::System {
                    family: "   ".to_owned(),
                },
                font_size_px: f64::NAN,
                font_weight: ReaderFontWeightDto::Weight(460),
                line_height: 99.0,
                letter_spacing_em: -1.0,
                word_spacing_em: f64::INFINITY,
                text_align: ReaderTextAlign::Justify,
                hyphenation: ReaderHyphenation::None,
                override_publisher_styles: true,
            },
            layout: ReaderLayoutSettingsDto {
                paragraph_spacing_em: -2.0,
                paragraph_indent_em: ReaderParagraphIndentDto::Em(f64::NAN),
                page_margin_inline_px: 500.0,
                page_margin_block_px: 1.0,
                max_inline_size_px: 100.0,
                column_count: ReaderColumnCountDto::Two,
                column_gap_percent: 42.0,
            },
            behavior: ReaderBehaviorSettingsDto::default(),
        };

        let out = sanitize_profile(input, &defaults, None);
        assert_eq!(out.typography.font, ReaderFontChoiceDto::Publisher);
        assert_eq!(out.typography.font_size_px, defaults.typography.font_size_px);
        assert_eq!(out.typography.font_weight, ReaderFontWeightDto::Weight(500));
        assert_eq!(out.typography.line_height, 2.4);
        assert_eq!(out.typography.letter_spacing_em, -0.05);
        assert_eq!(
            out.typography.word_spacing_em,
            defaults.typography.word_spacing_em
        );
        assert_eq!(out.layout.paragraph_spacing_em, 0.0);
        assert_eq!(
            out.layout.paragraph_indent_em,
            defaults.layout.paragraph_indent_em
        );
        assert_eq!(out.layout.page_margin_inline_px, 120.0);
        assert_eq!(out.layout.page_margin_block_px, 8.0);
        assert_eq!(out.layout.max_inline_size_px, 480.0);
        assert_eq!(out.layout.column_gap_percent, 12.0);
    }

    #[test]
    fn book_override_normalizes_hash_cleans_empty_patch_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let store = ReaderConfigStore::load_or_default(dir.path(), false);
        assert!(!store.settings_path().exists());

        let hash_upper = blake3::hash(b"book-a").to_hex().to_string().to_uppercase();
        let hash_lower = hash_upper.to_lowercase();

        assert_eq!(
            store
                .set_book_override("not-a-hash", ReaderProfilePatchDto::default(), font_url(), true)
                .unwrap_err(),
            ErrorCode::BookFileInvalidHashFormat
        );

        let snap = store
            .set_book_override(
                &hash_upper,
                ReaderProfilePatchDto {
                    typography: Some(ReaderTypographyPatchDto {
                        font_size_px: Some(120.0),
                        line_height: Some(f64::NAN),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                font_url(),
                true,
            )
            .unwrap();
        assert!(store.settings_path().exists());
        let saved_patch = snap.book_overrides.get(&hash_lower).expect("normalized key");
        assert_eq!(
            saved_patch.typography.as_ref().unwrap().font_size_px,
            Some(48.0)
        );
        assert_eq!(saved_patch.typography.as_ref().unwrap().line_height, None);

        // Reload from disk verifies round-trip persistence.
        let reloaded = ReaderConfigStore::load_or_default(dir.path(), false);
        let reloaded_snap = reloaded.snapshot(font_url(), true);
        assert_eq!(reloaded_snap.book_overrides, snap.book_overrides);

        // Empty patch removes the override key cleanly.
        let cleared = reloaded
            .set_book_override(&hash_lower, ReaderProfilePatchDto::default(), font_url(), true)
            .unwrap();
        assert!(cleared.book_overrides.is_empty());
    }

    #[test]
    fn corrupt_reader_settings_json_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(READER_SETTINGS_FILE_NAME), b"{not valid json").unwrap();

        let store = ReaderConfigStore::load_or_default(dir.path(), true);
        let snap = store.snapshot(font_url(), false);
        assert_eq!(snap.global, ReaderProfileDto::default_for_platform(true));
        assert!(snap.book_overrides.is_empty());
    }

    #[test]
    fn union_types_serialize_matching_ts_contracts() {
        assert_eq!(
            serde_json::to_string(&ReaderFontWeightDto::Auto).unwrap(),
            r#""auto""#
        );
        assert_eq!(
            serde_json::to_string(&ReaderFontWeightDto::Weight(600)).unwrap(),
            "600"
        );
        assert_eq!(
            serde_json::to_string(&ReaderColumnCountDto::One).unwrap(),
            "1"
        );
        assert_eq!(
            serde_json::to_string(&ReaderColumnCountDto::Auto).unwrap(),
            r#""auto""#
        );
        assert_eq!(
            serde_json::to_string(&ReaderParagraphIndentDto::Em(2.0)).unwrap(),
            "2.0"
        );
        assert_eq!(
            serde_json::to_string(&ReaderFontChoiceDto::Custom {
                font_id: "abcd".to_owned()
            })
            .unwrap(),
            r#"{"kind":"custom","fontId":"abcd"}"#
        );

        // ReaderColumnCountDto deserializes integer-valued floats (e.g. 1.0, 2.0) and rejects non-integers.
        assert_eq!(
            serde_json::from_str::<ReaderColumnCountDto>("1.0").unwrap(),
            ReaderColumnCountDto::One
        );
        assert_eq!(
            serde_json::from_str::<ReaderColumnCountDto>("2.0").unwrap(),
            ReaderColumnCountDto::Two
        );
        assert_eq!(
            serde_json::from_str::<ReaderColumnCountDto>(r#""auto""#).unwrap(),
            ReaderColumnCountDto::Auto
        );
        assert!(serde_json::from_str::<ReaderColumnCountDto>("1.5").is_err());
        assert!(serde_json::from_str::<ReaderColumnCountDto>("3.0").is_err());
    }
}

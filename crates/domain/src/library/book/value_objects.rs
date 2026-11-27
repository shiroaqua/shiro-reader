use std::{collections::HashSet, fmt};

use bitflags::bitflags;
use derive_more::{AsRef, Display, From, Into};
use jiff::{Timestamp, civil::Date};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::library::book::errors::BookDomainError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, From, AsRef, Into, Serialize, Deserialize)]
pub struct BookId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookTitle(String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Into)]
pub struct BookHash(blake3::Hash);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookStatus {
    Active,
    Deleted { deleted_at: Timestamp },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
pub enum BookFileType {
    PDF,
    EPUB,
}


impl BookHash {
    pub const fn new(hash: blake3::Hash) -> Self {
        Self(hash)
    }

    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(blake3::Hash::from_bytes(bytes))
    }

    pub const fn as_hash(&self) -> &blake3::Hash {
        &self.0
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        self.0.as_bytes()
    }

    pub const fn into_inner(self) -> blake3::Hash {
        self.0
    }
}

impl From<blake3::Hash> for BookHash {
    fn from(hash: blake3::Hash) -> Self {
        Self(hash)
    }
}

impl BookId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    pub const fn as_uuid(&self) -> Uuid {
        self.0
    }

    pub const fn into_inner(self) -> Uuid {
        self.0
    }

    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::InvalidId);
        }

        let parsed = Uuid::parse_str(raw).map_err(|_| BookDomainError::InvalidId)?;
        Ok(Self(parsed))
    }
}

impl Default for BookId {
    fn default() -> Self {
        Self::new()
    }
}

impl BookTitle {
    pub fn parse(value: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() {
            return Err(BookDomainError::MissingTitle);
        }

        if raw.trim() != raw {
            return Err(BookDomainError::InvalidTitleFormat);
        }

        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

pub const MAX_BOOK_AUTHORS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookSubtitle(String);

impl BookSubtitle {
    pub fn parse(value: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty()
            || raw.trim() != raw
            || raw.chars().any(char::is_control)
            || raw.chars().count() > 256
        {
            return Err(BookDomainError::InvalidSubtitle);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookAuthor(String);

impl BookAuthor {
    pub fn parse(value: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty()
            || raw.trim() != raw
            || raw.chars().any(char::is_control)
            || raw.chars().count() > 256
        {
            return Err(BookDomainError::InvalidAuthor);
        }
        Ok(Self(value.into()))
    }

    pub fn parse_list(values: Vec<String>) -> Result<Vec<Self>, BookDomainError> {
        if values.is_empty() || values.len() > MAX_BOOK_AUTHORS {
            return Err(BookDomainError::InvalidAuthor);
        }
        let mut seen = HashSet::with_capacity(values.len());
        let mut authors = Vec::with_capacity(values.len());
        for raw in values {
            let author = Self::parse(raw)?;
            if !seen.insert(author.clone()) {
                return Err(BookDomainError::InvalidAuthor);
            }
            authors.push(author);
        }
        Ok(authors)
    }

    pub fn validate_list(authors: &[Self]) -> Result<(), BookDomainError> {
        if authors.is_empty() || authors.len() > MAX_BOOK_AUTHORS {
            return Err(BookDomainError::InvalidAuthor);
        }
        let mut seen = HashSet::with_capacity(authors.len());
        for author in authors {
            if !seen.insert(author) {
                return Err(BookDomainError::InvalidAuthor);
            }
        }
        Ok(())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookPublisher(String);

impl BookPublisher {
    pub fn parse(value: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty()
            || raw.trim() != raw
            || raw.chars().any(char::is_control)
            || raw.chars().count() > 256
        {
            return Err(BookDomainError::InvalidPublisher);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookDescription(String);

impl BookDescription {
    pub fn parse(value: impl AsRef<str> + Into<String>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty()
            || raw.trim() != raw
            || raw
                .chars()
                .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
            || raw.chars().count() > 16_384
        {
            return Err(BookDomainError::InvalidDescription);
        }
        Ok(Self(value.into()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BookPublishedDate {
    Year(i16),
    YearMonth { year: i16, month: i8 },
    Date(Date),
}

impl BookPublishedDate {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() || raw.trim() != raw || !raw.is_ascii() {
            return Err(BookDomainError::InvalidPublishedDate);
        }

        let bytes = raw.as_bytes();
        match bytes.len() {
            4 => {
                let year = parse_ascii_int::<i16>(raw, 1..=9999)
                    .ok_or(BookDomainError::InvalidPublishedDate)?;
                Date::new(year, 1, 1).map_err(|_| BookDomainError::InvalidPublishedDate)?;
                Ok(Self::Year(year))
            }
            7 if bytes[4] == b'-' => {
                let year = parse_ascii_int::<i16>(&raw[0..4], 1..=9999)
                    .ok_or(BookDomainError::InvalidPublishedDate)?;
                let month = parse_ascii_int::<i8>(&raw[5..7], 1..=12)
                    .ok_or(BookDomainError::InvalidPublishedDate)?;
                Date::new(year, month, 1).map_err(|_| BookDomainError::InvalidPublishedDate)?;
                Ok(Self::YearMonth { year, month })
            }
            10 if bytes[4] == b'-' && bytes[7] == b'-' => {
                let year = parse_ascii_int::<i16>(&raw[0..4], 1..=9999)
                    .ok_or(BookDomainError::InvalidPublishedDate)?;
                let month = parse_ascii_int::<i8>(&raw[5..7], 1..=12)
                    .ok_or(BookDomainError::InvalidPublishedDate)?;
                let day = parse_ascii_int::<i8>(&raw[8..10], 1..=31)
                    .ok_or(BookDomainError::InvalidPublishedDate)?;
                let date =
                    Date::new(year, month, day).map_err(|_| BookDomainError::InvalidPublishedDate)?;
                Ok(Self::Date(date))
            }
            _ => Err(BookDomainError::InvalidPublishedDate),
        }
    }

    pub fn year(&self) -> i16 {
        match *self {
            Self::Year(year) | Self::YearMonth { year, .. } => year,
            Self::Date(date) => date.year(),
        }
    }

    pub fn month(&self) -> Option<i8> {
        match *self {
            Self::Year(_) => None,
            Self::YearMonth { month, .. } => Some(month),
            Self::Date(date) => Some(date.month()),
        }
    }

    pub fn day(&self) -> Option<i8> {
        match *self {
            Self::Year(_) | Self::YearMonth { .. } => None,
            Self::Date(date) => Some(date.day()),
        }
    }
}

impl fmt::Display for BookPublishedDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Year(year) => write!(f, "{year:04}"),
            Self::YearMonth { year, month } => write!(f, "{year:04}-{month:02}"),
            Self::Date(date) => write!(
                f,
                "{:04}-{:02}-{:02}",
                date.year(),
                date.month(),
                date.day()
            ),
        }
    }
}

impl From<BookPublishedDate> for String {
    fn from(value: BookPublishedDate) -> Self {
        value.to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, AsRef, Into)]
pub struct BookLanguage(String);

impl BookLanguage {
    /// Validates and canonicalizes a BCP 47 (RFC 5646) language tag using Section 2.1.1 casing.
    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty() || raw.trim() != raw || !raw.is_ascii() || raw.len() > 64 {
            return Err(BookDomainError::InvalidLanguage);
        }

        let parts: Vec<&str> = raw.split('-').collect();
        if parts.iter().any(|part| part.is_empty()) {
            return Err(BookDomainError::InvalidLanguage);
        }

        let primary = parts[0];
        if !(2..=3).contains(&primary.len()) || !primary.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err(BookDomainError::InvalidLanguage);
        }

        let mut canonical = Vec::with_capacity(parts.len());
        canonical.push(primary.to_ascii_lowercase());
        let mut idx = 1usize;

        // Optional extlang: up to 3 subtags of exactly 3 ASCII letters.
        let mut extlang_count = 0usize;
        while idx < parts.len()
            && extlang_count < 3
            && parts[idx].len() == 3
            && parts[idx].bytes().all(|b| b.is_ascii_alphabetic())
        {
            canonical.push(parts[idx].to_ascii_lowercase());
            idx += 1;
            extlang_count += 1;
        }

        // Optional script: 4 ASCII letters -> Title Case.
        if idx < parts.len()
            && parts[idx].len() == 4
            && parts[idx].bytes().all(|b| b.is_ascii_alphabetic())
        {
            canonical.push(title_case_ascii(parts[idx]));
            idx += 1;
        }

        // Optional region: 2 ASCII letters (UPPERCASE) or 3 ASCII digits.
        if idx < parts.len() && is_bcp47_region(parts[idx]) {
            canonical.push(parts[idx].to_ascii_uppercase());
            idx += 1;
        }

        // Optional variants: (5..=8 alnum) or (digit + 3 alnum), no duplicates.
        let mut seen_variants = Vec::new();
        while idx < parts.len() && is_bcp47_variant(parts[idx]) {
            let lowered = parts[idx].to_ascii_lowercase();
            if seen_variants.contains(&lowered) {
                return Err(BookDomainError::InvalidLanguage);
            }
            seen_variants.push(lowered.clone());
            canonical.push(lowered);
            idx += 1;
        }

        // Optional extensions: singleton (1 alnum != 'x') + 1..=N (2..=8 alnum), no duplicate singletons.
        let mut seen_singletons = Vec::new();
        while idx < parts.len() && is_bcp47_extension_singleton(parts[idx]) {
            let singleton = parts[idx].to_ascii_lowercase();
            if seen_singletons.contains(&singleton) {
                return Err(BookDomainError::InvalidLanguage);
            }
            seen_singletons.push(singleton.clone());
            canonical.push(singleton);
            idx += 1;

            let mut ext_parts = 0usize;
            while idx < parts.len()
                && (2..=8).contains(&parts[idx].len())
                && parts[idx].bytes().all(|b| b.is_ascii_alphanumeric())
            {
                canonical.push(parts[idx].to_ascii_lowercase());
                idx += 1;
                ext_parts += 1;
            }
            if ext_parts == 0 {
                return Err(BookDomainError::InvalidLanguage);
            }
        }

        // Optional private use: 'x' + 1..=N (1..=8 alnum).
        if idx < parts.len() && parts[idx].eq_ignore_ascii_case("x") {
            canonical.push("x".to_owned());
            idx += 1;
            let mut priv_parts = 0usize;
            while idx < parts.len()
                && (1..=8).contains(&parts[idx].len())
                && parts[idx].bytes().all(|b| b.is_ascii_alphanumeric())
            {
                canonical.push(parts[idx].to_ascii_lowercase());
                idx += 1;
                priv_parts += 1;
            }
            if priv_parts == 0 {
                return Err(BookDomainError::InvalidLanguage);
            }
        }

        if idx != parts.len() {
            return Err(BookDomainError::InvalidLanguage);
        }

        Ok(Self(canonical.join("-")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display)]
pub enum BookIdentifierScheme {
    Isbn13,
    Isbn10,
    Issn,
    Issn13,
    Ismn13,
    Ismn10,
    Doi,
    Istc,
    Isan,
    UrnNbn,
    Cn,
    Csbn,
    Cip,
    Lccn,
    Jpnb,
    JpMagazine,
    Gpn,
    Ecn,
    Sbn,
    Bnb,
    Dnb,
    Bnf,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BookIdentifier {
    scheme: BookIdentifierScheme,
    canonical: String,
}

impl BookIdentifier {
    pub fn parse(value: impl AsRef<str>) -> Result<Self, BookDomainError> {
        let raw = value.as_ref();
        if raw.is_empty()
            || raw.trim() != raw
            || raw.chars().any(char::is_control)
            || raw.len() > 128
        {
            return Err(BookDomainError::InvalidIdentifier);
        }

        if let Some(rest) = strip_prefix_ci(raw, "urn:nbn:") {
            return parse_urn_nbn(rest);
        }
        if let Some(rest) = strip_prefix_ci(raw, "urn:isbn:") {
            return parse_isbn_any(rest.trim_start_matches(' '));
        }
        if let Some(rest) = strip_prefix_ci(raw, "urn:issn:") {
            return parse_issn_any(rest.trim_start_matches(' '));
        }
        if let Some(rest) = strip_prefix_ci(raw, "urn:ismn:") {
            return parse_ismn_any(rest.trim_start_matches(' '));
        }

        if let Some(rest) = strip_scheme_prefix(raw, &["isbn-13", "isbn13"]) {
            return parse_isbn13(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["isbn-10", "isbn10"]) {
            return parse_isbn10(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["isbn"]) {
            return parse_isbn_any(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["issn-13"]) {
            return parse_issn13(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["issn-l", "issn"]) {
            return parse_issn_any(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["ismn-13"]) {
            return parse_ismn13(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["ismn-10"]) {
            return parse_ismn10(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["ismn"]) {
            return parse_ismn_any(rest);
        }
        if let Some(rest) = strip_doi_prefix(raw) {
            return parse_doi(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["istc"]) {
            return parse_istc(rest, false);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["isan"]) {
            return parse_isan(rest, false);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["cn"]) {
            return parse_cn(rest, true);
        }
        if let Some(rest) = strip_csbn_prefix(raw) {
            return parse_csbn(rest, true);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["cip"]) {
            return parse_cip(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["lccn"]) {
            return parse_lccn(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["jpnb", "jp"]) {
            return parse_jpnb(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["jpmag"]) {
            return parse_jpmag(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["gpn"]) {
            return parse_gpn(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["ecn"]) {
            return parse_ecn(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["sbn"]) {
            return parse_sbn(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["bnb"]) {
            return parse_bnb(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["dnb"]) {
            return parse_dnb(rest);
        }
        if let Some(rest) = strip_scheme_prefix(raw, &["bnf"]) {
            return parse_bnf(rest);
        }

        // Unambiguous bare identifiers.
        if raw.starts_with("10.") {
            return parse_doi(raw);
        }
        if let Some(rest) = strip_prefix_ci(raw, "cn")
            && rest.as_bytes().first().is_some_and(u8::is_ascii_digit)
        {
            return parse_cn(rest, false);
        }
        if let Some(rest) = strip_prefix_ci(raw, "jp")
            && (8..=10).contains(&rest.len())
            && rest.bytes().all(|b| b.is_ascii_digit())
        {
            return parse_jpnb(rest);
        }
        if strip_prefix_ci(raw, "ecn-").is_some() {
            return parse_ecn(raw);
        }
        if raw.len() == 9 && strip_prefix_ci(raw, "gb").is_some() {
            return parse_bnb(raw);
        }
        if strip_prefix_ci(raw, "ark:/12148/").is_some() {
            return parse_bnf(raw);
        }
        if raw.starts_with(['M', 'm']) {
            return parse_ismn10(raw);
        }
        if raw.contains(['·', '•']) {
            return parse_csbn(raw, false);
        }
        if is_bare_issn_shape(raw) {
            return parse_issn_strict(raw);
        }
        if is_bare_istc_shape(raw) {
            return parse_istc(raw, true);
        }
        if is_bare_isan_shape(raw) {
            return parse_isan(raw, true);
        }

        if let Some(clean) = strip_clean_separators(raw, &['-', ' ']) {
            if clean.len() == 13 && clean.bytes().all(|b| b.is_ascii_digit()) {
                if clean.starts_with("9790") {
                    return parse_ismn13(&clean);
                }
                if clean.starts_with("977") {
                    return parse_issn13(&clean);
                }
                if clean.starts_with("4910") {
                    return parse_jpmag_13(&clean);
                }
                if clean.starts_with("978") || clean.starts_with("979") {
                    return parse_isbn13(&clean);
                }
            }
            if clean.len() == 10 {
                return parse_isbn10(&clean);
            }
        }

        Err(BookDomainError::InvalidIdentifier)
    }

    pub const fn scheme(&self) -> BookIdentifierScheme {
        self.scheme
    }

    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    pub fn into_inner(self) -> String {
        self.canonical
    }
}

impl fmt::Display for BookIdentifier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical)
    }
}

impl AsRef<str> for BookIdentifier {
    fn as_ref(&self) -> &str {
        &self.canonical
    }
}

impl From<BookIdentifier> for String {
    fn from(value: BookIdentifier) -> Self {
        value.canonical
    }
}

fn parse_ascii_int<T>(raw: &str, range: std::ops::RangeInclusive<T>) -> Option<T>
where
    T: std::str::FromStr + PartialOrd,
{
    if !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let parsed = raw.parse::<T>().ok()?;
    range.contains(&parsed).then_some(parsed)
}

fn title_case_ascii(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for (idx, b) in raw.bytes().enumerate() {
        if idx == 0 {
            out.push(char::from(b.to_ascii_uppercase()));
        } else {
            out.push(char::from(b.to_ascii_lowercase()));
        }
    }
    out
}

fn is_bcp47_region(part: &str) -> bool {
    (part.len() == 2 && part.bytes().all(|b| b.is_ascii_alphabetic()))
        || (part.len() == 3 && part.bytes().all(|b| b.is_ascii_digit()))
}

fn is_bcp47_variant(part: &str) -> bool {
    let bytes = part.as_bytes();
    ((5..=8).contains(&bytes.len()) && bytes.iter().all(u8::is_ascii_alphanumeric))
        || (bytes.len() == 4
            && bytes[0].is_ascii_digit()
            && bytes[1..].iter().all(u8::is_ascii_alphanumeric))
}

fn is_bcp47_extension_singleton(part: &str) -> bool {
    part.len() == 1
        && part.as_bytes()[0].is_ascii_alphanumeric()
        && !part.eq_ignore_ascii_case("x")
}

fn strip_prefix_ci<'a>(input: &'a str, prefix: &str) -> Option<&'a str> {
    if input.len() < prefix.len() || !input.is_char_boundary(prefix.len()) {
        return None;
    }
    let (head, tail) = input.split_at(prefix.len());
    head.eq_ignore_ascii_case(prefix).then_some(tail)
}

fn strip_scheme_prefix<'a>(input: &'a str, names: &[&str]) -> Option<&'a str> {
    for name in names {
        if let Some(rest) = strip_prefix_ci(input, name) {
            if let Some(after_colon) = rest.strip_prefix(':') {
                let body = after_colon.trim_start_matches(' ');
                if !body.is_empty() {
                    return Some(body);
                }
                return Some("");
            }
            if let Some(after_space) = rest.strip_prefix(' ') {
                let body = after_space.trim_start_matches(' ');
                return Some(body);
            }
        }
    }
    None
}

fn strip_doi_prefix(input: &str) -> Option<&str> {
    if let Some(rest) = strip_scheme_prefix(input, &["doi"]) {
        return Some(rest);
    }
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
    ] {
        if let Some(rest) = strip_prefix_ci(input, prefix) {
            return Some(rest);
        }
    }
    None
}

fn strip_csbn_prefix(input: &str) -> Option<&str> {
    if let Some(rest) = strip_scheme_prefix(input, &["csbn"]) {
        return Some(rest);
    }
    for prefix in ["统一书号:", "统一书号："] {
        if let Some(rest) = input.strip_prefix(prefix) {
            return Some(rest.trim_start_matches(' '));
        }
    }
    if let Some(rest) = input.strip_prefix("统一书号 ") {
        return Some(rest.trim_start_matches(' '));
    }
    None
}

fn strip_clean_separators(raw: &str, allowed_seps: &[char]) -> Option<String> {
    if raw.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(raw.len());
    let mut prev_sep = true;
    for ch in raw.chars() {
        if allowed_seps.contains(&ch) {
            if prev_sep {
                return None;
            }
            prev_sep = true;
        } else {
            out.push(ch);
            prev_sep = false;
        }
    }
    if prev_sep || out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn is_ean13_valid(digits: &str) -> bool {
    let bytes = digits.as_bytes();
    if bytes.len() != 13 || !bytes.iter().all(u8::is_ascii_digit) {
        return false;
    }
    let sum: u32 = bytes[..12]
        .iter()
        .enumerate()
        .map(|(i, &b)| u32::from(b - b'0') * if i % 2 == 0 { 1 } else { 3 })
        .sum();
    let check = (10 - (sum % 10)) % 10;
    check == u32::from(bytes[12] - b'0')
}

fn is_isbn10_mod11_valid(chars: &[u8]) -> bool {
    if chars.len() != 10 {
        return false;
    }
    let mut sum: u32 = 0;
    for (idx, &b) in chars.iter().enumerate() {
        let val = if idx < 9 {
            if !b.is_ascii_digit() {
                return false;
            }
            u32::from(b - b'0')
        } else if b.is_ascii_digit() {
            u32::from(b - b'0')
        } else if b == b'X' || b == b'x' {
            10
        } else {
            return false;
        };
        sum += val * (10 - idx as u32);
    }
    sum % 11 == 0
}

fn is_issn_mod11_valid(chars: &[u8]) -> bool {
    if chars.len() != 8 {
        return false;
    }
    let mut sum: u32 = 0;
    for (idx, &b) in chars.iter().enumerate() {
        let val = if idx < 7 {
            if !b.is_ascii_digit() {
                return false;
            }
            u32::from(b - b'0')
        } else if b.is_ascii_digit() {
            u32::from(b - b'0')
        } else if b == b'X' || b == b'x' {
            10
        } else {
            return false;
        };
        sum += val * (8 - idx as u32);
    }
    sum % 11 == 0
}

fn parse_isbn13(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    if (clean.starts_with("978") || (clean.starts_with("979") && !clean.starts_with("9790")))
        && is_ean13_valid(&clean)
    {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Isbn13,
            canonical: clean,
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_isbn10(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    if is_isbn10_mod11_valid(clean.as_bytes()) {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Isbn10,
            canonical: clean.to_ascii_uppercase(),
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_isbn_any(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    parse_isbn13(raw).or_else(|_| parse_isbn10(raw))
}

fn is_bare_issn_shape(raw: &str) -> bool {
    let b = raw.as_bytes();
    b.len() == 9 && b[4] == b'-'
}

fn parse_issn_strict(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-']).ok_or(BookDomainError::InvalidIdentifier)?;
    if is_issn_mod11_valid(clean.as_bytes()) {
        let upper = clean.to_ascii_uppercase();
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Issn,
            canonical: format!("{}-{}", &upper[..4], &upper[4..]),
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_issn13(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    if clean.starts_with("977") && is_ean13_valid(&clean) {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Issn13,
            canonical: clean,
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_issn_any(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    if clean.len() == 8 && is_issn_mod11_valid(clean.as_bytes()) {
        let upper = clean.to_ascii_uppercase();
        return Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Issn,
            canonical: format!("{}-{}", &upper[..4], &upper[4..]),
        });
    }
    parse_issn13(raw)
}

fn parse_ismn13(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    if clean.starts_with("9790") && is_ean13_valid(&clean) {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Ismn13,
            canonical: clean,
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_ismn10(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    let bytes = clean.as_bytes();
    if bytes.len() != 10
        || !matches!(bytes[0], b'M' | b'm')
        || !bytes[1..].iter().all(u8::is_ascii_digit)
    {
        return Err(BookDomainError::InvalidIdentifier);
    }
    // In ISMN-10, 'M' carries value 3 at odd position (weight 3 -> 9).
    let sum: u32 = 9 + bytes[1..]
        .iter()
        .enumerate()
        .map(|(i, &b)| u32::from(b - b'0') * if i % 2 == 0 { 1 } else { 3 })
        .sum::<u32>();
    if sum % 10 != 0 {
        return Err(BookDomainError::InvalidIdentifier);
    }
    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Ismn10,
        canonical: format!("M{}", &clean[1..]),
    })
}

fn parse_ismn_any(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    parse_ismn13(raw).or_else(|_| parse_ismn10(raw))
}

fn parse_doi(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let after_10 = raw
        .strip_prefix("10.")
        .ok_or(BookDomainError::InvalidIdentifier)?;
    let (registrant, suffix) = after_10
        .split_once('/')
        .ok_or(BookDomainError::InvalidIdentifier)?;
    if registrant.is_empty() || suffix.is_empty() {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let mut segs = registrant.split('.');
    let first = segs.next().ok_or(BookDomainError::InvalidIdentifier)?;
    if !(4..=9).contains(&first.len()) || !first.bytes().all(|b| b.is_ascii_digit()) {
        return Err(BookDomainError::InvalidIdentifier);
    }
    for seg in segs {
        if seg.is_empty() || !seg.bytes().all(|b| b.is_ascii_digit()) {
            return Err(BookDomainError::InvalidIdentifier);
        }
    }
    if suffix
        .chars()
        .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(BookDomainError::InvalidIdentifier);
    }
    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Doi,
        canonical: format!("DOI:{raw}"),
    })
}

fn is_bare_istc_shape(raw: &str) -> bool {
    let b = raw.as_bytes();
    b.len() == 19 && b[3] == b'-' && b[8] == b'-' && b[17] == b'-'
}

fn parse_istc(raw: &str, require_hyphenated: bool) -> Result<BookIdentifier, BookDomainError> {
    if require_hyphenated && !is_bare_istc_shape(raw) {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    let bytes = clean.as_bytes();
    if bytes.len() != 16 || !bytes.iter().all(u8::is_ascii_hexdigit) {
        return Err(BookDomainError::InvalidIdentifier);
    }
    parse_ascii_int::<u16>(&clean[3..7], 1000..=2999).ok_or(BookDomainError::InvalidIdentifier)?;

    let mut acc: u8 = 0;
    for &b in &bytes[..15] {
        let d = hex_digit_val(b).ok_or(BookDomainError::InvalidIdentifier)?;
        acc = ((acc + d) * 3) % 16;
    }
    let expected = hex_digit_val(bytes[15]).ok_or(BookDomainError::InvalidIdentifier)?;
    if acc != expected {
        return Err(BookDomainError::InvalidIdentifier);
    }

    let upper = clean.to_ascii_uppercase();
    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Istc,
        canonical: format!(
            "ISTC:{}-{}-{}-{}",
            &upper[0..3],
            &upper[3..7],
            &upper[7..15],
            &upper[15..16]
        ),
    })
}

fn is_bare_isan_shape(raw: &str) -> bool {
    let b = raw.as_bytes();
    match b.len() {
        21 => b[4] == b'-' && b[9] == b'-' && b[14] == b'-' && b[19] == b'-',
        33 => {
            b[4] == b'-'
                && b[9] == b'-'
                && b[14] == b'-'
                && b[19] == b'-'
                && b[21] == b'-'
                && b[26] == b'-'
                && b[31] == b'-'
        }
        _ => false,
    }
}

fn parse_isan(raw: &str, require_hyphenated: bool) -> Result<BookIdentifier, BookDomainError> {
    if require_hyphenated && !is_bare_isan_shape(raw) {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    let upper = clean.to_ascii_uppercase();
    let bytes = upper.as_bytes();

    match bytes.len() {
        17 => {
            if !bytes[..16].iter().all(u8::is_ascii_hexdigit)
                || !iso7064_mod37_36_matches(&bytes[..16], bytes[16])
            {
                return Err(BookDomainError::InvalidIdentifier);
            }
            Ok(BookIdentifier {
                scheme: BookIdentifierScheme::Isan,
                canonical: format!(
                    "ISAN:{}-{}-{}-{}-{}",
                    &upper[0..4],
                    &upper[4..8],
                    &upper[8..12],
                    &upper[12..16],
                    &upper[16..17]
                ),
            })
        }
        26 => {
            if !bytes[..16].iter().all(u8::is_ascii_hexdigit)
                || !iso7064_mod37_36_matches(&bytes[..16], bytes[16])
                || !bytes[17..25].iter().all(u8::is_ascii_hexdigit)
            {
                return Err(BookDomainError::InvalidIdentifier);
            }
            let mut combined = [0u8; 24];
            combined[..16].copy_from_slice(&bytes[..16]);
            combined[16..24].copy_from_slice(&bytes[17..25]);
            if !iso7064_mod37_36_matches(&combined, bytes[25]) {
                return Err(BookDomainError::InvalidIdentifier);
            }
            Ok(BookIdentifier {
                scheme: BookIdentifierScheme::Isan,
                canonical: format!(
                    "ISAN:{}-{}-{}-{}-{}-{}-{}-{}",
                    &upper[0..4],
                    &upper[4..8],
                    &upper[8..12],
                    &upper[12..16],
                    &upper[16..17],
                    &upper[17..21],
                    &upper[21..25],
                    &upper[25..26]
                ),
            })
        }
        _ => Err(BookDomainError::InvalidIdentifier),
    }
}

fn hex_digit_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn mod37_36_val(b: u8) -> Option<u32> {
    match b {
        b'0'..=b'9' => Some(u32::from(b - b'0')),
        b'A'..=b'Z' => Some(u32::from(b - b'A') + 10),
        b'a'..=b'z' => Some(u32::from(b - b'a') + 10),
        _ => None,
    }
}

fn iso7064_mod37_36_matches(data: &[u8], check: u8) -> bool {
    let Some(expected) = mod37_36_val(check) else {
        return false;
    };
    let mut p: u32 = 36;
    for &b in data {
        let Some(v) = mod37_36_val(b) else {
            return false;
        };
        let mut s = (p + v) % 36;
        if s == 0 {
            s = 36;
        }
        p = (s * 2) % 37;
    }
    ((37 - p) % 36) == expected
}

fn parse_urn_nbn(rest: &str) -> Result<BookIdentifier, BookDomainError> {
    let (ns, specific) = rest
        .split_once(':')
        .ok_or(BookDomainError::InvalidIdentifier)?;
    if ns.is_empty() || specific.is_empty() {
        return Err(BookDomainError::InvalidIdentifier);
    }
    if let Some((cc, subns)) = ns.split_once('-') {
        if cc.len() != 2
            || !cc.bytes().all(|b| b.is_ascii_alphabetic())
            || subns.is_empty()
            || subns.starts_with('-')
            || subns.ends_with('-')
            || subns.contains("--")
            || !subns
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err(BookDomainError::InvalidIdentifier);
        }
    } else if ns.len() != 2 || !ns.bytes().all(|b| b.is_ascii_alphabetic()) {
        return Err(BookDomainError::InvalidIdentifier);
    }

    if !specific.bytes().all(|b| {
        b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'/' | b'+' | b'-')
    }) {
        return Err(BookDomainError::InvalidIdentifier);
    }

    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::UrnNbn,
        canonical: format!("urn:nbn:{}:{specific}", ns.to_ascii_lowercase()),
    })
}

fn parse_cn(raw: &str, allow_inner_cn_prefix: bool) -> Result<BookIdentifier, BookDomainError> {
    const VALID_CN_REGIONS: &[&str] = &[
        "09", "10", "11", "12", "13", "14", "15", "21", "22", "23", "30", "31", "32", "33", "34",
        "35", "36", "37", "41", "42", "43", "44", "45", "46", "50", "51", "52", "53", "54", "61",
        "62", "63", "64", "65", "81",
    ];

    let body = if allow_inner_cn_prefix {
        if let Some(after_cn) = strip_prefix_ci(raw, "cn") {
            let trimmed = after_cn.trim_start_matches(' ');
            if trimmed.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                trimmed
            } else {
                raw
            }
        } else {
            raw
        }
    } else {
        raw
    };

    let (base, suffix) = match body.split_once('/') {
        Some((b, s)) => (b, Some(s)),
        None => (body, None),
    };

    let base_bytes = base.as_bytes();
    if base_bytes.len() != 7 || base_bytes[2] != b'-' {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let region = &base[0..2];
    let seq = &base[3..7];
    if !VALID_CN_REGIONS.contains(&region)
        || !seq.bytes().all(|b| b.is_ascii_digit())
        || seq == "0000"
    {
        return Err(BookDomainError::InvalidIdentifier);
    }

    if let Some(suf) = suffix {
        let mut normalized = String::with_capacity(suf.len());
        for ch in suf.chars() {
            let mapped = match ch {
                '（' => '(',
                '）' => ')',
                _ => ch,
            };
            if !mapped.is_ascii_alphanumeric() && !matches!(mapped, '(' | ')' | '-') {
                return Err(BookDomainError::InvalidIdentifier);
            }
            normalized.push(mapped.to_ascii_uppercase());
        }
        if normalized.is_empty() || normalized.len() > 16 {
            return Err(BookDomainError::InvalidIdentifier);
        }
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Cn,
            canonical: format!("CN:{region}-{seq}/{normalized}"),
        })
    } else {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Cn,
            canonical: format!("CN:{region}-{seq}"),
        })
    }
}

fn parse_csbn(raw: &str, allow_ascii_sep: bool) -> Result<BookIdentifier, BookDomainError> {
    let sep_pos = raw
        .find(|c| {
            c == '·'
                || c == '•'
                || (allow_ascii_sep && (c == '.' || c == '-'))
        })
        .ok_or(BookDomainError::InvalidIdentifier)?;
    let sep_len = raw[sep_pos..].chars().next().expect("separator exists").len_utf8();
    let left = &raw[..sep_pos];
    let right = &raw[sep_pos + sep_len..];

    if left.is_empty()
        || right.is_empty()
        || right.len() > 6
        || !right.bytes().all(|b| b.is_ascii_digit())
        || right.bytes().all(|b| b == b'0')
    {
        return Err(BookDomainError::InvalidIdentifier);
    }

    let left_bytes = left.as_bytes();
    let (prefix, pub_digits) = if left_bytes[0].is_ascii_alphabetic() {
        (
            Some(char::from(left_bytes[0].to_ascii_uppercase())),
            &left[1..],
        )
    } else {
        (None, left)
    };

    if !(4..=5).contains(&pub_digits.len())
        || !pub_digits.bytes().all(|b| b.is_ascii_digit())
        || pub_digits.bytes().all(|b| b == b'0')
    {
        return Err(BookDomainError::InvalidIdentifier);
    }

    let canonical = match prefix {
        Some(p) => format!("CSBN:{p}{pub_digits}\u{00B7}{right}"),
        None => format!("CSBN:{pub_digits}\u{00B7}{right}"),
    };
    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Csbn,
        canonical,
    })
}

fn parse_cip(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let (year, seq) = if (9..=12).contains(&raw.len()) && raw.bytes().all(|b| b.is_ascii_digit()) {
        (&raw[..4], &raw[4..])
    } else {
        let rest = raw
            .strip_prefix('(')
            .or_else(|| raw.strip_prefix('（'))
            .ok_or(BookDomainError::InvalidIdentifier)?;
        let close_pos = rest
            .find(['(', ')', '（', '）'])
            .ok_or(BookDomainError::InvalidIdentifier)?;
        let close_ch = rest[close_pos..].chars().next().expect("close char");
        if !matches!(close_ch, ')' | '）') {
            return Err(BookDomainError::InvalidIdentifier);
        }
        let year = &rest[..close_pos];
        let tail = &rest[close_pos + close_ch.len_utf8()..];
        let seq = if let Some(after_di) = tail.strip_prefix('第') {
            after_di
                .strip_suffix('号')
                .ok_or(BookDomainError::InvalidIdentifier)?
        } else {
            tail
        };
        (year, seq)
    };

    if year.len() != 4 || parse_ascii_int::<u16>(year, 1990..=2100).is_none() {
        return Err(BookDomainError::InvalidIdentifier);
    }
    if !(5..=8).contains(&seq.len())
        || !seq.bytes().all(|b| b.is_ascii_digit())
        || seq.bytes().all(|b| b == b'0')
    {
        return Err(BookDomainError::InvalidIdentifier);
    }

    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Cip,
        canonical: format!("CIP:{year}{seq}"),
    })
}

fn parse_lccn(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &[' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    let prefix_len = clean
        .bytes()
        .take_while(u8::is_ascii_alphabetic)
        .count();
    if prefix_len > 3 {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let prefix = clean[..prefix_len].to_ascii_lowercase();
    let rest = &clean[prefix_len..];

    let (year, serial_padded) = if let Some((y, s)) = rest.split_once('-') {
        if !(1..=6).contains(&s.len())
            || !s.bytes().all(|b| b.is_ascii_digit())
            || s.bytes().all(|b| b == b'0')
        {
            return Err(BookDomainError::InvalidIdentifier);
        }
        if y.len() == 2 && y.bytes().all(|b| b.is_ascii_digit()) {
            (y, format!("{s:0>6}"))
        } else if y.len() == 4
            && prefix_len <= 2
            && parse_ascii_int::<u16>(y, 2001..=2100).is_some()
        {
            (y, format!("{s:0>6}"))
        } else {
            return Err(BookDomainError::InvalidIdentifier);
        }
    } else if rest.len() == 8 && rest.bytes().all(|b| b.is_ascii_digit()) {
        let (y, s) = rest.split_at(2);
        if s == "000000" {
            return Err(BookDomainError::InvalidIdentifier);
        }
        (y, s.to_owned())
    } else if rest.len() == 10
        && prefix_len <= 2
        && rest.bytes().all(|b| b.is_ascii_digit())
        && parse_ascii_int::<u16>(&rest[..4], 2001..=2100).is_some()
    {
        let (y, s) = rest.split_at(4);
        if s == "000000" {
            return Err(BookDomainError::InvalidIdentifier);
        }
        (y, s.to_owned())
    } else {
        return Err(BookDomainError::InvalidIdentifier);
    };

    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Lccn,
        canonical: format!("LCCN:{prefix}{year}{serial_padded}"),
    })
}

fn parse_jpnb(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let digits = strip_prefix_ci(raw, "jp").unwrap_or(raw);
    if !(8..=10).contains(&digits.len())
        || !digits.bytes().all(|b| b.is_ascii_digit())
        || digits.bytes().all(|b| b == b'0')
    {
        return Err(BookDomainError::InvalidIdentifier);
    }
    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Jpnb,
        canonical: format!("JP:{digits}"),
    })
}

fn parse_jpmag_13(clean: &str) -> Result<BookIdentifier, BookDomainError> {
    if clean.starts_with("4910") && is_ean13_valid(clean) {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::JpMagazine,
            canonical: format!("JPMAG:{clean}"),
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_jpmag(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    if let Some(clean) = strip_clean_separators(raw, &['-', ' '])
        && clean.len() == 13
    {
        return parse_jpmag_13(&clean);
    }
    match raw.split_once('-') {
        Some((code, issue)) => {
            if code.len() == 5
                && code.bytes().all(|b| b.is_ascii_digit())
                && code != "00000"
                && issue.len() == 2
                && parse_ascii_int::<u8>(issue, 1..=99).is_some()
            {
                Ok(BookIdentifier {
                    scheme: BookIdentifierScheme::JpMagazine,
                    canonical: format!("JPMAG:{code}-{issue}"),
                })
            } else {
                Err(BookDomainError::InvalidIdentifier)
            }
        }
        None => {
            if raw.len() == 5 && raw.bytes().all(|b| b.is_ascii_digit()) && raw != "00000" {
                Ok(BookIdentifier {
                    scheme: BookIdentifierScheme::JpMagazine,
                    canonical: format!("JPMAG:{raw}"),
                })
            } else {
                Err(BookDomainError::InvalidIdentifier)
            }
        }
    }
}

fn parse_gpn(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    if raw.len() == 10
        && raw.bytes().all(|b| b.is_ascii_digit())
        && raw.bytes().any(|b| b != b'0')
    {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Gpn,
            canonical: format!("GPN:{raw}"),
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_ecn(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let body = strip_prefix_ci(raw, "ecn-").unwrap_or(raw);
    let clean = if body.len() == 20 && body.bytes().all(|b| b.is_ascii_digit()) {
        body.to_owned()
    } else {
        let parts: Vec<&str> = body.split('-').collect();
        if parts.len() != 4
            || parts[0].len() != 4
            || parts[1].len() != 4
            || parts[2].len() != 3
            || parts[3].len() != 9
            || !parts.iter().all(|p| p.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(BookDomainError::InvalidIdentifier);
        }
        parts.concat()
    };

    if parse_ascii_int::<u16>(&clean[4..8], 1990..=2100).is_none()
        || clean[11..20].bytes().all(|b| b == b'0')
    {
        return Err(BookDomainError::InvalidIdentifier);
    }

    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Ecn,
        canonical: format!(
            "ECN:{}-{}-{}-{}",
            &clean[0..4],
            &clean[4..8],
            &clean[8..11],
            &clean[11..20]
        ),
    })
}

fn parse_sbn(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let clean = strip_clean_separators(raw, &['-', ' ']).ok_or(BookDomainError::InvalidIdentifier)?;
    if clean.len() != 9 {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let prefixed = format!("0{clean}");
    if is_isbn10_mod11_valid(prefixed.as_bytes()) {
        Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Sbn,
            canonical: format!("SBN:{}", clean.to_ascii_uppercase()),
        })
    } else {
        Err(BookDomainError::InvalidIdentifier)
    }
}

fn parse_bnb(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let rest = strip_prefix_ci(raw, "gb").ok_or(BookDomainError::InvalidIdentifier)?;
    if rest.len() != 7 {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let bytes = rest.as_bytes();
    let valid = (bytes[0].is_ascii_alphabetic() || bytes[0].is_ascii_digit())
        && bytes[1..].iter().all(u8::is_ascii_digit)
        && bytes[1..].iter().any(|&b| b != b'0');
    if !valid {
        return Err(BookDomainError::InvalidIdentifier);
    }
    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Bnb,
        canonical: format!("BNB:GB{}", rest.to_ascii_uppercase()),
    })
}

fn parse_dnb(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let bytes = raw.as_bytes();
    if !(8..=10).contains(&bytes.len()) {
        return Err(BookDomainError::InvalidIdentifier);
    }
    let (body, last) = bytes.split_at(bytes.len() - 1);
    if !body.iter().all(u8::is_ascii_digit)
        || body.iter().all(|&b| b == b'0')
        || !(last[0].is_ascii_digit() || last[0] == b'X' || last[0] == b'x')
    {
        return Err(BookDomainError::InvalidIdentifier);
    }
    Ok(BookIdentifier {
        scheme: BookIdentifierScheme::Dnb,
        canonical: format!("DNB:{}", raw.to_ascii_uppercase()),
    })
}

fn parse_bnf(raw: &str) -> Result<BookIdentifier, BookDomainError> {
    let body = strip_prefix_ci(raw, "ark:/12148/").unwrap_or(raw);
    if let Some(cb_rest) = strip_prefix_ci(body, "cb") {
        let bytes = cb_rest.as_bytes();
        if bytes.len() == 9
            && bytes[..8].iter().all(u8::is_ascii_digit)
            && bytes[..8].iter().any(|&b| b != b'0')
            && bytes[8].is_ascii_alphanumeric()
        {
            return Ok(BookIdentifier {
                scheme: BookIdentifierScheme::Bnf,
                canonical: format!("BNF:cb{}", cb_rest.to_ascii_lowercase()),
            });
        }
        return Err(BookDomainError::InvalidIdentifier);
    }
    if let Some(fr_rest) = strip_prefix_ci(body, "frbnf")
        && (8..=9).contains(&fr_rest.len())
        && fr_rest.bytes().all(|b| b.is_ascii_digit())
        && fr_rest.bytes().any(|b| b != b'0')
    {
        return Ok(BookIdentifier {
            scheme: BookIdentifierScheme::Bnf,
            canonical: format!("BNF:FRBNF{fr_rest}"),
        });
    }
    Err(BookDomainError::InvalidIdentifier)
}



bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct TocEntryFlags: u16 {
        /// Entry is collapsed by default (e.g. PDF outline `/Count < 0`).
        const DEFAULT_COLLAPSED = 1 << 0;
        /// Entry points to an external link/URI rather than an in-book location.
        const EXTERNAL_LINK = 1 << 1;
    }
}

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

const NUMBER_REGEX: &str = r"^-?[0-9]+([.][0-9]+)?$";
const DATE_REGEX: &str = r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$";
const ISSN_ISBN_REGEX: &str =
    r"^(?:97[89](-?[0-9]){10}|([0-9]-?){9}[0-9Xx]|[0-9]{4}-?[0-9]{3}[0-9Xx])$";
const CLC_REGEX: &str = r"^[A-Z][A-Z0-9]*([.][A-Z0-9]+)*$";
const DDC_REGEX: &str = r"^[0-9]{3}([.][0-9]+)*$";
const LCC_REGEX: &str = r"^[A-Z]{1,3}[0-9]{0,4}([.][A-Z0-9]+)*$";

const LANGUAGE_CODES: &[&str] = &[
    "zh-Hans", "zh-Hant", "en", "ja", "ab", "aa", "af", "ak", "sq", "am", "ar", "an", "hy", "as",
    "av", "ae", "ay", "az", "bm", "bn", "ba", "eu", "be", "bh", "bi", "bs", "br", "bg", "my", "ca",
    "ch", "ce", "cu", "cv", "kw", "co", "cr", "hr", "cs", "da", "dv", "nl", "dz", "eo", "et", "ee",
    "fo", "fj", "tl", "fi", "fr", "ff", "gl", "lg", "ka", "de", "el", "gn", "gu", "ht", "ha", "he",
    "hz", "hi", "ho", "hu", "is", "io", "ig", "id", "ia", "ie", "iu", "ik", "ga", "it", "jv", "kl",
    "kn", "kr", "ks", "kk", "km", "ki", "rw", "kv", "kg", "ko", "kj", "ku", "ky", "lo", "la", "lv",
    "li", "ln", "lt", "lu", "lb", "mk", "mg", "ms", "ml", "mt", "gv", "mi", "mr", "mh", "mn", "na",
    "nv", "ng", "ne", "nd", "se", "no", "nb", "nn", "ny", "oc", "or", "oj", "om", "os", "pi", "ps",
    "fa", "pl", "pt", "pa", "qu", "ro", "rm", "rn", "ru", "sm", "sg", "sa", "sc", "gd", "sr", "sn",
    "ii", "sd", "si", "sk", "sl", "so", "nr", "st", "es", "su", "sw", "ss", "sv", "ty", "tg", "ta",
    "tt", "te", "th", "bo", "ti", "to", "ts", "tn", "tr", "tk", "tw", "uk", "ur", "ug", "uz", "ve",
    "vi", "vo", "wa", "cy", "fy", "wo", "xh", "yi", "yo", "za", "zu",
];

fn language_regex() -> &'static str {
    static LANGUAGE_REGEX: LazyLock<String> =
        LazyLock::new(|| format!("^(?:{})$", LANGUAGE_CODES.join("|")));
    LANGUAGE_REGEX.as_str()
}

/// Client-side tag value type mapped to/from `(allows_value, value_regex)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TagValueType {
    None,
    Number,
    Text,
    Date,
    Language,
    IssnIsbn,
    Clc,
    Ddc,
    Lcc,
}

impl TagValueType {
    /// Whether tags of this value type accept a value when bound to a book.
    pub const fn allows_value(self) -> bool {
        !matches!(self, Self::None)
    }

    /// Returns the default preset regex for this value type, if any.
    pub fn default_regex(self) -> Option<&'static str> {
        match self {
            Self::None | Self::Text => None,
            Self::Number => Some(NUMBER_REGEX),
            Self::Date => Some(DATE_REGEX),
            Self::Language => Some(language_regex()),
            Self::IssnIsbn => Some(ISSN_ISBN_REGEX),
            Self::Clc => Some(CLC_REGEX),
            Self::Ddc => Some(DDC_REGEX),
            Self::Lcc => Some(LCC_REGEX),
        }
    }

    /// Converts this client value type to the underlying domain/API `(allows_value, value_regex)` rule.
    pub fn to_value_rule(self) -> (bool, Option<String>) {
        (
            self.allows_value(),
            self.default_regex().map(str::to_owned),
        )
    }

    /// Maps an underlying `(allows_value, value_regex)` rule to a client value type.
    ///
    /// Any unrecognized or custom regex falls back to [`Self::Text`] when `allows_value` is true.
    pub fn from_value_rule(allows_value: bool, value_regex: Option<&str>) -> Self {
        if !allows_value {
            return Self::None;
        }
        let pattern = value_regex.filter(|p| !p.is_empty());
        match pattern {
            Some(r) if r == NUMBER_REGEX => Self::Number,
            Some(r) if r == DATE_REGEX => Self::Date,
            Some(r) if r == language_regex() => Self::Language,
            Some(r) if r == ISSN_ISBN_REGEX => Self::IssnIsbn,
            Some(r) if r == CLC_REGEX => Self::Clc,
            Some(r) if r == DDC_REGEX => Self::Ddc,
            Some(r) if r == LCC_REGEX => Self::Lcc,
            _ => Self::Text,
        }
    }
}

#[cfg(test)]
mod tests {
    use shiro_domain::library::tag::value_objects::TagValueRule;

    use super::*;

    const ALL_TYPES: [TagValueType; 9] = [
        TagValueType::None,
        TagValueType::Number,
        TagValueType::Text,
        TagValueType::Date,
        TagValueType::Language,
        TagValueType::IssnIsbn,
        TagValueType::Clc,
        TagValueType::Ddc,
        TagValueType::Lcc,
    ];

    #[test]
    fn serde_round_trips_all_tag_value_types() {
        let expected_wire = [
            (TagValueType::None, "\"none\""),
            (TagValueType::Number, "\"number\""),
            (TagValueType::Text, "\"text\""),
            (TagValueType::Date, "\"date\""),
            (TagValueType::Language, "\"language\""),
            (TagValueType::IssnIsbn, "\"issn_isbn\""),
            (TagValueType::Clc, "\"clc\""),
            (TagValueType::Ddc, "\"ddc\""),
            (TagValueType::Lcc, "\"lcc\""),
        ];
        for (value_type, wire) in expected_wire {
            assert_eq!(serde_json::to_string(&value_type).unwrap(), wire);
            assert_eq!(serde_json::from_str::<TagValueType>(wire).unwrap(), value_type);
        }
    }

    #[test]
    fn round_trips_every_tag_value_type_through_value_rule() {
        for value_type in ALL_TYPES {
            let (allows_value, value_regex) = value_type.to_value_rule();
            assert_eq!(
                TagValueType::from_value_rule(allows_value, value_regex.as_deref()),
                value_type
            );
        }
    }

    #[test]
    fn unknown_empty_and_disabled_rules_map_consistently() {
        assert_eq!(
            TagValueType::from_value_rule(true, Some("^[A-Z]{2}$")),
            TagValueType::Text
        );
        assert_eq!(
            TagValueType::from_value_rule(true, Some("[0-9]{4}")),
            TagValueType::Text
        );
        assert_eq!(
            TagValueType::from_value_rule(true, Some("")),
            TagValueType::Text
        );
        assert_eq!(
            TagValueType::from_value_rule(true, None),
            TagValueType::Text
        );
        assert_eq!(
            TagValueType::from_value_rule(false, None),
            TagValueType::None
        );
        assert_eq!(
            TagValueType::from_value_rule(false, Some("^[A-Z]{2}$")),
            TagValueType::None
        );
        assert_eq!(
            TagValueType::from_value_rule(false, Some(NUMBER_REGEX)),
            TagValueType::None
        );
    }

    #[test]
    fn all_preset_rules_compile_and_validate_domain_values() {
        assert_eq!(LANGUAGE_CODES.len(), 185);

        for value_type in ALL_TYPES {
            let (allows_value, value_regex) = value_type.to_value_rule();
            let rule = TagValueRule::new(allows_value, value_regex)
                .expect("preset rule should be valid in domain");

            match value_type {
                TagValueType::None => {
                    assert!(rule.validate(None).is_ok());
                    assert!(rule.validate(Some("anything")).is_err());
                }
                TagValueType::Number => {
                    assert!(rule.validate(Some("42")).is_ok());
                    assert!(rule.validate(Some("-3.14")).is_ok());
                    assert!(rule.validate(Some("12.")).is_err());
                    assert!(rule.validate(Some("abc")).is_err());
                }
                TagValueType::Text => {
                    assert!(rule.validate(Some("Any non-empty string")).is_ok());
                    assert!(rule.validate(Some("")).is_err());
                }
                TagValueType::Date => {
                    assert!(rule.validate(Some("2026-01-15")).is_ok());
                    assert!(rule.validate(Some("2026/01/15")).is_err());
                }
                TagValueType::Language => {
                    assert!(rule.validate(Some("zh-Hans")).is_ok());
                    assert!(rule.validate(Some("en")).is_ok());
                    assert!(rule.validate(Some("zu")).is_ok());
                    assert!(rule.validate(Some("xx")).is_err());
                }
                TagValueType::IssnIsbn => {
                    assert!(rule.validate(Some("978-0-306-40615-7")).is_ok());
                    assert!(rule.validate(Some("0378-5955")).is_ok());
                    assert!(rule.validate(Some("not-an-isbn")).is_err());
                }
                TagValueType::Clc => {
                    assert!(rule.validate(Some("TP312.8")).is_ok());
                    assert!(rule.validate(Some("tp312")).is_err());
                }
                TagValueType::Ddc => {
                    assert!(rule.validate(Some("005.133")).is_ok());
                    assert!(rule.validate(Some("05.1")).is_err());
                }
                TagValueType::Lcc => {
                    assert!(rule.validate(Some("QA76.73.R8")).is_ok());
                    assert!(rule.validate(Some("1234")).is_err());
                }
            }
        }
    }
}

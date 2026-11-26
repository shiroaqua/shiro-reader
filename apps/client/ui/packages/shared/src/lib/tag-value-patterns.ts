import type { LocalTranslationKey } from "@shiro/ui-shared/lib/i18n"
import type { Tag, TagValueType } from "@shiro/ui-shared/lib/types"

/**
 * Rust owns the mapping between `value_type` and `(allows_value, value_regex)`
 * and provides concrete `value_type` + `value_regex` to TypeScript so input
 * validation stays synchronous without extra IPC calls.
 */
export type ValueType = TagValueType

/**
 * ISO 639-1 language codes with a hardcoded display name per code. Priority
 * languages come first, the rest follow their English name. Labels are written
 * in the language itself; stored values remain the codes.
 */
export const LANGUAGE_OPTIONS = [
  { code: "zh-Hans", label: "简体中文" },
  { code: "zh-Hant", label: "繁體中文" },
  { code: "en", label: "English" },
  { code: "ja", label: "日本語" },
  { code: "ab", label: "аԥсуа" },
  { code: "aa", label: "Afaraf" },
  { code: "af", label: "Afrikaans" },
  { code: "ak", label: "Akan" },
  { code: "sq", label: "shqip" },
  { code: "am", label: "አማርኛ" },
  { code: "ar", label: "العربية" },
  { code: "an", label: "aragonés" },
  { code: "hy", label: "հայերեն" },
  { code: "as", label: "অসমীয়া" },
  { code: "av", label: "авар мацӀ" },
  { code: "ae", label: "Avestan" },
  { code: "ay", label: "Aymar aru" },
  { code: "az", label: "azərbaycan" },
  { code: "bm", label: "bamanankan" },
  { code: "bn", label: "বাংলা" },
  { code: "ba", label: "башҡорт теле" },
  { code: "eu", label: "euskara" },
  { code: "be", label: "беларуская" },
  { code: "bh", label: "भोजपुरी" },
  { code: "bi", label: "Bislama" },
  { code: "bs", label: "bosanski" },
  { code: "br", label: "brezhoneg" },
  { code: "bg", label: "български" },
  { code: "my", label: "မြန်မာ" },
  { code: "ca", label: "català" },
  { code: "ch", label: "Chamoru" },
  { code: "ce", label: "нохчийн" },
  { code: "cu", label: "Church Slavic" },
  { code: "cv", label: "чӑваш чӗлхи" },
  { code: "kw", label: "kernewek" },
  { code: "co", label: "corsu" },
  { code: "cr", label: "Cree" },
  { code: "hr", label: "hrvatski" },
  { code: "cs", label: "čeština" },
  { code: "da", label: "dansk" },
  { code: "dv", label: "ދިވެހި" },
  { code: "nl", label: "Nederlands" },
  { code: "dz", label: "རྫོང་ཁ" },
  { code: "eo", label: "Esperanto" },
  { code: "et", label: "eesti" },
  { code: "ee", label: "Eʋegbe" },
  { code: "fo", label: "føroyskt" },
  { code: "fj", label: "vosa Vakaviti" },
  { code: "tl", label: "Filipino" },
  { code: "fi", label: "suomi" },
  { code: "fr", label: "français" },
  { code: "ff", label: "Fulfulde" },
  { code: "gl", label: "galego" },
  { code: "lg", label: "Luganda" },
  { code: "ka", label: "ქართული" },
  { code: "de", label: "Deutsch" },
  { code: "el", label: "Ελληνικά" },
  { code: "gn", label: "Avañe'ẽ" },
  { code: "gu", label: "ગુજરાતી" },
  { code: "ht", label: "Kreyòl ayisyen" },
  { code: "ha", label: "Hausa" },
  { code: "he", label: "עברית" },
  { code: "hz", label: "Otjiherero" },
  { code: "hi", label: "हिन्दी" },
  { code: "ho", label: "Hiri Motu" },
  { code: "hu", label: "magyar" },
  { code: "is", label: "íslenska" },
  { code: "io", label: "Ido" },
  { code: "ig", label: "Igbo" },
  { code: "id", label: "Bahasa Indonesia" },
  { code: "ia", label: "interlingua" },
  { code: "ie", label: "Interlingue" },
  { code: "iu", label: "ᐃᓄᒃᑎᑐᑦ" },
  { code: "ik", label: "Iñupiaq" },
  { code: "ga", label: "Gaeilge" },
  { code: "it", label: "italiano" },
  { code: "jv", label: "basa Jawa" },
  { code: "kl", label: "kalaallisut" },
  { code: "kn", label: "ಕನ್ನಡ" },
  { code: "kr", label: "Kanuri" },
  { code: "ks", label: "کٲشُر" },
  { code: "kk", label: "қазақ тілі" },
  { code: "km", label: "ខ្មែរ" },
  { code: "ki", label: "Gĩkũyũ" },
  { code: "rw", label: "Ikinyarwanda" },
  { code: "kv", label: "коми кыв" },
  { code: "kg", label: "Kikongo" },
  { code: "ko", label: "한국어" },
  { code: "kj", label: "Kuanyama" },
  { code: "ku", label: "kurdî" },
  { code: "ky", label: "кыргызча" },
  { code: "lo", label: "ລາວ" },
  { code: "la", label: "Latin" },
  { code: "lv", label: "latviešu" },
  { code: "li", label: "Limburgs" },
  { code: "ln", label: "lingála" },
  { code: "lt", label: "lietuvių" },
  { code: "lu", label: "Tshiluba" },
  { code: "lb", label: "Lëtzebuergesch" },
  { code: "mk", label: "македонски" },
  { code: "mg", label: "Malagasy" },
  { code: "ms", label: "Bahasa Melayu" },
  { code: "ml", label: "മലയാളം" },
  { code: "mt", label: "Malti" },
  { code: "gv", label: "Gaelg" },
  { code: "mi", label: "Māori" },
  { code: "mr", label: "मराठी" },
  { code: "mh", label: "Kajin M̧ajeļ" },
  { code: "mn", label: "монгол" },
  { code: "na", label: "Dorerin Naoero" },
  { code: "nv", label: "Diné bizaad" },
  { code: "ng", label: "Owambo" },
  { code: "ne", label: "नेपाली" },
  { code: "nd", label: "isiNdebele" },
  { code: "se", label: "davvisámegiella" },
  { code: "no", label: "norsk" },
  { code: "nb", label: "norsk bokmål" },
  { code: "nn", label: "norsk nynorsk" },
  { code: "ny", label: "Chichewa" },
  { code: "oc", label: "occitan" },
  { code: "or", label: "ଓଡ଼ିଆ" },
  { code: "oj", label: "Anishinaabemowin" },
  { code: "om", label: "Afaan Oromoo" },
  { code: "os", label: "ирон æвзаг" },
  { code: "pi", label: "Pāli" },
  { code: "ps", label: "پښتو" },
  { code: "fa", label: "فارسی" },
  { code: "pl", label: "polski" },
  { code: "pt", label: "português" },
  { code: "pa", label: "ਪੰਜਾਬੀ" },
  { code: "qu", label: "Runasimi" },
  { code: "ro", label: "română" },
  { code: "rm", label: "rumantsch" },
  { code: "rn", label: "Ikirundi" },
  { code: "ru", label: "русский" },
  { code: "sm", label: "gagana Sāmoa" },
  { code: "sg", label: "Sängö" },
  { code: "sa", label: "संस्कृतम्" },
  { code: "sc", label: "sardu" },
  { code: "gd", label: "Gàidhlig" },
  { code: "sr", label: "српски" },
  { code: "sn", label: "chiShona" },
  { code: "ii", label: "ꆈꌠꉙ" },
  { code: "sd", label: "سنڌي" },
  { code: "si", label: "සිංහල" },
  { code: "sk", label: "slovenčina" },
  { code: "sl", label: "slovenščina" },
  { code: "so", label: "Soomaali" },
  { code: "nr", label: "isiNdebele seSewula" },
  { code: "st", label: "Sesotho" },
  { code: "es", label: "español" },
  { code: "su", label: "Basa Sunda" },
  { code: "sw", label: "Kiswahili" },
  { code: "ss", label: "SiSwati" },
  { code: "sv", label: "svenska" },
  { code: "ty", label: "reo Tahiti" },
  { code: "tg", label: "тоҷикӣ" },
  { code: "ta", label: "தமிழ்" },
  { code: "tt", label: "татарча" },
  { code: "te", label: "తెలుగు" },
  { code: "th", label: "ไทย" },
  { code: "bo", label: "བོད་སྐད་" },
  { code: "ti", label: "ትግርኛ" },
  { code: "to", label: "lea fakatonga" },
  { code: "ts", label: "Xitsonga" },
  { code: "tn", label: "Setswana" },
  { code: "tr", label: "Türkçe" },
  { code: "tk", label: "türkmençe" },
  { code: "tw", label: "Twi" },
  { code: "uk", label: "українська" },
  { code: "ur", label: "اردو" },
  { code: "ug", label: "ئۇيغۇرچە" },
  { code: "uz", label: "oʻzbekcha" },
  { code: "ve", label: "Tshivenḓa" },
  { code: "vi", label: "Tiếng Việt" },
  { code: "vo", label: "Volapük" },
  { code: "wa", label: "walon" },
  { code: "cy", label: "Cymraeg" },
  { code: "fy", label: "Frysk" },
  { code: "wo", label: "Wolof" },
  { code: "xh", label: "isiXhosa" },
  { code: "yi", label: "ייִדיש" },
  { code: "yo", label: "Èdè Yorùbá" },
  { code: "za", label: "Vahcuengh" },
  { code: "zu", label: "isiZulu" },
] as const

type ValueRule = Pick<Tag, "value_type" | "value_regex">

/** Order of the options in the value type select. */
export const VALUE_TYPES: readonly ValueType[] = [
  "none",
  "number",
  "text",
  "date",
  "language",
  "issn_isbn",
  "clc",
  "ddc",
  "lcc",
]

const VALUE_TYPE_LABEL_KEYS: Record<ValueType, LocalTranslationKey> = {
  none: "local.tag.value_type.none",
  number: "local.tag.value_type.number",
  text: "local.tag.value_type.text",
  date: "local.tag.value_type.date",
  language: "local.tag.value_type.language",
  issn_isbn: "local.tag.value_type.issn_isbn",
  clc: "local.tag.value_type.clc",
  ddc: "local.tag.value_type.ddc",
  lcc: "local.tag.value_type.lcc",
}

export function valueTypeLabelKey(type: ValueType): LocalTranslationKey {
  return VALUE_TYPE_LABEL_KEYS[type]
}

const TAG_NAME_PATTERN = /^[\p{L}\p{N}.]+$/u

/** Matches the server's TagName::parse rule: letters, numbers, and periods only. */
export function isValidTagName(name: string): boolean {
  return TAG_NAME_PATTERN.test(name)
}

/** Real-time client check using the value_type and value_regex provided by Rust. */
export function isValueValid(rule: ValueRule, value: string): boolean {
  if (rule.value_type === "none") return value === ""
  if (value === "") return false
  if (rule.value_regex !== null && rule.value_regex !== "") {
    try {
      if (!new RegExp(`^(?:${rule.value_regex})$`).test(value)) return false
    } catch {
      // Regex features unsupported in JS RegExp fall back to Rust/server validation.
    }
  }
  if (rule.value_type === "date") return isCalendarDate(value)
  return rule.value_type !== "issn_isbn" || isValidIssnIsbn(value)
}

function isValidIssnIsbn(value: string): boolean {
  const normalized = value.replaceAll("-", "").toUpperCase()
  if (/^97[89][0-9]{10}$/.test(normalized)) {
    const sum = [...normalized.slice(0, 12)].reduce(
      (total, digit, index) =>
        total + Number(digit) * (index % 2 === 0 ? 1 : 3),
      0
    )
    return (10 - (sum % 10)) % 10 === Number(normalized[12])
  }

  if (/^[0-9]{9}[0-9X]$/.test(normalized)) {
    const sum = [...normalized].reduce(
      (total, digit, index) =>
        total + (digit === "X" ? 10 : Number(digit)) * (10 - index),
      0
    )
    return sum % 11 === 0
  }

  if (/^[0-9]{7}[0-9X]$/.test(normalized)) {
    const sum = [...normalized.slice(0, 7)].reduce(
      (total, digit, index) => total + Number(digit) * (8 - index),
      0
    )
    const checkDigit = (11 - (sum % 11)) % 11
    const actualCheckDigit = normalized[7] === "X" ? 10 : Number(normalized[7])
    return checkDigit === actualCheckDigit
  }

  return false
}

/** Rejects dates that match the pattern but do not exist, such as 2026-02-31. */
function isCalendarDate(value: string): boolean {
  const [year, month, day] = value.split("-").map(Number)
  if (year === undefined || month === undefined || day === undefined) return false
  const date = new Date(0)
  date.setUTCFullYear(year, month - 1, day)
  return (
    date.getUTCFullYear() === year &&
    date.getUTCMonth() === month - 1 &&
    date.getUTCDate() === day
  )
}

import type { Locale } from "@shiro/ui-shared/lib/i18n"
import { LANGUAGE_OPTIONS } from "@shiro/ui-shared/lib/tag-value-patterns"

export const MAX_BOOK_AUTHORS = 64

export type LanguageOption = (typeof LANGUAGE_OPTIONS)[number]

const LANGUAGE_BY_LOWER_CODE = new Map<string, LanguageOption>(
  LANGUAGE_OPTIONS.map((option) => [option.code.toLowerCase(), option])
)

const CHINESE_PRIMARY_SUBTAGS = new Set([
  "zh",
  "zho",
  "chi",
  "cmn",
  "yue",
  "wuu",
  "nan",
  "hak",
])

const TRADITIONAL_CHINESE_SUBTAGS = new Set([
  "hant",
  "cht",
  "tw",
  "hk",
  "mo",
])

const SIMPLIFIED_CHINESE_SUBTAGS = new Set(["hans", "chs", "cn", "sg"])

/** Common ISO 639-2/3 and deprecated BCP 47 codes mapped to standard `LANGUAGE_OPTIONS` codes. */
const LANGUAGE_CODE_ALIASES: Record<string, string> = {
  aar: "aa",
  abk: "ab",
  afr: "af",
  aka: "ak",
  alb: "sq",
  amh: "am",
  ara: "ar",
  arg: "an",
  arm: "hy",
  asm: "as",
  ava: "av",
  ave: "ae",
  aym: "ay",
  aze: "az",
  bak: "ba",
  bam: "bm",
  baq: "eu",
  bel: "be",
  ben: "bn",
  bih: "bh",
  bis: "bi",
  bod: "bo",
  bos: "bs",
  bre: "br",
  bul: "bg",
  bur: "my",
  cat: "ca",
  ces: "cs",
  che: "ce",
  chu: "cu",
  chv: "cv",
  cor: "kw",
  cos: "co",
  cre: "cr",
  cym: "cy",
  cze: "cs",
  dan: "da",
  deu: "de",
  div: "dv",
  dut: "nl",
  dzo: "dz",
  ell: "el",
  eng: "en",
  epo: "eo",
  est: "et",
  eus: "eu",
  ewe: "ee",
  fao: "fo",
  fas: "fa",
  fij: "fj",
  fil: "tl",
  fin: "fi",
  fra: "fr",
  fre: "fr",
  fry: "fy",
  ful: "ff",
  geo: "ka",
  ger: "de",
  gla: "gd",
  gle: "ga",
  glg: "gl",
  glv: "gv",
  gre: "el",
  grn: "gn",
  guj: "gu",
  hat: "ht",
  hau: "ha",
  heb: "he",
  her: "hz",
  hin: "hi",
  hmo: "ho",
  hrv: "hr",
  hun: "hu",
  hye: "hy",
  ibo: "ig",
  ice: "is",
  ido: "io",
  iii: "ii",
  iku: "iu",
  ile: "ie",
  in: "id",
  ina: "ia",
  ind: "id",
  ipk: "ik",
  isl: "is",
  ita: "it",
  iw: "he",
  jav: "jv",
  ji: "yi",
  jpn: "ja",
  jw: "jv",
  kal: "kl",
  kan: "kn",
  kas: "ks",
  kat: "ka",
  kau: "kr",
  kaz: "kk",
  khm: "km",
  kik: "ki",
  kin: "rw",
  kir: "ky",
  kom: "kv",
  kon: "kg",
  kor: "ko",
  kua: "kj",
  kur: "ku",
  lao: "lo",
  lat: "la",
  lav: "lv",
  lim: "li",
  lin: "ln",
  lit: "lt",
  ltz: "lb",
  lub: "lu",
  lug: "lg",
  mac: "mk",
  mah: "mh",
  mal: "ml",
  mao: "mi",
  mar: "mr",
  may: "ms",
  mkd: "mk",
  mlg: "mg",
  mlt: "mt",
  mo: "ro",
  mon: "mn",
  mri: "mi",
  msa: "ms",
  mya: "my",
  nau: "na",
  nav: "nv",
  nbl: "nr",
  nde: "nd",
  ndo: "ng",
  nep: "ne",
  nld: "nl",
  nno: "nn",
  nob: "nb",
  nor: "no",
  nya: "ny",
  oci: "oc",
  oji: "oj",
  ori: "or",
  orm: "om",
  oss: "os",
  pan: "pa",
  per: "fa",
  pli: "pi",
  pol: "pl",
  por: "pt",
  pus: "ps",
  que: "qu",
  roh: "rm",
  ron: "ro",
  rum: "ro",
  run: "rn",
  rus: "ru",
  sag: "sg",
  san: "sa",
  sin: "si",
  slk: "sk",
  slo: "sk",
  slv: "sl",
  sme: "se",
  smo: "sm",
  sna: "sn",
  snd: "sd",
  som: "so",
  sot: "st",
  spa: "es",
  sqi: "sq",
  srd: "sc",
  srp: "sr",
  ssw: "ss",
  sun: "su",
  swa: "sw",
  swe: "sv",
  tah: "ty",
  tam: "ta",
  tat: "tt",
  tel: "te",
  tgk: "tg",
  tgl: "tl",
  tha: "th",
  tib: "bo",
  tir: "ti",
  ton: "to",
  tsn: "tn",
  tso: "ts",
  tuk: "tk",
  tur: "tr",
  twi: "tw",
  uig: "ug",
  ukr: "uk",
  urd: "ur",
  uzb: "uz",
  ven: "ve",
  vie: "vi",
  vol: "vo",
  wel: "cy",
  wln: "wa",
  wol: "wo",
  xho: "xh",
  yid: "yi",
  yor: "yo",
  zha: "za",
  zul: "zu",
}

/**
 * Matches a server-provided language tag against `LANGUAGE_OPTIONS`, allowing
 * case differences, regional subtags, Chinese script/region variants, and common
 * ISO 639-2/3 or legacy BCP 47 aliases.
 */
export function matchLanguageOption(
  raw: string | null | undefined
): LanguageOption | null {
  if (!raw) return null
  const trimmed = raw.trim()
  if (trimmed === "") return null

  const normalized = trimmed.replaceAll("_", "-")
  const exact = LANGUAGE_BY_LOWER_CODE.get(normalized.toLowerCase())
  if (exact) return exact

  const subtags = normalized
    .toLowerCase()
    .split("-")
    .filter((part) => part.length > 0)
  const primary = subtags[0]
  if (!primary) return null

  if (CHINESE_PRIMARY_SUBTAGS.has(primary)) {
    const rest = subtags.slice(1)
    if (rest.some((tag) => TRADITIONAL_CHINESE_SUBTAGS.has(tag))) {
      return LANGUAGE_BY_LOWER_CODE.get("zh-hant") ?? null
    }
    if (
      primary === "yue" &&
      !rest.some((tag) => SIMPLIFIED_CHINESE_SUBTAGS.has(tag))
    ) {
      return LANGUAGE_BY_LOWER_CODE.get("zh-hant") ?? null
    }
    return LANGUAGE_BY_LOWER_CODE.get("zh-hans") ?? null
  }

  const byPrimary = LANGUAGE_BY_LOWER_CODE.get(primary)
  if (byPrimary) return byPrimary

  const alias = LANGUAGE_CODE_ALIASES[primary]
  if (alias) return LANGUAGE_BY_LOWER_CODE.get(alias) ?? null

  return null
}

/** Translates a language tag to its localized display label, falling back to the raw tag. */
export function formatBookLanguage(
  raw: string | null | undefined
): string | null {
  if (!raw) return null
  const trimmed = raw.trim()
  if (trimmed === "") return null
  return matchLanguageOption(trimmed)?.label ?? trimmed
}

/** Returns true for full `YYYY-MM-DD` dates supported by `<input type="date">`. */
export function isFullIsoDate(value: string): boolean {
  return /^\d{4}-\d{2}-\d{2}$/.test(value)
}

/** Formats an author list for display using locale-appropriate list separators. */
export function formatBookAuthors(
  authors: readonly string[],
  locale: Locale
): string | null {
  if (authors.length === 0) return null
  return authors.join(locale === "zh" ? "、" : ", ")
}

/** Trims author entries, drops blank entries, and deduplicates preserving first-seen order. */
export function normalizeAuthorInputs(values: readonly string[]): string[] {
  const seen = new Set<string>()
  const normalized: string[] = []
  for (const raw of values) {
    const trimmed = raw.trim()
    if (trimmed === "" || seen.has(trimmed)) continue
    seen.add(trimmed)
    normalized.push(trimmed)
  }
  return normalized
}

const MIME_BY_EXTENSION: Readonly<Record<string, string>> = {
  xhtml: "application/xhtml+xml",
  xht: "application/xhtml+xml",
  html: "text/html",
  htm: "text/html",
  svg: "image/svg+xml",
  svgz: "image/svg+xml",
  xml: "application/xml",
  opf: "application/oebps-package+xml",
  ncx: "application/x-dtbncx+xml",
  xpgt: "application/vnd.adobe-page-template+xml",
  smil: "application/smil+xml",
  css: "text/css",
  js: "text/javascript",
  mjs: "text/javascript",
  woff2: "font/woff2",
  woff: "font/woff",
  ttf: "font/ttf",
  otf: "font/otf",
  ttc: "font/collection",
  eot: "application/vnd.ms-fontobject",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  webp: "image/webp",
  gif: "image/gif",
  avif: "image/avif",
  mp3: "audio/mpeg",
  m4a: "audio/mp4",
  ogg: "audio/ogg",
  mp4: "video/mp4",
  webm: "video/webm",
}

function stripQueryAndFragment(rawPath: string): string {
  const queryIdx = rawPath.indexOf("?")
  const hashIdx = rawPath.indexOf("#")
  let end = rawPath.length
  if (queryIdx >= 0 && queryIdx < end) end = queryIdx
  if (hashIdx >= 0 && hashIdx < end) end = hashIdx
  return rawPath.slice(0, end)
}

function normalizePathSegments(path: string): string {
  const clean = path.replaceAll("\\", "/")
  const parts = clean.split("/")
  const stack: string[] = []

  for (const part of parts) {
    if (part.length === 0 || part === ".") continue
    if (part === "..") {
      stack.pop()
      continue
    }
    stack.push(part)
  }

  return stack.join("/")
}

export function normalizeArchiveLookupPath(rawPath: string): string {
  return normalizePathSegments(stripQueryAndFragment(rawPath))
}

function safeDecodeURIComponent(value: string): string | null {
  try {
    return decodeURIComponent(value)
  } catch {
    return null
  }
}

export function resolveArchiveEntryPath(
  rawPath: string,
  entries: Readonly<Record<string, number>>
): string | null {
  const normalized = normalizeArchiveLookupPath(rawPath)
  if (normalized.length === 0) return null
  if (Object.hasOwn(entries, normalized)) return normalized

  const decoded = safeDecodeURIComponent(normalized)
  const normalizedDecoded =
    decoded !== null ? normalizePathSegments(decoded) : null
  if (
    normalizedDecoded !== null &&
    normalizedDecoded.length > 0 &&
    Object.hasOwn(entries, normalizedDecoded)
  ) {
    return normalizedDecoded
  }

  const targetExactLower = normalized.toLowerCase()
  const targetDecodedLower =
    normalizedDecoded && normalizedDecoded.length > 0
      ? normalizedDecoded.toLowerCase()
      : null

  for (const key of Object.keys(entries)) {
    const keyLower = key.toLowerCase()
    if (
      keyLower === targetExactLower ||
      (targetDecodedLower !== null && keyLower === targetDecodedLower)
    ) {
      return key
    }
  }

  return null
}

export function lookupEntrySize(
  rawPath: string,
  entries: Readonly<Record<string, number>>
): number | null {
  const resolved = resolveArchiveEntryPath(rawPath, entries)
  if (resolved === null) return null
  const size = entries[resolved]
  return typeof size === "number" ? size : null
}

export function inferEpubEntryMime(path: string): string {
  const clean = stripQueryAndFragment(path)
  const fileName = clean.slice(clean.lastIndexOf("/") + 1)
  const dotIdx = fileName.lastIndexOf(".")
  if (dotIdx < 0 || dotIdx === fileName.length - 1) {
    return "application/octet-stream"
  }
  const ext = fileName.slice(dotIdx + 1).toLowerCase()
  return MIME_BY_EXTENSION[ext] ?? "application/octet-stream"
}

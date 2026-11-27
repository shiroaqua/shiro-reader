import { describe, expect, it } from "vitest"
import {
  inferEpubEntryMime,
  lookupEntrySize,
  normalizeArchiveLookupPath,
  resolveArchiveEntryPath,
} from "@shiro/ui-shared/features/reader/engine/epub/mime"

describe("EPUB archive path normalization & MIME inference", () => {
  it("normalizes leading slashes, dot segments, queries, and fragments", () => {
    expect(normalizeArchiveLookupPath("/OEBPS/./Text/../ch1.xhtml#sec-1")).toBe(
      "OEBPS/ch1.xhtml"
    )
    expect(normalizeArchiveLookupPath("OEBPS//styles.css?v=2#top")).toBe(
      "OEBPS/styles.css"
    )
    expect(normalizeArchiveLookupPath("\\META-INF\\container.xml")).toBe(
      "META-INF/container.xml"
    )
  })

  it("resolves direct and percent-encoded paths for getSize lookup", () => {
    const entries = {
      "META-INF/container.xml": 240,
      "OEBPS/My Chapter.xhtml": 1024,
    } as const

    expect(resolveArchiveEntryPath("./META-INF/container.xml", entries)).toBe(
      "META-INF/container.xml"
    )
    expect(lookupEntrySize("OEBPS/My%20Chapter.xhtml#p1", entries)).toBe(1024)
    expect(lookupEntrySize("OEBPS/missing.xhtml", entries)).toBeNull()
    expect(lookupEntrySize("OEBPS/%E0%A4%A.xhtml", entries)).toBeNull()

    const hashEntries = {
      "OEBPS/ch#1.xhtml": 512,
    } as const
    expect(resolveArchiveEntryPath("OEBPS/ch%231.xhtml#sec", hashEntries)).toBe(
      "OEBPS/ch#1.xhtml"
    )
    expect(lookupEntrySize("OEBPS/ch%231.xhtml#sec", hashEntries)).toBe(512)
  })

  it("infers exact MIME types case-insensitively and strips query/fragment", () => {
    expect(inferEpubEntryMime("OEBPS/ch1.XHTML#sec")).toBe(
      "application/xhtml+xml"
    )
    expect(inferEpubEntryMime("OEBPS/cover.SVG?t=1")).toBe("image/svg+xml")
    expect(inferEpubEntryMime("OEBPS/content.opf")).toBe(
      "application/oebps-package+xml"
    )
    expect(inferEpubEntryMime("OEBPS/fonts/body.WOFF2")).toBe("font/woff2")
    expect(inferEpubEntryMime("OEBPS/images/hero.avif")).toBe("image/avif")
    expect(inferEpubEntryMime("OEBPS/data.bin")).toBe(
      "application/octet-stream"
    )
    expect(inferEpubEntryMime("mimetype")).toBe("application/octet-stream")
    expect(inferEpubEntryMime("OEBPS/page-template.XPGT")).toBe(
      "application/vnd.adobe-page-template+xml"
    )
    expect(inferEpubEntryMime("OEBPS/fonts/legacy.EOT")).toBe(
      "application/vnd.ms-fontobject"
    )
    expect(inferEpubEntryMime("OEBPS/diagram.svgz")).toBe("image/svg+xml")

    const caseEntries = { "OEBPS/Images/Cover.JPG": 2048 } as const
    expect(resolveArchiveEntryPath("oebps/images/cover.jpg", caseEntries)).toBe(
      "OEBPS/Images/Cover.JPG"
    )
    expect(lookupEntrySize("OEBPS/images/COVER.jpg", caseEntries)).toBe(2048)
  })
})

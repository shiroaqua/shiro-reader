#!/usr/bin/env python3
"""Generates deterministic, spec-compliant offline smoke-test fixtures:
1. smoke_pattern.pdf: a valid PDF 1.4 document whose first page renders distinct non-white geometry.
2. smoke_cover.epub: a valid EPUB 3.0 container with a real PNG cover image declared in OPF manifest.
"""

from pathlib import Path
import struct
import zipfile
import zlib

FIXTURES_DIR = Path(__file__).resolve().parent / "fixtures"

CONTAINER_XML = """<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="EPUB/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>
"""

CONTENT_OPF = """<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id" xml:lang="en">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:11111111-2222-4333-8444-555555555555</dc:identifier>
    <dc:title>ShiroReader Smoke Cover Fixture</dc:title>
    <dc:creator>ShiroReader</dc:creator>
    <dc:language>en</dc:language>
    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
    <meta name="cover" content="cover-image"/>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="cover-image" href="cover.png" media-type="image/png" properties="cover-image"/>
    <item id="chapter1" href="chapter1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chapter1"/>
  </spine>
</package>
"""

NAV_XHTML = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="en">
  <head>
    <title>Navigation</title>
  </head>
  <body>
    <nav epub:type="toc" id="toc">
      <h1>Contents</h1>
      <ol>
        <li><a href="chapter1.xhtml">Chapter 1</a></li>
      </ol>
    </nav>
  </body>
</html>
"""

CHAPTER1_XHTML = """<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xml:lang="en">
  <head>
    <title>Chapter 1</title>
  </head>
  <body>
    <h1>ShiroReader Offline Smoke Fixture</h1>
    <p>Deterministic EPUB 3 fixture carrying an embedded raster PNG cover.</p>
  </body>
</html>
"""


def build_pattern_pdf() -> bytes:
    stream = (
        b"0.12 0.38 0.88 rg\n40 80 220 340 re f\n"
        b"0.92 0.25 0.22 rg\n85 140 130 220 re f\n"
        b"0.98 0.80 0.15 rg\n115 200 70 100 re f\n"
    )
    objects = [
        b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n",
        b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n",
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 500] /Contents 4 0 R >>\nendobj\n",
        (
            f"4 0 obj\n<< /Length {len(stream)} >>\nstream\n".encode("ascii")
            + stream
            + b"endstream\nendobj\n"
        ),
    ]
    buf = bytearray(b"%PDF-1.4\n")
    offsets = [0]
    for obj in objects:
        offsets.append(len(buf))
        buf.extend(obj)
    xref = len(buf)
    buf.extend(f"xref\n0 {len(objects) + 1}\n".encode("ascii"))
    buf.extend(b"0000000000 65535 f \n")
    for off in offsets[1:]:
        buf.extend(f"{off:010d} 00000 n \n".encode("ascii"))
    buf.extend(
        f"trailer\n<< /Size {len(objects) + 1} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode(
            "ascii"
        )
    )
    return bytes(buf)


def build_cover_png(width: int = 128, height: int = 192) -> bytes:
    rows = []
    for y in range(height):
        row = bytearray([0])
        for x in range(width):
            if 24 <= x < width - 24 and 32 <= y < height - 32:
                row.extend((244, 63, 94, 255))
            else:
                row.extend((37, 99, 235, 255))
        rows.append(bytes(row))
    raw = b"".join(rows)

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + tag
            + data
            + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)
        )

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(raw, 6))
        + chunk(b"IEND", b"")
    )


def build_cover_epub() -> bytes:
    cover_png = build_cover_png()
    out_path = FIXTURES_DIR / "smoke_cover.epub"
    fixed_dt = (2026, 1, 1, 0, 0, 0)

    def add_entry(
        archive: zipfile.ZipFile,
        name: str,
        data: bytes,
        compress_type: int,
    ) -> None:
        info = zipfile.ZipInfo(filename=name, date_time=fixed_dt)
        info.compress_type = compress_type
        info.external_attr = 0o644 << 16
        archive.writestr(info, data, compress_type=compress_type)

    with zipfile.ZipFile(out_path, "w") as zout:
        # EPUB OCF mandates mimetype as the very first entry, uncompressed, without extra fields.
        add_entry(zout, "mimetype", b"application/epub+zip", zipfile.ZIP_STORED)
        add_entry(zout, "META-INF/container.xml", CONTAINER_XML.encode("utf-8"), zipfile.ZIP_DEFLATED)
        add_entry(zout, "EPUB/content.opf", CONTENT_OPF.encode("utf-8"), zipfile.ZIP_DEFLATED)
        add_entry(zout, "EPUB/nav.xhtml", NAV_XHTML.encode("utf-8"), zipfile.ZIP_DEFLATED)
        add_entry(zout, "EPUB/chapter1.xhtml", CHAPTER1_XHTML.encode("utf-8"), zipfile.ZIP_DEFLATED)
        add_entry(zout, "EPUB/cover.png", cover_png, zipfile.ZIP_DEFLATED)

    return out_path.read_bytes()


def main() -> None:
    FIXTURES_DIR.mkdir(parents=True, exist_ok=True)
    pdf_path = FIXTURES_DIR / "smoke_pattern.pdf"
    pdf_path.write_bytes(build_pattern_pdf())
    epub_bytes = build_cover_epub()
    print(f"Generated {pdf_path} ({pdf_path.stat().st_size} bytes)")
    print(f"Generated {FIXTURES_DIR / 'smoke_cover.epub'} ({len(epub_bytes)} bytes)")


if __name__ == "__main__":
    main()

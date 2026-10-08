import struct
import zlib

import pytest

from sve_carddb.ingest.http.validate import (
    ValidationError,
    check_image,
    check_jpeg,
    check_pdf,
    check_png,
    decode_html,
    require_media_type,
)


def png_chunk(chunk_type: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(chunk_type + data)
    return struct.pack(">I", len(data)) + chunk_type + data + struct.pack(">I", crc)


IHDR = png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 0, 0, 0, 0))
IDAT = png_chunk(b"IDAT", zlib.compress(b"\x00\x00"))
IEND = png_chunk(b"IEND", b"")
SIGNATURE = b"\x89PNG\r\n\x1a\n"
PNG = SIGNATURE + IHDR + IDAT + IEND


def jpeg_segment(marker: int, data: bytes) -> bytes:
    return bytes([0xFF, marker]) + struct.pack(">H", len(data) + 2) + data


SOF0 = jpeg_segment(0xC0, bytes([8, 0, 1, 0, 1, 1, 1, 0x11, 0]))
SOS = jpeg_segment(0xDA, bytes([1, 1, 0, 0, 0x3F, 0]))
JPEG = b"\xff\xd8" + SOF0 + SOS + b"\x12\x34" + b"\xff\xd9"


def test_valid_png_passes() -> None:
    check_png(PNG)
    check_image(PNG, max_bytes=1024)


@pytest.mark.parametrize(
    ("body", "reason"),
    [
        pytest.param(PNG[:-6], "truncated", id="truncated"),
        pytest.param(SIGNATURE + IDAT + IEND, "IHDR", id="no IHDR first"),
        pytest.param(
            SIGNATURE + png_chunk(b"IHDR", b"short") + IDAT + IEND,
            "IHDR",
            id="IHDR wrong length",
        ),
        pytest.param(SIGNATURE + IHDR + IEND, "IDAT", id="no IDAT"),
        pytest.param(SIGNATURE + IHDR + IDAT, "IEND", id="no IEND"),
        pytest.param(PNG + b"junk", "after IEND", id="trailing data"),
        pytest.param(PNG[:-1] + bytes([PNG[-1] ^ 1]), "CRC", id="bad CRC"),
    ],
)
def test_broken_png_fails(body: bytes, reason: str) -> None:
    with pytest.raises(ValidationError, match=reason):
        check_png(body)


def test_valid_jpeg_passes() -> None:
    check_jpeg(JPEG)
    check_image(JPEG, max_bytes=1024)


@pytest.mark.parametrize(
    ("body", "reason"),
    [
        pytest.param(b"\xff\xd8\xff\xd9", "truncated", id="SOI and EOI only"),
        pytest.param(b"\xff\xd8" + SOS + b"\xff\xd9", "frame header", id="no SOF"),
        pytest.param(JPEG[:-2], "EOI", id="no EOI"),
        pytest.param(b"\xff\xd8\x00\x00\xff\xd9", "marker", id="garbage"),
    ],
)
def test_broken_jpeg_fails(body: bytes, reason: str) -> None:
    with pytest.raises(ValidationError, match=reason):
        check_jpeg(body)


def test_image_size_limit() -> None:
    with pytest.raises(ValidationError, match="over the"):
        check_image(PNG, max_bytes=len(PNG) - 1)


def test_unknown_image_format_fails() -> None:
    with pytest.raises(ValidationError, match="neither PNG nor JPEG"):
        check_image(b"GIF89a....", max_bytes=1024)


@pytest.mark.parametrize(
    ("content_type", "expected"),
    [
        ("text/html; charset=UTF-8", "text/html"),
        ("TEXT/HTML", "text/html"),
        ("image/png", "image/"),
        ("application/pdf", "application/pdf"),
    ],
)
def test_media_type_accepts(content_type: str, expected: str) -> None:
    require_media_type(content_type, expected)


@pytest.mark.parametrize("content_type", [None, "", "text/plain", "application/json"])
def test_media_type_rejects(content_type: str | None) -> None:
    with pytest.raises(ValidationError, match="content type"):
        require_media_type(content_type, "text/html")


def test_decode_html() -> None:
    body = "<html>合成の文字</html>".encode()
    assert decode_html(body, min_bytes=10) == "<html>合成の文字</html>"


def test_decode_html_rejects_short_page() -> None:
    with pytest.raises(ValidationError, match="at least"):
        decode_html(b"<html></html>", min_bytes=100)


def test_decode_html_rejects_invalid_utf8() -> None:
    with pytest.raises(ValidationError, match="UTF-8"):
        decode_html(b"<html>\xff\xfe</html>", min_bytes=1)


def test_check_pdf() -> None:
    check_pdf(b"%PDF-1.7\n...")
    with pytest.raises(ValidationError, match="PDF"):
        check_pdf(b"<html>")

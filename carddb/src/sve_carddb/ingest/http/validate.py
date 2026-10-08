"""Checks every response must pass before it is stored.

A 200 status is not success: maintenance pages, redirects to the home page and
truncated files all come back as 200. Page-specific checks (the card number on a
card page, list totals) live with each source; this module holds the generic ones.

The image checks are structural only: they catch truncation and some format
errors but do not prove the image decodes. Decoding happens later, when
thumbnails are made.
"""

import struct
import zlib

_PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
_PNG_CHUNK_HEADER = struct.Struct(">I4s")
_PNG_IHDR_LENGTH = 13
_JPEG_MARKER = 0xFF
_JPEG_SOI = b"\xff\xd8"
_JPEG_EOI = b"\xff\xd9"
_JPEG_SOS = 0xDA
# SOF0-SOF15, except DHT (C4), JPG (C8) and DAC (CC), which share the range.
_JPEG_SOF = frozenset(range(0xC0, 0xD0)) - {0xC4, 0xC8, 0xCC}
# Markers without a length field.
_JPEG_STANDALONE = frozenset({0x01, *range(0xD0, 0xD8)})


class ValidationError(ValueError):
    """The response is not the content we asked for."""


def require_media_type(content_type: str | None, expected: str) -> None:
    """Require `content_type` to start with `expected` (e.g. `text/html`, `image/`)."""
    media = (content_type or "").split(";", 1)[0].strip().lower()
    if not media.startswith(expected):
        msg = f"content type {content_type!r} is not {expected!r}"
        raise ValidationError(msg)


def decode_html(body: bytes, *, min_bytes: int) -> str:
    """Return the page as text, rejecting pages too short to be real content."""
    if len(body) < min_bytes:
        msg = f"page is {len(body)} bytes, expected at least {min_bytes}"
        raise ValidationError(msg)
    try:
        return body.decode()
    except UnicodeDecodeError as exc:
        msg = "page is not valid UTF-8"
        raise ValidationError(msg) from exc


def check_pdf(body: bytes) -> None:
    """Rough check only: the header is right. It does not prove the PDF is complete."""
    if not body.startswith(b"%PDF-"):
        msg = "not a PDF"
        raise ValidationError(msg)


def check_image(body: bytes, *, max_bytes: int) -> None:
    """Structural check of a PNG or JPEG."""
    if len(body) > max_bytes:
        msg = f"image is {len(body)} bytes, over the {max_bytes} limit"
        raise ValidationError(msg)
    if body.startswith(_PNG_SIGNATURE):
        check_png(body)
    elif body.startswith(_JPEG_SOI):
        check_jpeg(body)
    else:
        msg = "image is neither PNG nor JPEG"
        raise ValidationError(msg)


def check_png(body: bytes) -> None:
    """Walk every chunk: CRCs must match, IHDR first, at least one IDAT, IEND last."""
    chunks, end = _png_chunks(body)
    if not chunks or chunks[0] != (b"IHDR", _PNG_IHDR_LENGTH):
        msg = "PNG does not start with a valid IHDR"
        raise ValidationError(msg)
    types = [chunk_type for chunk_type, _ in chunks]
    if b"IDAT" not in types:
        msg = "PNG has no IDAT chunk"
        raise ValidationError(msg)
    if types[-1] != b"IEND":
        msg = "PNG does not end with IEND"
        raise ValidationError(msg)
    if end != len(body):
        msg = "PNG has data after IEND"
        raise ValidationError(msg)


def _png_chunks(body: bytes) -> tuple[list[tuple[bytes, int]], int]:
    """Return `(type, length)` of each chunk up to IEND, and where reading stopped."""
    offset = len(_PNG_SIGNATURE)
    chunks: list[tuple[bytes, int]] = []
    while offset < len(body):
        if offset + _PNG_CHUNK_HEADER.size > len(body):
            msg = "PNG chunk header is truncated"
            raise ValidationError(msg)
        length, chunk_type = _PNG_CHUNK_HEADER.unpack_from(body, offset)
        crc_start = offset + _PNG_CHUNK_HEADER.size + length
        if crc_start + 4 > len(body):
            msg = f"PNG {chunk_type!r} chunk is truncated"
            raise ValidationError(msg)
        (crc,) = struct.unpack_from(">I", body, crc_start)
        if zlib.crc32(body[offset + 4 : crc_start]) != crc:
            msg = f"PNG {chunk_type!r} chunk has a bad CRC"
            raise ValidationError(msg)
        chunks.append((chunk_type, length))
        offset = crc_start + 4
        if chunk_type == b"IEND":
            break
    return chunks, offset


def check_jpeg(body: bytes) -> None:
    """Segments must parse up to the scan, with a frame header before it and EOI at the end."""
    if not body.startswith(_JPEG_SOI) or not body.endswith(_JPEG_EOI):
        msg = "JPEG does not start with SOI and end with EOI"
        raise ValidationError(msg)
    offset = len(_JPEG_SOI)
    seen_frame = False
    while True:
        if offset + 2 > len(body) or body[offset] != _JPEG_MARKER:
            msg = "JPEG segment marker expected"
            raise ValidationError(msg)
        marker = body[offset + 1]
        if marker == _JPEG_MARKER:  # fill byte before a marker
            offset += 1
            continue
        if marker in _JPEG_STANDALONE:
            offset += 2
            continue
        if offset + 4 > len(body):
            msg = "JPEG segment is truncated"
            raise ValidationError(msg)
        (length,) = struct.unpack_from(">H", body, offset + 2)
        if marker == _JPEG_SOS:
            break
        seen_frame = seen_frame or marker in _JPEG_SOF
        offset += 2 + length
    if not seen_frame:
        msg = "JPEG has no frame header before the scan"
        raise ValidationError(msg)

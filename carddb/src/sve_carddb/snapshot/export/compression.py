"""Deterministic compression and bounded verification of transfer encodings."""

import gzip
import platform
import zlib
from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol, cast

import brotli as library

if TYPE_CHECKING:
    from collections.abc import Callable


@dataclass(frozen=True)
class Brotli:
    """Explicit pinned compressor supplied by the build recipe, at quality 11."""

    version: str
    compress: Callable[[bytes], bytes]


def python_brotli() -> Brotli:
    """Record the installed package version, keeping quality and window explicit."""

    def encode(raw: bytes) -> bytes:
        try:
            return library.compress(
                raw, mode=library.MODE_GENERIC, quality=11, lgwin=22
            )
        except library.error:
            raise ValueError("Brotli compression failed") from None

    return Brotli("brotli " + library.__version__, encode)


class _BoundedDecoder(Protocol):
    # brotli-stubs 1.2.0 omits the native 1.2.0 bounded streaming API.
    def process(self, data: bytes, *, output_buffer_limit: int) -> bytes: ...
    def is_finished(self) -> bool: ...


def verify_brotli(encoded: bytes, raw: bytes) -> None:
    """Accept any complete Brotli stream of these exact bytes without recompression."""
    decoder = cast("_BoundedDecoder", library.Decompressor())
    offset = 0
    pending = encoded
    try:
        while True:
            decoded = decoder.process(pending, output_buffer_limit=64 * 1024)
            end = offset + len(decoded)
            if end > len(raw) or decoded != raw[offset:end]:
                raise ValueError("Invalid Brotli representation")
            offset = end
            if decoder.is_finished():
                if offset != len(raw):
                    raise ValueError("Invalid Brotli representation")
                return
            # Consuming all input can still leave buffered output to drain.
            if not decoded:
                raise ValueError("Invalid Brotli representation")
            pending = b""
    except library.error:
        raise ValueError("Invalid Brotli representation") from None


@dataclass(frozen=True)
class Blob:
    """Raw canonical bytes and optional alternative transfer encodings."""

    raw: bytes
    br: bytes | None
    gzip: bytes


def compress(raw: bytes, brotli: Brotli | None) -> Blob:
    """Gzip has no filename and a zero timestamp; absent Brotli is explicit null."""
    return Blob(
        raw,
        None if brotli is None else brotli.compress(raw),
        gzip.compress(raw, compresslevel=9, mtime=0),
    )


def recipe(brotli: Brotli | None) -> dict[str, str | int | None]:
    """Keep implementation versions in the build recipe, outside wire payloads."""
    return {
        "canonical": "canonical-json-v1",
        "python": platform.python_version(),
        "gzip": zlib.ZLIB_VERSION,
        "gzip_level": 9,
        "gzip_mtime": 0,
        "brotli": None if brotli is None else brotli.version,
        "brotli_quality": None if brotli is None else 11,
    }

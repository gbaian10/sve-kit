"""Deterministic compression and bounded verification of transfer encodings."""

import gzip
import platform
import zlib
from dataclasses import dataclass
from typing import TYPE_CHECKING

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

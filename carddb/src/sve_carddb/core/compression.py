"""Bounded verification of already encoded public bytes."""

from typing import Protocol, cast

import brotli as library


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

"""Local paths under the data root and compressed storage of raw pages."""

from compression import zstd
from pathlib import Path, PurePosixPath

_FORBIDDEN_SEGMENTS = frozenset({"", ".", ".."})


class UnsafePathError(ValueError):
    """A path built from remote input would leave the data root."""


class CorruptDataError(ValueError):
    """Stored data cannot be decoded."""


def relpath(*segments: str) -> PurePosixPath:
    """Join segments taken from remote input into a path relative to the data root.

    Each segment must be a single path component: no separators, no `.` or `..`,
    no NUL. Other characters, including spaces and `Ⓢ`, are kept as-is.
    """
    for segment in segments:
        if (
            segment in _FORBIDDEN_SEGMENTS
            or "/" in segment
            or "\\" in segment
            or "\0" in segment
        ):
            msg = f"unsafe path segment: {segment!r}"
            raise UnsafePathError(msg)
    return PurePosixPath(*segments)


def resolve_within(root: Path, rel: PurePosixPath) -> Path:
    """Return the absolute path of `rel`, refusing anything outside `root`.

    Resolving follows symlinks, so a symlink inside the root that points
    elsewhere is rejected as well.
    """
    if rel.is_absolute():
        msg = f"expected a relative path: {rel}"
        raise UnsafePathError(msg)
    base = root.resolve()
    target = (base / rel).resolve()
    if not target.is_relative_to(base):
        msg = f"{rel} resolves outside {base}"
        raise UnsafePathError(msg)
    return target


def compress(raw: bytes) -> bytes:
    """Compress raw page content for storage."""
    return zstd.compress(raw)


def decompress(stored: bytes) -> bytes:
    """Decompress stored page content, or raise `CorruptDataError`."""
    try:
        return zstd.decompress(stored)
    except zstd.ZstdError as exc:
        msg = "stored data is not valid zstd"
        raise CorruptDataError(msg) from exc

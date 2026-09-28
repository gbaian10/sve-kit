"""Local paths under the data root and compressed storage of raw pages."""

import os
from compression import zstd
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Iterable

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


def resolve_within(
    root: Path, rel: PurePosixPath, *, also_allowed: Iterable[Path] = ()
) -> Path:
    """Return the absolute path of `rel` under `root`, refusing anything outside.

    Resolving follows symlinks, so a symlink inside the root that points
    elsewhere is rejected as well, unless it lands inside one of the
    `also_allowed` roots named explicitly by the user.
    """
    if rel.is_absolute():
        msg = f"expected a relative path: {rel}"
        raise UnsafePathError(msg)
    base = root.resolve()
    # `..` must not leave the root even on the way to an allowed root: only a
    # symlink the user placed inside the root may lead there.
    if not Path(os.path.normpath(base / rel)).is_relative_to(base):
        msg = f"{rel} resolves outside {base}"
        raise UnsafePathError(msg)
    target = (base / rel).resolve()
    allowed = [base, *(extra.resolve() for extra in also_allowed)]
    if not any(target.is_relative_to(parent) for parent in allowed):
        msg = f"{rel} resolves outside {', '.join(map(str, allowed))}"
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

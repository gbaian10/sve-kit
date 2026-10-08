"""Pure validation of relative paths assembled from untrusted segments."""

from pathlib import PurePosixPath

_FORBIDDEN_SEGMENTS = frozenset({"", ".", ".."})


class UnsafePathError(ValueError):
    """A path built from remote input would leave the data root."""


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

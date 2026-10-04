"""Shared redacted errors and non-symlink local member reads for R2 publication."""

import stat
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pathlib import Path


class UploadError(ValueError):
    """A redacted validation or remote error safe for the operator's log."""


def read_member(root: Path, key: str) -> bytes:
    """Reject symlinks and special files before reading a public member."""
    if key.startswith("/") or ".." in key.split("/"):
        raise UploadError("Invalid public member key")
    target = root / key
    if target.resolve() != target or not stat.S_ISREG(target.lstat().st_mode):
        raise UploadError("Public member is not a regular non-symlink file")
    return target.read_bytes()

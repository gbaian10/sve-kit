"""Canonical primary-key hashing for snapshot buckets."""

import hashlib
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical

if TYPE_CHECKING:
    from pydantic import JsonValue


def bucket(key: list[JsonValue], count: int) -> int:
    """Hash the canonical primary-key array using all 256 bits."""
    if count <= 0:
        raise ValueError("Bucket count must be positive")
    return int.from_bytes(hashlib.sha256(canonical(key)).digest(), "big") % count

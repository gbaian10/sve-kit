"""Verify only public printing WebPs while keeping image inputs read-only."""

import re
from pathlib import PurePosixPath
from typing import TYPE_CHECKING

from sve_carddb.core.json import integer, string
from sve_carddb.image_checks import ImageChecks
from sve_carddb.image_variants import SIZES
from sve_carddb.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from sve_carddb.snapshot.project.source import Record

PATH = re.compile(r"images/sha256/([0-9a-f]{2})/([0-9a-f]{64})\.webp\Z")


def _members(tables: dict[str, list[Record]]) -> dict[str, Record]:
    active = {
        string(row["image_id"])
        for row in tables["printing_image"]
        if row["availability"] == "available" and row["publication_state"] == "approved"
    }
    members: dict[str, Record] = {}
    for variant in tables["image_variant"]:
        if string(variant["image_id"]) not in active:
            raise ValueError("Only approved available printing images can be published")
        path = string(variant["path"])
        match = PATH.fullmatch(path)
        if variant["format"] != "webp" or match is None or match[1] != match[2][:2]:
            raise ValueError("Preview image requires a content-addressed WebP path")
        if path in members and any(
            members[path][field] != variant[field]
            for field in ("width", "height", "bytes")
        ):
            raise ValueError("Shared image blob metadata disagrees")
        members[path] = variant
    _require_sizes(active, tables["image_variant"])
    return members


def _require_sizes(active: set[str], variants: list[Record]) -> None:
    """Text-only or unapproved metadata must not claim a complete image result."""
    sizes: dict[str, set[str]] = {}
    for variant in variants:
        sizes.setdefault(string(variant["image_id"]), set()).add(
            string(variant["size_key"])
        )
    expected = {size.key for size in SIZES}
    for identifier in active:
        if sizes.get(identifier, set()) != expected:
            raise ValueError("Available preview printing image requires all five sizes")


def image_blobs(
    tables: dict[str, list[Record]],
    source: Path | None,
    *,
    checks: ImageChecks | None = None,
) -> Iterator[tuple[str, bytes]]:
    """Check public selection and stream deduplicated bytes without retaining the library."""
    checks = checks or ImageChecks()
    members = _members(tables)
    if members and source is None:
        raise ValueError("Preview images require an explicit asset source")
    if source is None:
        return
    if not source.is_absolute() or source.is_symlink():
        raise ValueError("Preview image input must be an absolute non-symlink root")
    for path, variant in sorted(members.items()):
        raw = resolve_within(source, PurePosixPath(path)).read_bytes()
        metadata = checks.inspect(resolve_within(source, PurePosixPath(path)))
        if metadata != (
            integer(variant["bytes"]),
            "sha256:" + PurePosixPath(path).stem,
            "WEBP",
            (integer(variant["width"]), integer(variant["height"])),
        ):
            raise ValueError("Preview image hash, bytes, format or dimensions mismatch")
        yield path, raw

"""Verify only public printing WebPs while keeping image inputs read-only."""

import re
from io import BytesIO
from pathlib import PurePosixPath
from typing import TYPE_CHECKING

from PIL import Image

from sve_carddb.image_variants import SIZES
from sve_carddb.snapshot.values import digest, integer, string
from sve_carddb.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from sve_carddb.snapshot.project.source import Record

PATH = re.compile(r"images/sha256/([0-9a-f]{2})/([0-9a-f]{64})\.webp\Z")


def _members(
    tables: dict[str, list[Record]], confirmed_images: frozenset[str]
) -> dict[str, Record]:
    """Require the caller's DB-verified review set, not an independent review audit."""
    assets = {string(row["id"]): row for row in tables["image_asset"]}
    for identifier, asset in assets.items():
        if (
            asset["origin"] == "third_party"
            and asset["publication_state"] == "approved"
            and identifier not in confirmed_images
        ):
            raise ValueError(
                "Third-party preview image needs individual confirmed review"
            )
    bound = {string(row["image_id"]) for row in tables["printing_image"]}
    members: dict[str, Record] = {}
    for variant in tables["image_variant"]:
        asset = assets[string(variant["image_id"])]
        if (
            variant["image_id"] not in bound
            or asset["availability"] != "available"
            or asset["publication_state"] != "approved"
        ):
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
    _require_sizes(assets, bound, tables["image_variant"])
    return members


def _require_sizes(
    assets: dict[str, Record], bound: set[str], variants: list[Record]
) -> None:
    """Text-only or unapproved metadata must not claim a complete image result."""
    sizes: dict[str, set[str]] = {}
    for variant in variants:
        sizes.setdefault(string(variant["image_id"]), set()).add(
            string(variant["size_key"])
        )
    expected = {size.key for size in SIZES}
    for identifier in bound:
        asset = assets[identifier]
        if (
            asset["availability"] == "available"
            and asset["publication_state"] == "approved"
            and sizes.get(identifier, set()) != expected
        ):
            raise ValueError("Available preview printing image requires all five sizes")


def image_blobs(
    tables: dict[str, list[Record]],
    source: Path | None,
    confirmed_images: frozenset[str] = frozenset(),
) -> Iterator[tuple[str, bytes]]:
    """Check audit gates and stream deduplicated bytes without retaining the library."""
    members = _members(tables, confirmed_images)
    if members and source is None:
        raise ValueError("Preview images require an explicit asset source")
    if source is None:
        return
    if not source.is_absolute() or source.is_symlink():
        raise ValueError("Preview image input must be an absolute non-symlink root")
    for path, variant in sorted(members.items()):
        raw = resolve_within(source, PurePosixPath(path)).read_bytes()
        if digest(raw)[7:] != PurePosixPath(path).stem or len(raw) != integer(
            variant["bytes"]
        ):
            raise ValueError("Preview image hash or bytes mismatch")
        with Image.open(BytesIO(raw)) as decoded:
            if decoded.format != "WEBP" or decoded.size != (
                integer(variant["width"]),
                integer(variant["height"]),
            ):
                raise ValueError("Preview image decoded format or dimensions mismatch")
            decoded.load()
        yield path, raw


def verify_images(
    tables: dict[str, list[Record]], source: Path, confirmed_images: frozenset[str]
) -> None:
    """Recheck published assets before switching the pointer to their manifest."""
    for _path, _raw in image_blobs(tables, source, confirmed_images):
        pass

"""Verify the complete public transport before allowing any remote I/O."""

import gzip
import os
import re
import stat
from collections import Counter
from dataclasses import dataclass
from io import BytesIO
from pathlib import Path
from typing import TYPE_CHECKING
from urllib.parse import urlsplit

from jsonschema import ValidationError as SchemaError
from PIL import Image

from sve_carddb.image_variants import SIZES
from sve_carddb.snapshot.publication import require_preview
from sve_carddb.snapshot.reader import read_snapshot, read_text_all
from sve_carddb.snapshot.values import (
    array,
    digest,
    integer,
    object_value,
    parse,
    string,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.snapshot.export import Brotli

POINTER = "snapshots/preview/current.json"
_JSON = re.compile(r"snapshots/(?:blobs|manifests)/[0-9a-f]{64}\.json\Z")
_IMAGE = re.compile(r"images/sha256/([0-9a-f]{2})/([0-9a-f]{64})\.webp\Z")
_LOCAL = re.compile(
    r"(?:^|[\s\"'=])(?:/(?:home|Users|srv|tmp|mnt|var|etc|opt|root|run|media)/|file://|[A-Za-z]:[\\/])"
)
_ABSOLUTE = re.compile(r"(?:^|[\s\"'=])/(?!/)[^/\s]+(?:/|$)")
IMMUTABLE_CACHE = "private, max-age=31536000, immutable"


class UploadError(ValueError):
    """A redacted validation or remote error safe for the operator's log."""


@dataclass(frozen=True)
class Member:
    key: str
    size: int
    sha256: str
    content_type: str
    phase: int

    @property
    def cache_control(self) -> str:
        """Keep development responses private and pointers uncached."""
        return "no-store" if self.key == POINTER else IMMUTABLE_CACHE


@dataclass(frozen=True)
class Plan:
    root: Path
    members: tuple[Member, ...]
    manifest_sha256: str

    def report(self) -> dict[str, object]:
        """Reconcile local candidates without guessing remote existence."""
        counts = Counter(
            "images" if m.key.startswith("images/") else "snapshots"
            for m in self.members
        )
        return {
            "mode": "offline_dry_run",
            "candidate_files": len(self.members),
            "candidate_bytes": sum(m.size for m in self.members),
            "files_by_kind": dict(counts),
            "bytes_by_kind": {
                kind: sum(m.size for m in self.members if m.key.startswith(kind + "/"))
                for kind in counts
            },
            "excluded_roots": ["private", "reports"],
            "remote_existence": "not_checked",
            "manifest_sha256": self.manifest_sha256,
        }


def read_member(root: Path, key: str) -> bytes:
    """Reject symlinks and special files before reading a public member."""
    if key.startswith("/") or ".." in key.split("/"):
        raise UploadError("Invalid public member key")
    target = root / key
    if target.resolve() != target or not stat.S_ISREG(target.lstat().st_mode):
        raise UploadError("Public member is not a regular non-symlink file")
    return target.read_bytes()


def _scan(root: Path) -> dict[str, bytes | None]:
    if not root.is_absolute() or root.resolve() != root or not root.is_dir():
        raise UploadError("Preview root must be an absolute non-symlink directory")
    if any(
        p.name not in {"snapshots", "images", "private", "reports"}
        for p in root.iterdir()
    ):
        raise UploadError("Unexpected entry at preview root")
    found: dict[str, bytes | None] = {}
    for name in ("snapshots", "images"):
        subtree = root / name
        if not subtree.exists() and not subtree.is_symlink():
            continue
        if subtree.is_symlink() or not subtree.is_dir():
            raise UploadError("Public subtree must be a non-symlink directory")
        for parent, directories, files in os.walk(subtree, followlinks=False):
            if any((Path(parent) / d).is_symlink() for d in directories):
                raise UploadError("Public subtree contains a symlink directory")
            for filename in files:
                key = (Path(parent) / filename).relative_to(root).as_posix()
                base = key.rsplit(".", 1)[0] if key.endswith((".gz", ".br")) else key
                if (
                    key != POINTER
                    and _JSON.fullmatch(base) is None
                    and _IMAGE.fullmatch(key) is None
                ):
                    raise UploadError("Unsupported public member")
                # Keep image bytes streaming; the JSON transport is small and independently joined.
                found[key] = (
                    None if key.startswith("images/") else read_member(root, key)
                )
    return found


def _check_paths(item: JsonValue, *, image_payload: bool = False) -> None:
    if isinstance(item, str) and (
        _LOCAL.search(item) or (not image_payload and _ABSOLUTE.search(item))
    ):
        raise UploadError("Public content contains a local path")
    if isinstance(item, dict):
        for key, child in item.items():
            _check_paths(key, image_payload=image_payload)
            _check_paths(child, image_payload=image_payload)
    elif isinstance(item, list):
        for child in item:
            _check_paths(child, image_payload=image_payload)


def _public_json(raw: bytes, *, image_payload: bool = False) -> JsonValue:
    value = parse(raw)
    _check_paths(value, image_payload=image_payload)
    return value


def pointer_value(raw: bytes) -> dict[str, JsonValue]:
    """Validate the exact preview pointer rather than trusting an arbitrary remote path."""
    value = object_value(_public_json(raw))
    if set(value) != {"manifest_path", "manifest_sha256"}:
        raise UploadError("Invalid preview pointer")
    hashed = string(value["manifest_sha256"])
    if (
        not re.fullmatch(r"sha256:[0-9a-f]{64}", hashed)
        or value["manifest_path"] != "snapshots/manifests/" + hashed[7:] + ".json"
    ):
        raise UploadError("Invalid preview pointer")
    return value


def _encodings(
    key: str,
    raw: bytes,
    files: dict[str, bytes | None],
    allowed: set[str],
    brotli: Brotli | None,
) -> None:
    if files.get(key + ".gz") != gzip.compress(raw, compresslevel=9, mtime=0):
        raise UploadError("Missing or inconsistent canonical gzip member")
    allowed.add(key + ".gz")
    if key + ".br" in files:
        if brotli is None:
            raise UploadError("Brotli members require the explicit producer compressor")
        if files[key + ".br"] != brotli.compress(raw):
            raise UploadError("Inconsistent Brotli member")
        allowed.add(key + ".br")


def plan_preview(root: Path, *, brotli: Brotli | None = None) -> Plan:
    """Check all complete local versions, excluding private/report trees without reading them."""
    try:
        return _plan(root, brotli)
    except UploadError:
        raise
    except OSError, ValueError, KeyError, TypeError, SchemaError:
        # Schema/parser/OS exceptions can contain official text or private filenames.
        raise UploadError("Public preview validation failed") from None


def _plan(root: Path, brotli: Brotli | None) -> Plan:
    files = _scan(root)
    pointer_raw = files.get(POINTER)
    if pointer_raw is None:
        raise UploadError("Preview pointer is missing")
    pointer = pointer_value(pointer_raw)
    allowed = {POINTER}
    expected_images: dict[str, tuple[int, int, int]] = {}
    manifests = sorted(
        key
        for key in files
        if key.startswith("snapshots/manifests/") and key.endswith(".json")
    )
    if pointer["manifest_path"] not in manifests:
        raise UploadError("Pointed manifest is missing")
    for key in manifests:
        raw = files[key]
        assert raw is not None
        if digest(raw)[7:] != Path(key).stem:
            raise UploadError("Content-addressed JSON hash mismatch")
        manifest = object_value(_public_json(raw))
        require_preview(
            manifest,
            regions=tuple(string(region) for region in array(manifest["regions"])),
        )
        allowed.add(key)
        _encodings(key, raw, files, allowed, brotli)
        joined = _payloads(manifest, files, allowed, brotli)
        _image_metadata(joined, expected_images, allowed)
    if set(files) != allowed:
        raise UploadError("Public tree contains unreferenced or private members")
    members = [_member(root, key, files, expected_images) for key in sorted(allowed)]
    return Plan(
        root,
        tuple(sorted(members, key=lambda m: (m.phase, m.key))),
        string(pointer["manifest_sha256"]),
    )


def _payloads(
    manifest: dict[str, JsonValue],
    files: dict[str, bytes | None],
    allowed: set[str],
    brotli: Brotli | None,
) -> dict[str, list[dict[str, JsonValue]]]:
    descriptions = [object_value(d) for d in array(manifest["files"])]
    union = object_value(manifest["text_all"])
    payloads: dict[str, bytes] = {}
    for description in [*descriptions, union]:
        path = string(description["path"])
        if path != "snapshots/blobs/" + string(description["sha256"])[7:] + ".json":
            raise UploadError("Invalid content-addressed payload path")
        data = files.get(path)
        if (
            data is None
            or digest(data) != description["sha256"]
            or len(data) != description["bytes"]
        ):
            raise UploadError("Payload hash or size mismatch")
        _public_json(data, image_payload=description.get("role") == "images")
        allowed.add(path)
        _encodings(path, data, files, allowed, brotli)
        compressed = object_value(description["compressed_bytes"])
        for codec, suffix in (("gzip", ".gz"), ("br", ".br")):
            encoded = files.get(path + suffix)
            if compressed[codec] != (None if encoded is None else len(encoded)):
                raise UploadError("Compressed payload size mismatch")
        if description is not union:
            payloads[string(description["key"])] = data
    joined = read_snapshot(manifest, payloads)
    contained = {string(object_value(v)["key"]) for v in array(union["contains"])}
    union_raw = files[string(union["path"])]
    assert union_raw is not None
    if (
        read_text_all(
            manifest,
            union_raw,
            {k: v for k, v in payloads.items() if k not in contained},
        )
        != joined
    ):
        raise UploadError("Text union differs from snapshot shards")
    return joined


def _image_metadata(
    joined: dict[str, list[dict[str, JsonValue]]],
    expected_images: dict[str, tuple[int, int, int]],
    allowed: set[str],
) -> None:
    assets = {string(row["id"]): row for row in joined["image_asset"]}
    bound = {string(row["image_id"]) for row in joined["printing_image"]}
    _asset_paths(assets)
    for variant in joined["image_variant"]:
        asset = assets[string(variant["image_id"])]
        if (
            variant["image_id"] not in bound
            or asset["availability"] != "available"
            or asset["publication_state"] != "approved"
        ):
            raise UploadError("Image variant lacks an approved public binding")
        path = string(variant["path"])
        match = _IMAGE.fullmatch(path)
        if variant["format"] != "webp" or match is None or match[1] != match[2][:2]:
            raise UploadError("Invalid public WebP path")
        shape = (
            integer(variant["width"]),
            integer(variant["height"]),
            integer(variant["bytes"]),
        )
        if path in expected_images and expected_images[path] != shape:
            raise UploadError("Shared image metadata disagrees")
        expected_images[path] = shape
        allowed.add(path)
    sizes_by_image: dict[str, set[str]] = {}
    for variant in joined["image_variant"]:
        sizes_by_image.setdefault(string(variant["image_id"]), set()).add(
            string(variant["size_key"])
        )
    for identifier in bound:
        asset = assets[identifier]
        if (
            asset["availability"] == "available"
            and asset["publication_state"] == "approved"
        ):
            sizes = sizes_by_image.get(identifier, set())
            if sizes != {size.key for size in SIZES}:
                raise UploadError("Available public image requires all five sizes")


def _member(
    root: Path,
    key: str,
    files: dict[str, bytes | None],
    expected_images: dict[str, tuple[int, int, int]],
) -> Member:
    raw = files[key]
    is_image = key.startswith("images/")
    if is_image:
        raw = read_member(root, key)
        width, height, size = expected_images[key]
        if digest(raw)[7:] != Path(key).stem or len(raw) != size:
            raise UploadError("Public WebP hash or size mismatch")
        with Image.open(BytesIO(raw)) as decoded:
            if decoded.format != "WEBP" or decoded.size != (width, height):
                raise UploadError("Public WebP format or dimensions mismatch")
            decoded.load()
    assert raw is not None
    phase = (
        3
        if key == POINTER
        else 2
        if key.startswith("snapshots/manifests/")
        else 0
        if is_image
        else 1
    )
    content_type = (
        "image/webp"
        if is_image
        else "application/json"
        if key.endswith(".json")
        else "application/octet-stream"
    )
    return Member(key, len(raw), digest(raw), content_type, phase)


def _asset_paths(assets: dict[str, dict[str, JsonValue]]) -> None:
    for asset in assets.values():
        _check_paths({k: v for k, v in asset.items() if k != "source_src_raw"})
        # Root-relative img src is a web reference under its public source URL, not a local file.
        src = asset.get("source_src_raw")
        if isinstance(src, str) and src.startswith("/"):
            base = urlsplit(string(asset["source_url"]))
            if base.scheme != "https" or not base.netloc or base.username is not None:
                raise UploadError(
                    "Root-relative image source requires a public HTTPS base"
                )

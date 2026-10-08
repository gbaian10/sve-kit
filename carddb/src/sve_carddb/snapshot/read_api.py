"""Select and verify only the public closure of one export-offline preview root."""

import gzip
import re
import stat
import zlib
from dataclasses import dataclass
from io import BytesIO
from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError
from PIL import Image

from sve_carddb.contracts.profiles import MEDIA
from sve_carddb.contracts.snapshot import validate
from sve_carddb.core.compression import verify_brotli
from sve_carddb.core.json import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)
from sve_carddb.snapshot.media_urls import display_url
from sve_carddb.snapshot.publication import require_preview
from sve_carddb.snapshot.reader import read_snapshot

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.snapshot.reader import Row

POINTER = "snapshots/preview/current.json"
INDEX = "snapshots/versions/index.json"
JSON_KEY = re.compile(
    r"snapshots/(?:blobs|manifests)/[0-9a-f]{64}\.json(?:\.(?:br|gz))?\Z"
)
IMAGE_KEY = re.compile(
    r"images/(?:card_[sml]|art_[sm])/[1-9][0-9]*(?:-f[1-9][0-9]*)?\.webp\Z"
)
_ENTRY = (
    "data_version",
    "published_at",
    "format_version",
    "min_reader_version",
    "required_capabilities",
    "engine_support_target",
)


class ExportError(ValueError):
    """A redacted local contract or publication error safe for operator logs."""


def read_member(root: Path, key: str) -> bytes:
    """Reject symlinks and special files before reading a public member."""
    if key.startswith("/") or ".." in key.split("/"):
        raise ExportError("Invalid public member key")
    target = root / key
    if target.resolve() != target or not stat.S_ISREG(target.lstat().st_mode):
        raise ExportError("Public member is not a regular non-symlink file")
    return target.read_bytes()


@dataclass(frozen=True, repr=False)
class Member:
    key: str
    raw: bytes
    encoding: str | None = None


@dataclass(frozen=True)
class ImageFile:
    key: str
    url: str
    width: int
    height: int
    bytes: int


@dataclass(frozen=True, repr=False)
class Export:
    """One manifest's JSON bytes in memory; images are re-read and checked per use."""

    root: Path
    entry: dict[str, JsonValue]
    members: tuple[Member, ...]
    images: tuple[ImageFile, ...]

    def image(self, item: ImageFile) -> bytes:
        """Catch a stray file; the writer drops the pointer before overwriting images."""
        raw = read_member(self.root, item.key)
        if len(raw) != item.bytes:
            raise ExportError("Export image differs from its manifest metadata")
        try:
            with Image.open(BytesIO(raw)) as decoded:
                if decoded.format != "WEBP" or decoded.size != (
                    item.width,
                    item.height,
                ):
                    raise ExportError("Export image differs from its manifest metadata")
        except OSError:
            raise ExportError("Export image is not a readable WebP") from None
        return raw


def directory(root: Path) -> None:
    """Keep explicit input paths from following symlink aliases."""
    if not root.is_absolute() or root.resolve() != root or not root.is_dir():
        raise ExportError("Explicit existing non-symlink directory required")


def closure(manifest_path: str, manifest: dict[str, JsonValue]) -> set[str]:
    """Changes.from is an identifier, never a recursively retained manifest."""
    if (
        not JSON_KEY.fullmatch(manifest_path)
        or not manifest_path.startswith("snapshots/manifests/")
        or not manifest_path.endswith(".json")
    ):
        raise ExportError("Invalid public manifest path")
    keys = {manifest_path, manifest_path + ".gz"}
    for row in _descriptions(manifest):
        path = string(row["path"])
        keys.add(path)
        encodings = object_value(row["compressed_bytes"])
        for encoding, suffix in (("gzip", ".gz"), ("br", ".br")):
            if encodings[encoding] is not None:
                keys.add(path + suffix)
    return keys


def image_files(tables: dict[str, list[Row]]) -> tuple[ImageFile, ...]:
    """Permanent keys and versioned URLs of every displayable size, never a hash path."""
    prints = {string(r["id"]): r for r in tables["printing"]}
    faces = {string(r["id"]): r for r in tables["face"]}
    sizes = {
        (string(r["image_id"]), string(r["size_key"])): r
        for r in tables["image_variant"]
    }
    result: dict[str, ImageFile] = {}
    for row in tables["printing_image"]:
        for raw in array(row["variants"]):
            variant = object_value(raw)
            size = string(variant["size_key"])
            url = display_url(
                prints[string(row["printing_id"])],
                faces[string(row["face_id"])],
                row,
                size,
            )
            if url is None:
                raise ExportError("Unavailable media cannot list display variants")
            key = url.split("?", 1)[0]
            if key in result or not IMAGE_KEY.fullmatch(key):
                raise ExportError("Duplicate or invalid permanent image key")
            result[key] = ImageFile(
                key,
                url,
                integer(variant["width"]),
                integer(variant["height"]),
                integer(sizes[string(row["image_id"]), size]["bytes"]),
            )
    return tuple(result[k] for k in sorted(result))


def load_export(root: Path) -> Export:
    """Validate the pointer's whole public closure before credentials or HTTP."""
    directory(root)
    try:
        return _load(root)
    except ExportError:
        raise
    except ValueError, OSError, KeyError, TypeError, SchemaError:
        raise ExportError("Export validation failed") from None


def _load(root: Path) -> Export:
    path, raw, manifest = _manifest(root)
    members = list(
        _encoded(root, path, raw, gz=True, br=(root / (path + ".br")).exists())
    )
    blobs: dict[str, bytes] = {}
    for row in _descriptions(manifest):
        blobs[string(row["path"])], encoded = _blob(root, row)
        members += encoded
    payloads = {
        string(f["key"]): blobs[string(f["path"])]
        for f in map(object_value, array(manifest["files"]))
    }
    tables = read_snapshot(manifest, payloads)
    unique = {m.key: m for m in members}
    if set(unique) - {path + ".br"} != closure(path, manifest):
        raise ExportError("Export members differ from the manifest closure")
    export = Export(
        root,
        {k: manifest[k] for k in _ENTRY}
        | {"manifest_path": path, "manifest_sha256": digest(raw)},
        tuple(unique[k] for k in sorted(unique)),
        image_files(tables),
    )
    for item in export.images:
        export.image(item)
    return export


def _manifest(root: Path) -> tuple[str, bytes, dict[str, JsonValue]]:
    pointer = object_value(parse(read_member(root, POINTER)))
    if set(pointer) != {"manifest_path", "manifest_sha256"}:
        raise ExportError("Invalid preview pointer")
    path = string(pointer["manifest_path"])
    raw = read_member(root, path)
    manifest = object_value(parse(raw))
    hashed = digest(raw)
    if (
        hashed != pointer["manifest_sha256"]
        or path != "snapshots/manifests/" + hashed[7:] + ".json"
        or canonical(manifest) != raw
    ):
        raise ExportError("Manifest differs from the preview pointer")
    require_preview(manifest, regions=tuple(map(string, array(manifest["regions"]))))
    if manifest["format_version"] != MEDIA:
        raise ExportError("Upload requires snapshot format 2.0.0")
    return path, raw, manifest


def _blob(root: Path, row: dict[str, JsonValue]) -> tuple[bytes, tuple[Member, ...]]:
    path = string(row["path"])
    raw = read_member(root, path)
    if digest(raw) != row["sha256"] or len(raw) != row["bytes"]:
        raise ExportError("Export payload differs from its manifest")
    lengths = object_value(row["compressed_bytes"])
    encoded = _encoded(
        root, path, raw, gz=lengths["gzip"] is not None, br=lengths["br"] is not None
    )
    actual = {m.key.removeprefix(path): len(m.raw) for m in encoded}
    if (actual.get(".gz"), actual.get(".br")) != (lengths["gzip"], lengths["br"]):
        raise ExportError("Export encoded length differs from its manifest")
    return raw, encoded


def _descriptions(manifest: dict[str, JsonValue]) -> list[dict[str, JsonValue]]:
    rows = [object_value(x) for x in array(manifest["files"])]
    rows += [
        object_value(manifest[f])
        for f in ("text_all", "changes_ref")
        if manifest[f] is not None
    ]
    for row in rows:
        if (
            string(row["path"])
            != "snapshots/blobs/" + string(row["sha256"])[7:] + ".json"
        ):
            raise ExportError("Invalid immutable payload path")
    return rows


def _encoded(
    root: Path, path: str, raw: bytes, *, gz: bool, br: bool
) -> tuple[Member, ...]:
    result = [Member(path, raw)]
    if gz:
        encoded = read_member(root, path + ".gz")
        try:
            with gzip.GzipFile(fileobj=BytesIO(encoded)) as stream:
                if stream.read(len(raw) + 1) != raw:
                    raise ExportError("Export gzip sibling differs from raw bytes")
        except EOFError, OSError, zlib.error:
            raise ExportError("Export gzip sibling is not readable") from None
        result.append(Member(path + ".gz", encoded, "gzip"))
    if br:
        encoded = read_member(root, path + ".br")
        verify_brotli(encoded, raw)
        result.append(Member(path + ".br", encoded, "br"))
    return tuple(result)


def validate_index(value: dict[str, JsonValue]) -> None:
    """Validate the public current/previous version index shape."""
    validate("Index", value, MEDIA)


def read_index(raw: bytes) -> dict[str, JsonValue]:
    """Read canonical version index bytes independently of transport metadata."""
    try:
        value = object_value(parse(raw))
        validate_index(value)
    except ValueError, TypeError, KeyError, SchemaError:
        raise ExportError("Invalid public version index") from None
    if raw != canonical(value):
        raise ExportError("Version index is not canonical")
    return value


def retained_manifest(raw: bytes, entry: dict[str, JsonValue]) -> dict[str, JsonValue]:
    """Verify a retained manifest against its version index identity."""
    if digest(raw) != entry["manifest_sha256"]:
        raise ExportError("GC requires intact retained manifests")
    manifest = object_value(parse(raw))
    validate("Manifest", manifest, string(manifest["format_version"]))
    if raw != canonical(manifest) or any(
        manifest[k] != entry[k] for k in ("data_version", "published_at")
    ):
        raise ExportError("GC manifest differs from its index entry")
    return manifest


def current_image_keys(
    manifest: dict[str, JsonValue], payloads: dict[str, bytes]
) -> set[str]:
    """Images follow current only; callers supply verified retained payload bytes."""
    return {item.key for item in image_files(read_snapshot(manifest, payloads))}

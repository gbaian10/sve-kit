"""Select and verify only the public closure of one export-offline preview root."""

import gzip
import re
import zlib
from dataclasses import dataclass
from io import BytesIO
from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError
from PIL import Image

from sve_carddb.core.json import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)
from sve_carddb.r2_upload.boundary import UploadError, read_member
from sve_carddb.snapshot.export.compression import verify_brotli
from sve_carddb.snapshot.media import display_url
from sve_carddb.snapshot.preview import POINTER
from sve_carddb.snapshot.profiles import MEDIA
from sve_carddb.snapshot.publication import require_preview
from sve_carddb.snapshot.reader import read_snapshot

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.snapshot.project.source import Record

INDEX = "snapshots/versions/index.json"
JSON_KEY = re.compile(
    r"snapshots/(?:blobs|manifests)/[0-9a-f]{64}\.json(?:\.(?:br|gz))?\Z"
)
IMAGE_KEY = re.compile(
    r"images/(?:card_[sml]|art_[sm])/[1-9][0-9]*(?:-f[1-9][0-9]*)?\.webp\Z"
)
JSON_HEADERS = {
    "content-type": "application/json",
    "cache-control": "public,max-age=31536000,immutable",
}
IMAGE_HEADERS = {
    "content-type": "image/webp",
    "cache-control": "public,max-age=86400,must-revalidate",
}
INDEX_HEADERS = {"content-type": "application/json", "cache-control": "no-store"}
_ENTRY = (
    "data_version",
    "published_at",
    "format_version",
    "min_reader_version",
    "required_capabilities",
    "engine_support_target",
)


@dataclass(frozen=True, repr=False)
class Member:
    key: str
    raw: bytes
    headers: dict[str, str]


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
            raise UploadError("Export image differs from its manifest metadata")
        try:
            with Image.open(BytesIO(raw)) as decoded:
                if decoded.format != "WEBP" or decoded.size != (
                    item.width,
                    item.height,
                ):
                    raise UploadError("Export image differs from its manifest metadata")
        except OSError:
            raise UploadError("Export image is not a readable WebP") from None
        return raw


def directory(root: Path) -> None:
    """Keep explicit input paths from following symlink aliases."""
    if not root.is_absolute() or root.resolve() != root or not root.is_dir():
        raise UploadError("Explicit existing non-symlink directory required")


def closure(manifest_path: str, manifest: dict[str, JsonValue]) -> set[str]:
    """Changes.from is an identifier, never a recursively retained manifest."""
    if (
        not JSON_KEY.fullmatch(manifest_path)
        or not manifest_path.startswith("snapshots/manifests/")
        or not manifest_path.endswith(".json")
    ):
        raise UploadError("Invalid public manifest path")
    keys = {manifest_path, manifest_path + ".gz"}
    for row in _descriptions(manifest):
        path = string(row["path"])
        keys.add(path)
        encodings = object_value(row["compressed_bytes"])
        for encoding, suffix in (("gzip", ".gz"), ("br", ".br")):
            if encodings[encoding] is not None:
                keys.add(path + suffix)
    return keys


def image_files(tables: dict[str, list[Record]]) -> tuple[ImageFile, ...]:
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
                raise UploadError("Unavailable media cannot list display variants")
            key = url.split("?", 1)[0]
            if key in result or not IMAGE_KEY.fullmatch(key):
                raise UploadError("Duplicate or invalid permanent image key")
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
    except UploadError:
        raise
    except ValueError, OSError, KeyError, TypeError, SchemaError:
        raise UploadError("Export validation failed") from None


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
        raise UploadError("Export members differ from the manifest closure")
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
        raise UploadError("Invalid preview pointer")
    path = string(pointer["manifest_path"])
    raw = read_member(root, path)
    manifest = object_value(parse(raw))
    hashed = digest(raw)
    if (
        hashed != pointer["manifest_sha256"]
        or path != "snapshots/manifests/" + hashed[7:] + ".json"
        or canonical(manifest) != raw
    ):
        raise UploadError("Manifest differs from the preview pointer")
    require_preview(manifest, regions=tuple(map(string, array(manifest["regions"]))))
    if manifest["format_version"] != MEDIA:
        raise UploadError("Upload requires snapshot format 2.0.0")
    return path, raw, manifest


def _blob(root: Path, row: dict[str, JsonValue]) -> tuple[bytes, tuple[Member, ...]]:
    path = string(row["path"])
    raw = read_member(root, path)
    if digest(raw) != row["sha256"] or len(raw) != row["bytes"]:
        raise UploadError("Export payload differs from its manifest")
    lengths = object_value(row["compressed_bytes"])
    encoded = _encoded(
        root, path, raw, gz=lengths["gzip"] is not None, br=lengths["br"] is not None
    )
    actual = {m.key.removeprefix(path): len(m.raw) for m in encoded}
    if (actual.get(".gz"), actual.get(".br")) != (lengths["gzip"], lengths["br"]):
        raise UploadError("Export encoded length differs from its manifest")
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
            raise UploadError("Invalid immutable payload path")
    return rows


def _encoded(
    root: Path, path: str, raw: bytes, *, gz: bool, br: bool
) -> tuple[Member, ...]:
    result = [Member(path, raw, JSON_HEADERS)]
    if gz:
        encoded = read_member(root, path + ".gz")
        try:
            with gzip.GzipFile(fileobj=BytesIO(encoded)) as stream:
                if stream.read(len(raw) + 1) != raw:
                    raise UploadError("Export gzip sibling differs from raw bytes")
        except EOFError, OSError, zlib.error:
            raise UploadError("Export gzip sibling is not readable") from None
        result.append(
            Member(path + ".gz", encoded, JSON_HEADERS | {"content-encoding": "gzip"})
        )
    if br:
        encoded = read_member(root, path + ".br")
        verify_brotli(encoded, raw)
        result.append(
            Member(path + ".br", encoded, JSON_HEADERS | {"content-encoding": "br"})
        )
    return tuple(result)

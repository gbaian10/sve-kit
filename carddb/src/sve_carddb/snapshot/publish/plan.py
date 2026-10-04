"""Seal a complete formal 2.0 transport and its verified image plan before I/O."""

import gzip
import re
from copy import deepcopy
from dataclasses import dataclass
from typing import TYPE_CHECKING
from urllib.parse import urlsplit

from jsonschema import ValidationError as SchemaError
from pydantic import JsonValue

from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.export.compression import Blob, compress, verify_brotli
from sve_carddb.snapshot.media import display_url, image_path, prepare_media
from sve_carddb.snapshot.profiles import MEDIA
from sve_carddb.snapshot.project.source import json_list
from sve_carddb.snapshot.publication import require_formal
from sve_carddb.snapshot.publish.storage import PublishError
from sve_carddb.snapshot.reader import read_text_all
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    string,
)

if TYPE_CHECKING:
    from collections.abc import Iterator, Mapping
    from pathlib import Path

    from sve_carddb.snapshot.export import Brotli, Snapshot
    from sve_carddb.snapshot.media import MediaPlan
    from sve_carddb.snapshot.project import Projection

INDEX = "snapshots/versions/index.json"
JSON_KEY = re.compile(
    r"snapshots/(?:blobs|manifests)/[0-9a-f]{64}\.json(?:\.(?:br|gz))?\Z"
)
IMAGE_KEY = re.compile(
    r"images/(?:card_[sml]|art_[sm])/[1-9][0-9]*(?:-f[1-9][0-9]*)?\.webp\Z"
)
IMMUTABLE = "public,max-age=31536000,immutable"
IMAGE_CACHE = "public,max-age=86400,must-revalidate"


@dataclass(frozen=True, repr=False)
class Member:
    key: str
    raw: bytes
    headers: dict[str, str]

    def describe(self) -> dict[str, JsonValue]:
        """Only hashes and sizes enter the private long-term publication ledger."""
        return {
            "path": self.key,
            "sha256": digest(self.raw),
            "bytes": len(self.raw),
            "headers": dict(self.headers),
        }


def closure(manifest_path: str, manifest: dict[str, JsonValue]) -> set[str]:
    """Changes.from is an identifier, never a recursively retained manifest."""
    if (
        not JSON_KEY.fullmatch(manifest_path)
        or not manifest_path.startswith("snapshots/manifests/")
        or not manifest_path.endswith(".json")
    ):
        raise PublishError("Invalid public manifest path")
    keys = {manifest_path, manifest_path + ".gz"}
    descriptions = [object_value(x) for x in array(manifest["files"])]
    for field in ("text_all", "changes_ref"):
        if manifest[field] is not None:
            descriptions.extend([object_value(manifest[field])])
    for row in descriptions:
        path = string(row["path"])
        if path != "snapshots/blobs/" + string(row["sha256"])[7:] + ".json":
            raise PublishError("Invalid immutable payload path")
        keys.add(path)
        encodings = object_value(row["compressed_bytes"])
        for encoding, suffix in (("gzip", ".gz"), ("br", ".br")):
            if encodings[encoding] is not None:
                keys.add(path + suffix)
    return keys


def _members(
    path: str, raw: bytes, compressed: bytes, br: bytes | None
) -> list[Member]:
    if not JSON_KEY.fullmatch(path) or not path.endswith(".json"):
        raise PublishError("Invalid immutable member key")
    if compressed != gzip.compress(raw, compresslevel=9, mtime=0):
        raise PublishError("Invalid canonical gzip representation")
    if br is not None:
        verify_brotli(br, raw)
    if digest(raw)[7:] != path.rsplit("/", 1)[1][:-5]:
        raise PublishError("Immutable member content address mismatch")
    headers = {"content-type": "application/json", "cache-control": IMMUTABLE}
    result = [
        Member(path, raw, headers),
        Member(path + ".gz", compressed, headers | {"content-encoding": "gzip"}),
    ]
    if br is not None:
        result.append(Member(path + ".br", br, headers | {"content-encoding": "br"}))
    return result


@dataclass(frozen=True, repr=False)
class Release:
    """No CLI/credential path: operators must inject formal gates and storage."""

    snapshot: Snapshot
    media: MediaPlan
    source: Path | None
    members: tuple[Member, ...]
    entry: dict[str, JsonValue]
    assets: tuple[dict[str, JsonValue], ...]
    texts: dict[str, str]
    events: dict[str, str]
    changes: dict[str, JsonValue] | None
    cdn_root: str
    confirmed_images: frozenset[str]

    def images(self) -> Iterator[tuple[str, bytes]]:
        """Reverify source bytes on every retry rather than trusting file paths."""
        if self.source is not None:
            yield from self.media.blobs(self.source)
        elif self.assets:
            raise PublishError("Image source required for the sealed release")

    def describe(self) -> dict[str, JsonValue]:
        """Pin outputs and tokens without retaining full historic snapshots/text."""
        return {
            "entry": dict(self.entry),
            "media_state": dict(self.media.state),
            "members": [m.describe() for m in self.members],
            "assets": list(self.assets),
            "text_keys": dict(self.texts),
            "events": dict(self.events),
            "cdn_root": self.cdn_root,
            "confirmed_images": json_list(sorted(self.confirmed_images)),
        }


def prepare(  # ruff: ignore[too-many-arguments] -- compressor, CDN root and per-image evidence are independent explicit boundaries
    snapshot: Snapshot,
    media: MediaPlan,
    source: Path | None,
    *,
    cdn_root: str,
    brotli: Brotli | None = None,
    changes: bytes | None = None,
    confirmed_images: frozenset[str] = frozenset(),
    attachments: Mapping[str, Blob] | None = None,
) -> Release:
    """Validate a caller's formal artifact; never promote preview data implicitly."""
    try:
        return _prepare(
            snapshot,
            media,
            source,
            cdn_root=cdn_root,
            brotli=brotli,
            changes=changes,
            confirmed_images=confirmed_images,
            attachments=attachments,
        )
    except PublishError:
        raise
    except ValueError, TypeError, KeyError, OSError, SchemaError:
        version = snapshot.manifest.get("data_version")
        if isinstance(version, str) and version.startswith("preview-"):
            raise PublishError("Formal publish refuses preview artifacts") from None
        raise PublishError("Formal release validation failed") from None


def _prepare(  # ruff: ignore[too-many-arguments] -- explicit independent publication boundaries
    snapshot: Snapshot,
    media: MediaPlan,
    source: Path | None,
    *,
    cdn_root: str,
    brotli: Brotli | None = None,
    changes: bytes | None = None,
    confirmed_images: frozenset[str] = frozenset(),
    attachments: Mapping[str, Blob] | None = None,
) -> Release:
    """Validate a caller's formal artifact; never promote preview data implicitly."""
    require_formal(snapshot.manifest)
    if snapshot.manifest["format_version"] != MEDIA:
        raise PublishError("Publisher requires snapshot format 2.0.0")
    url = urlsplit(cdn_root)
    if url.query or url.fragment:
        raise PublishError("Explicit HTTPS CDN root required")
    if (
        url.scheme != "https"
        or not url.netloc
        or url.username
        or url.password
        or not cdn_root.endswith("/")
    ):
        raise PublishError("Explicit HTTPS CDN root required")
    snapshot.verify(media.projection)
    text_attachments = {
        key: blob.raw
        for key, blob in snapshot.payloads.items()
        if key == "programs" or key.startswith("images/")
    }
    if (
        read_text_all(snapshot.manifest, snapshot.text_all.raw, text_attachments)
        != media.projection.tables
    ):
        raise PublishError("Formal text union and shards differ")
    manifest = snapshot.manifest
    members: list[Member] = []
    for raw in array(manifest["files"]):
        file = object_value(raw)
        blob = snapshot.payloads[string(file["key"])]
        _blob(file, blob.raw, blob.gzip, blob.br)
        members.extend(_members(string(file["path"]), blob.raw, blob.gzip, blob.br))
    union = object_value(manifest["text_all"])
    _blob(
        union,
        snapshot.text_all.raw,
        snapshot.text_all.gzip,
        snapshot.text_all.br,
    )
    members.extend(
        _members(
            string(union["path"]),
            snapshot.text_all.raw,
            snapshot.text_all.gzip,
            snapshot.text_all.br,
        )
    )
    encoded = _manifest_blob(manifest, brotli, attachments)
    manifest_path = "snapshots/manifests/" + digest(encoded.raw)[7:] + ".json"
    change_value, change_members = _changes(manifest, changes, brotli, attachments)
    members.extend(change_members)
    members.extend(_members(manifest_path, encoded.raw, encoded.gzip, encoded.br))
    # Deduplicate shared raw/compressed blobs, refusing inconsistent metadata.
    unique: dict[str, Member] = {}
    for m in members:
        if m.key in unique and unique[m.key] != m:
            raise PublishError("Conflicting immutable members")
        unique[m.key] = m
    assets = _assets(media, source, cdn_root)
    texts = _text_keys(media)
    entry = {
        k: manifest[k]
        for k in (
            "data_version",
            "published_at",
            "format_version",
            "min_reader_version",
            "required_capabilities",
            "engine_support_target",
        )
    } | {"manifest_path": manifest_path, "manifest_sha256": digest(encoded.raw)}
    return Release(
        snapshot,
        media,
        source,
        tuple(unique[k] for k in sorted(unique)),
        entry,
        assets,
        texts,
        {
            string(r["id"]): string(r["data_version"])
            for r in media.projection.tables["identity_change"]
        },
        change_value,
        cdn_root,
        confirmed_images,
    )


def _blob(
    ref: dict[str, JsonValue],
    raw: bytes,
    gz: bytes,
    br: bytes | None,
) -> None:
    if digest(raw) != ref["sha256"] or len(raw) != ref["bytes"]:
        raise PublishError("Transport bytes differ from manifest")
    encodings = object_value(ref["compressed_bytes"])
    if encodings["gzip"] != len(gz) or encodings["br"] != (
        None if br is None else len(br)
    ):
        raise PublishError("Transport encoded lengths differ from manifest")


def _encoded(
    path: str, raw: bytes, brotli: Brotli | None, attachments: Mapping[str, Blob] | None
) -> Blob:
    if attachments is None:
        return compress(raw, brotli)
    blob = attachments[path]
    if blob.raw != raw:
        raise PublishError("Frozen attachment differs from canonical content")
    return blob


def _manifest_blob(
    manifest: dict[str, JsonValue],
    brotli: Brotli | None,
    attachments: Mapping[str, Blob] | None,
) -> Blob:
    raw = canonical(manifest)
    path = "snapshots/manifests/" + digest(raw)[7:] + ".json"
    expected = {path}
    if manifest["changes_ref"] is not None:
        expected.add(string(object_value(manifest["changes_ref"])["path"]))
    if attachments is not None and set(attachments) != expected:
        raise PublishError("Frozen attachments differ from the manifest closure")
    return _encoded(path, raw, brotli, attachments)


def release_attachments(release: Release) -> dict[str, Blob]:
    """Preserve frozen manifest/changes bytes when validating or resuming a release."""
    paths = {string(release.entry["manifest_path"])}
    ref = release.snapshot.manifest["changes_ref"]
    if ref is not None:
        paths.add(string(object_value(ref)["path"]))
    members = {member.key: member.raw for member in release.members}
    try:
        return {
            path: Blob(members[path], members.get(path + ".br"), members[path + ".gz"])
            for path in paths
        }
    except KeyError:
        raise PublishError("Frozen attachment is missing") from None


def _assets(
    media: MediaPlan, source: Path | None, cdn_root: str
) -> tuple[dict[str, JsonValue], ...]:
    prints = {string(r["id"]): r for r in media.projection.tables["printing"]}
    faces = {string(r["id"]): r for r in media.projection.tables["face"]}
    urls: dict[str, str] = {}
    for row in media.projection.tables["printing_image"]:
        for variant in array(row["variants"]):
            path = display_url(
                prints[string(row["printing_id"])],
                faces[string(row["face_id"])],
                row,
                string(object_value(variant)["size_key"]),
            )
            if path is None:
                raise PublishError("Display asset has no approved URL")
            urls[path.split("?", 1)[0]] = cdn_root + path
    assets = tuple(
        asset | {"url": urls[string(asset["path"])]} for asset in media.assets
    )
    if len(urls) != len(assets) or any(
        not IMAGE_KEY.fullmatch(string(a["path"])) for a in assets
    ):
        raise PublishError("Media asset and URL inventory disagree")
    # Read all verified sources once before any remote access.
    for _key, _raw in media.blobs(source) if source is not None else ():
        pass
    if assets and source is None:
        raise PublishError("Image source required for the sealed release")
    return assets


def _text_keys(media: MediaPlan) -> dict[str, str]:
    texts = {}
    for row in media.projection.tables["text_unit"]:
        full = digest(string(row["text"]).encode())
        identifier = "t:" + string(row["lang"]) + ":" + full[7:23]
        if row["id"] != identifier:
            raise PublishError("Public text ID differs from its exact content")
        texts[identifier] = full
    return texts


def verify_media(release: Release, previous: JsonValue) -> None:
    """Recompute all fingerprints and tokens from the ledger's committed basis.

    Frozen Python dataclasses still contain mutable dictionaries. Neither a
    caller-supplied state nor metadata alone authorizes a version or image.
    """
    restored: Projection = deepcopy(release.media.projection)
    prints = {string(r["id"]): r for r in restored.tables["printing"]}
    faces = {string(r["id"]): r for r in restored.tables["face"]}
    assets = {string(a["path"]): a for a in release.media.assets}
    sources: dict[tuple[str, str], str] = {}
    for row in restored.tables["printing_image"]:
        for variant in array(row["variants"]):
            size = string(object_value(variant)["size_key"])
            path = image_path(
                integer(prints[string(row["printing_id"])]["int_id"]),
                integer(faces[string(row["face_id"])]["ordinal"]),
                size,
            )
            key = (string(row["image_id"]), size)
            source = string(assets[path]["source"])
            if key in sources and sources[key] != source:
                raise PublishError("Shared media source disagrees")
            sources[key] = source
    for row in restored.tables["image_variant"]:
        row["path"] = sources[string(row["image_id"]), string(row["size_key"])]
    expected = prepare_media(
        restored,
        release.source,
        revision=integer(release.media.state["revision"]),
        previous=previous,
        confirmed_images=release.confirmed_images,
    )
    if expected != release.media:
        raise PublishError("Media plan differs from durable committed basis")


def _changes(
    manifest: dict[str, JsonValue],
    changes: bytes | None,
    brotli: Brotli | None,
    attachments: Mapping[str, Blob] | None,
) -> tuple[dict[str, JsonValue] | None, list[Member]]:
    members: list[Member] = []
    change_value = None
    if manifest["changes_ref"] is not None:
        if changes is None:
            raise PublishError("Formal changes attachment is missing")
        from sve_carddb.snapshot.values import parse  # ruff: ignore[import-outside-top-level] -- parse only the optional attachment

        change_value = object_value(parse(changes))
        validate("Changes", change_value, MEDIA)
        if (
            canonical(change_value) != changes
            or change_value["to_data_version"] != manifest["data_version"]
        ):
            raise PublishError("Changes are not canonical or target another release")
        ref = object_value(manifest["changes_ref"])
        encoded = _encoded(string(ref["path"]), changes, brotli, attachments)
        _blob(ref, encoded.raw, encoded.gzip, encoded.br)
        members.extend(
            _members(string(ref["path"]), encoded.raw, encoded.gzip, encoded.br)
        )
    elif changes is not None:
        raise PublishError("Unreferenced changes attachment")
    return change_value, members

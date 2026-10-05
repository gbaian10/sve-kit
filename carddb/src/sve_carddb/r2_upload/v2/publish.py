"""Upload one verified export: changed objects only, read back, CDN, index last."""

from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError

from sve_carddb.r2_upload.boundary import UploadError
from sve_carddb.r2_upload.v2.export import IMAGE_HEADERS, INDEX, INDEX_HEADERS
from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.profiles import MEDIA
from sve_carddb.snapshot.values import (
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.r2_upload.v2.adapter import R2Store, Stored
    from sve_carddb.r2_upload.v2.export import Export
    from sve_carddb.r2_upload.v2.freshness import CDNFreshness


def read_index(remote: Stored | None) -> dict[str, JsonValue] | None:
    """The remote index is the only record of what current and previous are."""
    if remote is None:
        return None
    if remote.headers != INDEX_HEADERS:
        raise UploadError("Version index metadata differs from contract")
    try:
        value = object_value(parse(remote.raw))
        validate("Index", value, MEDIA)
    except ValueError, TypeError, KeyError, SchemaError:
        raise UploadError("Invalid public version index") from None
    if remote.raw != canonical(value):
        raise UploadError("Version index is not canonical")
    return value


def next_index(
    index: dict[str, JsonValue] | None, entry: dict[str, JsonValue]
) -> dict[str, JsonValue] | None:
    """None means the export is already current; previous becomes the old current."""
    if index is None:
        result: dict[str, JsonValue] = {
            "index_format": 2,
            "revision": 1,
            "current": entry,
            "previous": None,
        }
    else:
        current = object_value(index["current"])
        if current == entry:
            return None
        if entry["data_version"] in {
            object_value(e)["data_version"]
            for e in (index["current"], index["previous"])
            if e is not None
        }:
            raise UploadError("Data version is already published with another manifest")
        if string(entry["published_at"]) < string(current["published_at"]):
            raise UploadError("Export is older than the remote current")
        result = {
            "index_format": 2,
            "revision": integer(index["revision"]) + 1,
            "current": entry,
            "previous": current,
        }
    validate("Index", result, MEDIA)
    return result


def upload(
    store: R2Store, export: Export, cdn: CDNFreshness | None
) -> dict[str, object]:
    """Index is written last, so a failed run leaves the old current; rerun resumes."""
    remote = store.get(INDEX)
    index = next_index(read_index(remote), export.entry)
    written = 0
    digests: dict[str, str] = {}
    for item in export.images:
        raw = export.image(item)
        digests[item.key] = digest(raw)
        written += _sync(store, item.key, raw, IMAGE_HEADERS, overwrite=True)
    for member in export.members:
        written += _sync(store, member.key, member.raw, member.headers, overwrite=False)
    if cdn is not None:
        _verify_cdn(cdn, export, digests)
    if index is not None:
        _commit(store, remote, index)
    final = read_index(store.get(INDEX))
    if final is None or object_value(final["current"]) != export.entry:
        raise UploadError("Version index read-back differs from this export")
    return {
        "mode": "execute",
        "written_files": written,
        "unchanged_files": len(export.images) + len(export.members) - written,
        "index": "unchanged" if index is None else "updated",
        "index_revision": final["revision"],
        "cdn_verification": "skipped" if cdn is None else "verified",
    }


def _verify_cdn(cdn: CDNFreshness, export: Export, digests: dict[str, str]) -> None:
    for item in export.images:
        # A second ordinary GET checks what the first one left in the cache.
        for _ in range(2):
            raw = cdn.get(cdn.root + item.url)
            if raw is None or digest(raw) != digests[item.key]:
                raise UploadError("CDN full-URL verification failed")


def _commit(store: R2Store, remote: Stored | None, index: dict[str, JsonValue]) -> None:
    if store.get(INDEX) != remote:
        raise UploadError("Version index changed during upload")
    if not store.put(
        INDEX,
        canonical(index),
        INDEX_HEADERS,
        expected=None if remote is None else remote.etag,
    ):
        raise UploadError("Version index conditional write failed")


def _sync(
    store: R2Store, key: str, raw: bytes, headers: dict[str, str], *, overwrite: bool
) -> int:
    """Return 1 after a verified write; an identical object counts as read back."""
    existing = store.get(key)
    if existing is not None and existing.raw == raw and existing.headers == headers:
        return 0
    if existing is not None and not overwrite:
        raise UploadError("Immutable JSON object differs from the export")
    if not store.put(
        key, raw, headers, expected=None if existing is None else existing.etag
    ):
        raise UploadError("Conditional PUT failed; the object changed concurrently")
    stored = store.get(key)
    if stored is None or stored.raw != raw or stored.headers != headers:
        raise UploadError("Origin read-back differs from the export")
    return 1


def report(export: Export) -> dict[str, object]:
    """Local counts only; remote existence is unknown without credentials."""
    return {
        "mode": "dry_run",
        "data_version": export.entry["data_version"],
        "manifest_sha256": export.entry["manifest_sha256"],
        "json_files": len(export.members),
        "json_bytes": sum(len(m.raw) for m in export.members),
        "image_files": len(export.images),
        "image_bytes": sum(i.bytes for i in export.images),
        "remote": "not_checked",
    }

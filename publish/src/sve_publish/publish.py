"""Upload one verified export: changed objects only, read back, CDN, index last."""

from typing import TYPE_CHECKING

from sve_carddb.export.read_api import (
    INDEX,
    ExportError,
    canonical,
    digest,
    integer,
    object_value,
    string,
    validate_index,
)
from sve_carddb.export.read_api import read_index as read_index_bytes

from sve_publish.headers import IMAGE_HEADERS, INDEX_HEADERS, member_headers

if TYPE_CHECKING:
    from pydantic import JsonValue
    from sve_carddb.export.read_api import Export

    from sve_publish.adapter import R2Store, Stored
    from sve_publish.freshness import CDNFreshness


def read_index(remote: Stored | None) -> dict[str, JsonValue] | None:
    """The remote index is the only record of what current and previous are."""
    if remote is None:
        return None
    if remote.headers != INDEX_HEADERS:
        raise ExportError("Version index metadata differs from contract")
    return read_index_bytes(remote.raw)


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
            raise ExportError("Data version is already published with another manifest")
        if _instant(entry["published_at"]) < _instant(current["published_at"]):
            raise ExportError("Export is older than the remote current")
        result = {
            "index_format": 2,
            "revision": integer(index["revision"]) + 1,
            "current": entry,
            "previous": current,
        }
    validate_index(result)
    return result


def _instant(value: JsonValue) -> tuple[str, str]:
    """Schema-validated UTC instants can have absent or differently padded fractions."""
    seconds, _, fraction = string(value).removesuffix("Z").partition(".")
    return seconds, fraction.rstrip("0")


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
        written += _sync(
            store, member.key, member.raw, member_headers(member), overwrite=False
        )
    if cdn is not None:
        _verify_cdn(cdn, export, digests)
    if index is not None:
        _commit(store, remote, index)
    final = read_index(store.get(INDEX))
    if final is None or object_value(final["current"]) != export.entry:
        raise ExportError("Version index read-back differs from this export")
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
                raise ExportError("CDN full-URL verification failed")


def _commit(store: R2Store, remote: Stored | None, index: dict[str, JsonValue]) -> None:
    if store.get(INDEX) != remote:
        raise ExportError("Version index changed during upload")
    if not store.put(
        INDEX,
        canonical(index),
        INDEX_HEADERS,
        expected=None if remote is None else remote.etag,
    ):
        raise ExportError("Version index conditional write failed")


def _sync(
    store: R2Store, key: str, raw: bytes, headers: dict[str, str], *, overwrite: bool
) -> int:
    """Return 1 after a verified write; an identical object counts as read back."""
    existing = store.get(key)
    if existing is not None and existing.raw == raw and existing.headers == headers:
        return 0
    if existing is not None and not overwrite:
        raise ExportError("Immutable JSON object differs from the export")
    if not store.put(
        key, raw, headers, expected=None if existing is None else existing.etag
    ):
        raise ExportError("Conditional PUT failed; the object changed concurrently")
    stored = store.get(key)
    if stored is None or stored.raw != raw or stored.headers != headers:
        raise ExportError("Origin read-back differs from the export")
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

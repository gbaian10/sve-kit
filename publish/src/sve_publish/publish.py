"""Upload one verified export: changed objects only, read back, CDN, index last."""

from functools import partial
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
from sve_publish.parallel import UploadObject, UploadPool

if TYPE_CHECKING:
    from collections.abc import Callable

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
    store: R2Store,
    export: Export,
    cdn: CDNFreshness | None,
    *,
    jobs: int = 16,
    progress: Callable[[str], None] | None = None,
) -> dict[str, object]:
    """Index is written last, so a failed run leaves the old current; rerun resumes."""
    if jobs < 1:
        raise ExportError("Upload jobs must be at least 1")
    remote = store.get(INDEX)
    index = next_index(read_index(remote), export.entry)
    pool = UploadPool(store, jobs, len(export.images) + len(export.members), progress)
    digests = pool.sync(
        UploadObject(item.key, partial(export.image, item), IMAGE_HEADERS, True)
        for item in export.images
    )
    pool.sync(
        UploadObject(
            member.key, partial(bytes, member.raw), member_headers(member), False
        )
        for member in export.members
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
        "written_files": pool.written,
        "unchanged_files": len(export.images) + len(export.members) - pool.written,
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

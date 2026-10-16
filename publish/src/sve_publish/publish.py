"""Upload the manifest difference of a verified export, committing its index last."""

from functools import partial
from typing import TYPE_CHECKING

from sve_carddb.export.read_api import (
    INDEX,
    ExportError,
    canonical,
    integer,
    object_value,
    string,
    validate_index,
)
from sve_carddb.export.read_api import read_index as read_index_bytes

from sve_publish.headers import INDEX_HEADERS, member_headers
from sve_publish.inventory import INVENTORY, require_images
from sve_publish.parallel import UploadObject, UploadPool
from sve_publish.snapshot_diff import missing_members

if TYPE_CHECKING:
    from collections.abc import Callable

    from pydantic import JsonValue
    from sve_carddb.export.read_api import Export

    from sve_publish.adapter import R2Store, Stored


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
    *,
    jobs: int = 16,
    progress: Callable[[str], None] | None = None,
) -> dict[str, object]:
    """Index is written last, so a failed run leaves the old current; rerun resumes."""
    if jobs < 1:
        raise ExportError("Upload jobs must be at least 1")
    remote = store.get(INDEX)
    before = read_index(remote)
    index = next_index(before, export.entry)
    images = store.get(INVENTORY)
    require_images(images, export.images)
    members = () if index is None else missing_members(store, before, export.members)
    pool = UploadPool(store, jobs, len(members), progress)
    pool.sync(
        UploadObject(member.key, partial(bytes, member.raw), member_headers(member))
        for member in members
    )
    if store.get(INVENTORY) != images:
        raise ExportError("Image inventory changed during snapshot upload")
    if index is not None:
        _commit(store, remote, index)
    final = read_index(store.get(INDEX))
    if final is None or object_value(final["current"]) != export.entry:
        raise ExportError("Version index read-back differs from this export")
    return {
        "mode": "execute",
        "written_files": pool.written,
        "unchanged_files": len(export.members) - pool.written,
        "index": "unchanged" if index is None else "updated",
        "index_revision": final["revision"],
        "cdn_verification": "not_checked",
    }


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

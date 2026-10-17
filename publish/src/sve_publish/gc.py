"""Collect public objects that the remote current/previous index no longer uses."""

from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError
from sve_carddb.export.read_api import (
    IMAGE_KEY,
    INDEX,
    JSON_KEY,
    ExportError,
    array,
    closure,
    current_image_keys,
    digest,
    object_value,
    retained_manifest,
    string,
)

from sve_publish.adapter import IMAGE_PREFIXES, MAX_DELETE_BATCH, SNAPSHOT_PREFIXES
from sve_publish.inventory import INVENTORY, commit_inventory, inventory, read_inventory
from sve_publish.publish import read_index

if TYPE_CHECKING:
    from collections.abc import Callable

    from pydantic import JsonValue

    from sve_publish.adapter import R2Store, Stored


def _manifest(store: R2Store, entry: dict[str, JsonValue]) -> dict[str, JsonValue]:
    path = string(entry["manifest_path"])
    remote = store.get(path)
    if remote is None or digest(remote.raw) != entry["manifest_sha256"]:
        raise ExportError("GC requires intact retained manifests")
    return retained_manifest(remote.raw, entry)


def _current_images(store: R2Store, manifest: dict[str, JsonValue]) -> set[str]:
    """Images follow current only; previous metadata never retains old bytes."""
    payloads: dict[str, bytes] = {}
    for raw in array(manifest["files"]):
        row = object_value(raw)
        remote = store.get(string(row["path"]))
        if remote is None or digest(remote.raw) != row["sha256"]:
            raise ExportError("GC requires intact retained payloads")
        payloads[string(row["key"])] = remote.raw
    return current_image_keys(manifest, payloads)


def retained(store: R2Store) -> tuple[Stored, set[str], set[str]]:
    """Required JSON closures of both versions and current images; optional manifest .br."""
    remote = store.get(INDEX)
    index = read_index(remote)
    if remote is None or index is None:
        raise ExportError("GC requires a published version index")
    required: set[str] = set()
    optional: set[str] = set()
    for name in ("current", "previous"):
        if index[name] is None:
            continue
        entry = object_value(index[name])
        manifest = _manifest(store, entry)
        path = string(entry["manifest_path"])
        required |= closure(path, manifest)
        optional.add(path + ".br")
    return remote, required, optional


def _delete_batches(
    store: R2Store,
    remote: Stored,
    candidates: list[str],
    progress: Callable[[str], None] | None,
) -> list[str]:
    deleted: list[str] = []
    for start in range(0, len(candidates), MAX_DELETE_BATCH):
        batch = candidates[start : start + MAX_DELETE_BATCH]
        if store.get(INDEX) != remote:
            raise ExportError("GC version index changed; run it again")
        store.delete_many(batch)
        deleted.extend(batch)
        if progress is not None:
            progress(f"GC: {len(deleted)}/{len(candidates)} deleted")
    return deleted


def collect(
    store: R2Store,
    namespaces: frozenset[str],
    *,
    execute: bool,
    progress: Callable[[str], None] | None = None,
) -> dict[str, object]:
    """Recheck the index before each batch; never run during an upload."""
    if not namespaces or not namespaces <= SNAPSHOT_PREFIXES:
        raise ExportError("GC namespace is not explicitly public")
    try:
        remote, required, optional = retained(store)
    except ExportError:
        raise
    except ValueError, TypeError, KeyError, SchemaError:
        raise ExportError("GC retained snapshot is invalid") from None
    listed: set[str] = set()
    for prefix in SNAPSHOT_PREFIXES:
        listed.update(store.keys(prefix))
    if any(not JSON_KEY.fullmatch(key) for key in listed):
        raise ExportError("GC inventory contains an irregular public key")
    if required - listed:
        raise ExportError("GC retained closure is incomplete")
    candidates = sorted(
        k
        for k in listed - required - optional
        if any(k.startswith(p) for p in namespaces)
    )
    if progress is not None:
        progress(f"GC: {len(candidates)} candidates")
    deleted = _delete_batches(store, remote, candidates, progress) if execute else []
    return {
        "mode": "execute" if execute else "dry_run",
        "candidates": candidates,
        "deleted": deleted,
    }


def collect_images(  # ruff: ignore[complex-structure] -- retention checks and invalidation must all precede the first DELETE
    store: R2Store,
    namespaces: frozenset[str],
    *,
    execute: bool,
    progress: Callable[[str], None] | None = None,
) -> dict[str, object]:
    """Invalidate claims before deleting; LIST also finds interrupted upload orphans."""
    if not namespaces or not namespaces <= IMAGE_PREFIXES:
        raise ExportError("GC namespace is not explicitly public")
    remote = store.get(INDEX)
    index = read_index(remote)
    if remote is None or index is None:
        raise ExportError("GC requires a published version index")
    try:
        required = _current_images(
            store, _manifest(store, object_value(index["current"]))
        )
    except ExportError:
        raise
    except ValueError, TypeError, KeyError, SchemaError:
        raise ExportError("GC retained snapshot is invalid") from None
    before = store.get(INVENTORY)
    rows = read_inventory(before)
    listed = {key for prefix in IMAGE_PREFIXES for key in store.keys(prefix)}
    if any(not IMAGE_KEY.fullmatch(key) for key in listed):
        raise ExportError("GC inventory contains an irregular public key")
    if required - listed or required - {item.key for item in rows.objects}:
        raise ExportError("GC retained images are incomplete; restore images first")
    candidates = sorted(
        key for key in listed - required if any(key.startswith(p) for p in namespaces)
    )
    kept = tuple(
        item
        for item in rows.objects
        if item.key in required or not any(item.key.startswith(p) for p in namespaces)
    )
    if progress is not None:
        progress(f"GC: {len(candidates)} candidates")
    deleted = []
    if execute:
        if store.get(INDEX) != remote:
            raise ExportError("GC version index changed; run it again")
        if kept != rows.objects:
            commit_inventory(store, before, inventory(kept))
        deleted = _delete_batches(store, remote, candidates, progress)
    return {
        "mode": "execute" if execute else "dry_run",
        "candidates": candidates,
        "deleted": deleted,
    }

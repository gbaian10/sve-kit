"""Collect public objects that the remote current/previous index no longer uses."""

from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError

from sve_carddb.core.json import array, digest, object_value, string
from sve_carddb.export.read_api import (
    IMAGE_KEY,
    INDEX,
    JSON_KEY,
    closure,
    current_image_keys,
    retained_manifest,
)
from sve_carddb.export.read_api import ExportError as UploadError
from sve_carddb.r2_upload.v2.adapter import PUBLIC_PREFIXES
from sve_carddb.r2_upload.v2.publish import read_index

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.r2_upload.v2.adapter import R2Store, Stored


def _manifest(store: R2Store, entry: dict[str, JsonValue]) -> dict[str, JsonValue]:
    path = string(entry["manifest_path"])
    remote = store.get(path)
    if remote is None or digest(remote.raw) != entry["manifest_sha256"]:
        raise UploadError("GC requires intact retained manifests")
    return retained_manifest(remote.raw, entry)


def _current_images(store: R2Store, manifest: dict[str, JsonValue]) -> set[str]:
    """Images follow current only; previous metadata never retains old bytes."""
    payloads: dict[str, bytes] = {}
    for raw in array(manifest["files"]):
        row = object_value(raw)
        remote = store.get(string(row["path"]))
        if remote is None or digest(remote.raw) != row["sha256"]:
            raise UploadError("GC requires intact retained payloads")
        payloads[string(row["key"])] = remote.raw
    return current_image_keys(manifest, payloads)


def retained(store: R2Store) -> tuple[Stored, set[str], set[str]]:
    """Required JSON closures of both versions and current images; optional manifest .br."""
    remote = store.get(INDEX)
    index = read_index(remote)
    if remote is None or index is None:
        raise UploadError("GC requires a published version index")
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
        if name == "current":
            required |= _current_images(store, manifest)
    return remote, required, optional


def collect(
    store: R2Store, namespaces: frozenset[str], *, execute: bool
) -> dict[str, object]:
    """Recheck the index before each DELETE; never run while an upload is running."""
    if not namespaces or not namespaces <= PUBLIC_PREFIXES:
        raise UploadError("GC namespace is not explicitly public")
    try:
        remote, required, optional = retained(store)
    except UploadError:
        raise
    except ValueError, TypeError, KeyError, SchemaError:
        raise UploadError("GC retained snapshot is invalid") from None
    listed: set[str] = set()
    for prefix in PUBLIC_PREFIXES:
        listed.update(store.keys(prefix))
    if any(not (JSON_KEY.fullmatch(key) or IMAGE_KEY.fullmatch(key)) for key in listed):
        raise UploadError("GC inventory contains an irregular public key")
    if required - listed:
        raise UploadError("GC retained closure is incomplete")
    candidates = sorted(
        k
        for k in listed - required - optional
        if any(k.startswith(p) for p in namespaces)
    )
    deleted: list[str] = []
    if execute:
        for key in candidates:
            if store.get(INDEX) != remote:
                raise UploadError("GC version index changed; run it again")
            store.delete(key)
            deleted.append(key)
    return {
        "mode": "execute" if execute else "dry_run",
        "candidates": candidates,
        "deleted": deleted,
    }

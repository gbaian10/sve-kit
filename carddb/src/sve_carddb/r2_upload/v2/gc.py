"""Per-run approved GC under the same writer lease, with index rechecks."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.r2_upload.v2.adapter import PUBLIC_PREFIXES
from sve_carddb.snapshot.project.source import json_list
from sve_carddb.snapshot.publish import _index, _keep, _receipt_index
from sve_carddb.snapshot.publish.plan import IMAGE_KEY, INDEX, JSON_KEY
from sve_carddb.snapshot.publish.storage import PublishError
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    string,
)

if TYPE_CHECKING:
    from sve_carddb.r2_upload.v2.adapter import R2Store
    from sve_carddb.snapshot.publish import Ledger
    from sve_carddb.snapshot.publish.storage import Stored

PREVIEW_POINTER = "snapshots/preview/current.json"


def _refuse_preview(store: R2Store) -> None:
    """Legacy previews share public blob keys but do not join this writer lease."""
    if store.get(PREVIEW_POINTER) is not None:
        raise PublishError("GC refuses a bucket with a preview pointer")


def confirmation(plan: dict[str, JsonValue]) -> str:
    """Bind the maintainer's contemporary consent to the exact list and index."""
    return "DELETE-V2 " + digest(canonical(plan))


def _retained(ledger: Ledger, store: R2Store, index: dict[str, JsonValue]) -> set[str]:
    keep = _keep(store, index)
    versions = {
        object_value(index[k])["data_version"]
        for k in ("current", "previous")
        if index[k] is not None
    }
    for raw in array(ledger.read()["attempts"]):
        item = object_value(raw)
        if item["plan"] is None:
            continue
        plan = object_value(item["plan"])
        if (
            item["status"] == "committed"
            and object_value(plan["entry"])["data_version"] in versions
        ) or item["status"] in {"sealed", "failed"}:
            keep.update(string(object_value(a)["path"]) for a in array(plan["assets"]))
        if item["status"] in {"sealed", "failed"}:
            keep.update(string(object_value(m)["path"]) for m in array(plan["members"]))
    if any(store.get(key) is None for key in keep):
        raise PublishError("GC retained closure is incomplete")
    return keep


def _plan(
    ledger: Ledger, store: R2Store, namespaces: frozenset[str]
) -> tuple[dict[str, JsonValue], Stored]:
    if not namespaces or not namespaces <= PUBLIC_PREFIXES:
        raise PublishError("GC namespace is not explicitly public")
    ledger.verify_backup()
    _refuse_preview(store)
    remote = store.get(INDEX)
    index = _index(remote)
    if remote is None or index is None or index != _receipt_index(ledger.read()):
        raise PublishError("GC index differs from durable committed receipts")
    keep = _retained(ledger, store, index)
    objects: list[JsonValue] = []
    for prefix in sorted(namespaces):
        for key in store.keys(prefix):
            if not key.startswith(prefix) or not (
                JSON_KEY.fullmatch(key) or IMAGE_KEY.fullmatch(key)
            ):
                raise PublishError("GC inventory contains an irregular public key")
            if key in keep:
                continue
            value = store.get(key)
            if value is None:
                raise PublishError("GC candidate disappeared during inventory")
            objects.append(
                {
                    "key": key,
                    "bytes": len(value.raw),
                    "sha256": digest(value.raw),
                    "etag": value.etag,
                }
            )
    _unchanged(store, remote)
    return {
        "format": 1,
        "revision": index["revision"],
        "index_sha256": digest(remote.raw),
        "index_etag": remote.etag,
        "namespaces": json_list(sorted(namespaces)),
        "objects": objects,
    }, remote


def _unchanged(store: R2Store, approved: Stored) -> None:
    store.verify_lease()
    _refuse_preview(store)
    if store.get(INDEX) != approved:
        raise PublishError(
            "GC current/previous index changed; discard the approved list"
        )


def inspect(
    ledger: Ledger, store: R2Store, *, namespaces: frozenset[str]
) -> dict[str, JsonValue]:
    """Inspect public data without deletion, using coordination-only lease writes."""
    with ledger.exclusive(), store.exclusive():
        plan, _remote = _plan(ledger, store, namespaces)
        return plan


def validate(plan: dict[str, JsonValue]) -> None:
    """A local list is inert until a matching consent and remote replan succeed."""
    if (
        set(plan)
        != {"format", "revision", "index_sha256", "index_etag", "namespaces", "objects"}
        or type(plan["format"]) is not int
        or integer(plan["revision"]) <= 0
    ):
        raise PublishError("Invalid GC approval list")
    if plan["format"] != 1:
        raise PublishError("Invalid GC approval list")
    namespaces = frozenset(string(p) for p in array(plan["namespaces"]))
    if not namespaces or not namespaces <= PUBLIC_PREFIXES:
        raise PublishError("GC namespace is not explicitly public")
    seen = set()
    for raw in array(plan["objects"]):
        row = object_value(raw)
        key = string(row["key"])
        if (
            set(row) != {"key", "bytes", "sha256", "etag"}
            or key in seen
            or not any(key.startswith(p) for p in namespaces)
            or not (JSON_KEY.fullmatch(key) or IMAGE_KEY.fullmatch(key))
        ):
            raise PublishError("Invalid or irregular GC candidate")
        if integer(row["bytes"]) < 0 or not string(row["etag"]):
            raise PublishError("Invalid or irregular GC candidate")
        seen.add(key)


def execute(
    ledger: Ledger, store: R2Store, plan: dict[str, JsonValue], *, confirmed: str
) -> tuple[str, ...]:
    """Replan and re-read current/previous immediately before each DELETE."""
    validate(plan)
    if confirmed != confirmation(plan):
        raise PublishError("GC requires the exact contemporary confirmation string")
    namespaces = frozenset(string(p) for p in array(plan["namespaces"]))
    with ledger.exclusive(), store.exclusive():
        current, approved_index = _plan(ledger, store, namespaces)
        if current != plan:
            raise PublishError(
                "GC inventory or current/previous changed; approve a new list"
            )
        deleted = []
        for raw in array(plan["objects"]):
            row = object_value(raw)
            key = string(row["key"])
            _unchanged(store, approved_index)
            store.delete_approved(key, index=approved_index, candidate=row)
            deleted.append(key)
        return tuple(deleted)


def report(plan: dict[str, JsonValue]) -> dict[str, object]:
    """List keys/counts/bytes only, without fetching official content in dry-run."""
    validate(plan)
    rows = [object_value(v) for v in array(plan["objects"])]
    return {
        "mode": "offline_dry_run",
        "candidate_files": len(rows),
        "candidate_bytes": sum(integer(r["bytes"]) for r in rows),
        "would_collect": [string(r["key"]) for r in rows],
        "confirmation": confirmation(plan),
        "remote_existence": "not_checked",
    }

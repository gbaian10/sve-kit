"""Offline-tested 2.0 publication orchestration; no live upload entry point.

Formal approval gates (#34), an operator-authorized distributed writer lease,
real CDN/header regression and separate purge permission remain prerequisites
for a future live adapter. A green synthetic test is not release authorization.
"""

from typing import TYPE_CHECKING

from jsonschema import ValidationError as SchemaError
from pydantic import JsonValue

from sve_carddb.snapshot.contract import validate
from sve_carddb.snapshot.publish.plan import (
    IMAGE_CACHE,
    IMAGE_KEY,
    INDEX,
    JSON_KEY,
    Release,
    closure,
    release_attachments,
    verify_media,
)
from sve_carddb.snapshot.publish.state import (
    Ledger,
    attempt,
    media_basis,
    merge_text_keys,
    state_hash,
)
from sve_carddb.snapshot.publish.storage import PublishError
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

if TYPE_CHECKING:
    from sve_carddb.snapshot.publish.storage import Freshness, ObjectStore, Stored

__all__ = ["Ledger", "PublishError", "Release", "collect", "publish"]


def _index(remote: Stored | None) -> dict[str, JsonValue] | None:
    if remote is None:
        return None
    if not remote.etag or remote.headers.get("cache-control") != "no-store":
        raise PublishError("Version index requires ETag and no-store")
    try:
        value = object_value(parse(remote.raw))
        validate("Index", value, "2.0.0")
    except ValueError, TypeError, KeyError, SchemaError:
        raise PublishError("Invalid public version index") from None
    if remote.raw != canonical(value):
        raise PublishError("Version index is not canonical")
    return value


def _receipt_index(state: dict[str, JsonValue]) -> JsonValue:
    result: JsonValue = None
    for raw in array(state["attempts"]):
        item = object_value(raw)
        if item["status"] == "committed":
            result = object_value(item["receipt"])["index"]
    return result


def _known_images(state: dict[str, JsonValue]) -> dict[str, set[str]]:
    known: dict[str, set[str]] = {}
    for raw in array(state["attempts"]):
        item = object_value(raw)
        if item["plan"] is not None:
            for asset in array(object_value(item["plan"])["assets"]):
                value = object_value(asset)
                known.setdefault(string(value["path"]), set()).add(
                    string(value["sha256"])
                )
    return known


def _reserved_attempt(
    state: dict[str, JsonValue], revision: int, data_version: str
) -> dict[str, JsonValue]:
    """Only a reserved or resumable attempt can enter the writer boundary."""
    item = attempt(state, revision)
    if item["status"] == "superseded":
        raise PublishError("Superseded release cannot be resumed")
    if item["data_version"] != data_version:
        raise PublishError("Release differs from its reserved data version")
    for raw in array(state["attempts"]):
        other = object_value(raw)
        if other["revision"] != revision and other["status"] in {"sealed", "failed"}:
            raise PublishError(
                "Unfinished release requires forward recovery or supersession"
            )
    return item


def _seal(ledger: Ledger, store: ObjectStore, release: Release) -> dict[str, JsonValue]:
    state = ledger.read()
    ledger.verify_backup()
    revision = integer(release.media.state["revision"])
    item = _reserved_attempt(state, revision, string(release.entry["data_version"]))
    described = release.describe()
    if item["plan"] is not None:
        prior = object_value(item["plan"])
        if any(prior[k] != value for k, value in described.items()):
            raise PublishError("Retry changes reserved release outputs")
        return item
    verify_media(release, media_basis(state))
    prior_remote = store.get(INDEX)
    prior_index = _index(prior_remote)
    if prior_index != _receipt_index(state):
        raise PublishError("Public index differs from durable publication receipts")
    if prior_index is not None and revision <= integer(prior_index["revision"]):
        raise PublishError("Release revision does not advance current")
    _adjacent_changes(release, prior_index)
    conditions = _image_conditions(state, store, release)
    index: dict[str, JsonValue] = {
        "index_format": 2,
        "revision": revision,
        "current": dict(release.entry),
        "previous": None if prior_index is None else prior_index["current"],
    }
    validate("Index", index, "2.0.0")
    events = object_value(state["first_events"])
    for event, version in release.events.items():
        if events.get(event, release.entry["data_version"]) != version:
            raise PublishError("Unproven or changed event first-publication version")
    merge_text_keys(state, release.texts)
    item["plan"] = described | {
        "prior_etag": None if prior_remote is None else prior_remote.etag,
        "prior_index": prior_index,
        "index": index,
        "image_conditions": conditions,
    }
    item["status"] = "sealed"
    ledger.save(state)
    return item


def _write_image(
    store: ObjectStore,
    plan: dict[str, JsonValue],
    asset: dict[str, JsonValue],
    raw: bytes,
) -> None:
    key = string(asset["path"])
    if digest(raw) != asset["sha256"] or len(raw) != asset["bytes"]:
        raise PublishError("Sealed image source changed")
    existing = store.get(key)
    headers = {"content-type": "image/webp", "cache-control": IMAGE_CACHE}
    if existing is not None and digest(existing.raw) == asset["sha256"]:
        if existing.headers != headers:
            raise PublishError("Origin image metadata differs from contract")
        return
    condition = object_value(object_value(plan["image_conditions"])[key])
    if (None if existing is None else existing.etag) != condition["etag"] or (
        None if existing is None else digest(existing.raw)
    ) != condition["sha256"]:
        raise PublishError("Origin image changed concurrently")
    expected = None if existing is None else existing.etag
    if not store.put(key, raw, headers, expected=expected):
        raise PublishError("Origin image conditional overwrite failed")


def _verify_images(store: ObjectStore, release: Release, freshness: Freshness) -> None:
    for asset in release.assets:
        origin = store.get(string(asset["path"]))
        if (
            origin is None
            or len(origin.raw) != asset["bytes"]
            or digest(origin.raw) != asset["sha256"]
            or origin.headers
            != {"content-type": "image/webp", "cache-control": IMAGE_CACHE}
        ):
            raise PublishError("Origin image verification failed")
        # Repeat ordinary GET to catch poisoned warm/negative query cache entries.
        for _ in range(2):
            raw = freshness.get(string(asset["url"]))
            if (
                raw is None
                or len(raw) != asset["bytes"]
                or digest(raw) != asset["sha256"]
            ):
                raise PublishError("CDN full-URL freshness verification failed")


def _immutable(store: ObjectStore, release: Release) -> None:
    for member in release.members:
        existing = store.get(member.key)
        if existing is None:
            if not store.put(member.key, member.raw, member.headers, expected=None):
                existing = store.get(member.key)
            else:
                existing = store.get(member.key)
        if (
            existing is None
            or existing.raw != member.raw
            or existing.headers != member.headers
        ):
            raise PublishError("Immutable JSON member differs from sealed release")


def _unchanged_index(store: ObjectStore, plan: dict[str, JsonValue]) -> None:
    remote = store.get(INDEX)
    if (None if remote is None else remote.etag) != plan["prior_etag"] or _index(
        remote
    ) != plan["prior_index"]:
        raise PublishError("Version index changed concurrently")


def _finish(
    ledger: Ledger, release: Release, plan: dict[str, JsonValue]
) -> dict[str, JsonValue]:
    state = ledger.read()
    item = attempt(state, integer(release.media.state["revision"]))
    item["status"] = "committed"
    item["receipt"] = {
        "index": plan["index"],
        "plan_sha256": digest(canonical(plan)),
        "manifest_sha256": release.entry["manifest_sha256"],
        "events": dict(release.events),
    }
    object_value(state["first_events"]).update(release.events)
    ledger.save(state)
    return {
        "revision": item["revision"],
        "data_version": item["data_version"],
        "manifest_sha256": release.entry["manifest_sha256"],
        "state_sha256": state_hash(ledger),
        "status": "committed",
    }


def publish(
    ledger: Ledger,
    store: ObjectStore,
    release: Release,
    freshness: Freshness,
) -> dict[str, JsonValue]:
    """Resume pinned writes, verify full URL bytes, and CAS the index last.

    A failed/uncertain index response is reconciled by exact index read-back on
    retry; its prepared ledger plan is retained until the receipt is finalized.
    """
    from sve_carddb.snapshot.publish.plan import prepare  # ruff: ignore[import-outside-top-level] -- revalidate before acquiring output access

    if (
        prepare(
            release.snapshot,
            release.media,
            release.source,
            cdn_root=release.cdn_root,
            attachments=release_attachments(release),
            changes=None if release.changes is None else canonical(release.changes),
            confirmed_images=release.confirmed_images,
        )
        != release
    ):
        raise PublishError("Sealed local release changed")
    with ledger.exclusive(), store.exclusive():
        item = _seal(ledger, store, release)
        plan = object_value(item["plan"])
        revision = integer(release.media.state["revision"])
        current = _index(store.get(INDEX))
        if current == plan["index"]:
            _verify_images(store, release, freshness)
            _immutable(store, release)
            _verify_current(store, plan)
            return _finish(ledger, release, plan)
        if item["status"] == "committed":
            raise PublishError("Committed release cannot replace a newer current")
        _unchanged_index(store, plan)
        try:
            ledger.verify_backup()
            assets = {string(a["path"]): a for a in release.assets}
            for key, raw in release.images():
                _write_image(store, plan, assets[key], raw)
            _verify_images(store, release, freshness)
            _immutable(store, release)
            _verify_images(store, release, freshness)
            _unchanged_index(store, plan)
            ledger.verify_backup()
            _commit_index(store, plan)
        except OSError, PublishError:
            state = ledger.read()
            attempt(state, revision)["status"] = "failed"
            ledger.save(state)
            raise
        return _finish(ledger, release, plan)


def _keep(store: ObjectStore, index: dict[str, JsonValue]) -> set[str]:
    keep = {INDEX}
    for name in ("current", "previous"):
        if index[name] is None:
            continue
        entry = object_value(index[name])
        key = string(entry["manifest_path"])
        remote = store.get(key)
        if remote is None or digest(remote.raw) != entry["manifest_sha256"]:
            raise PublishError("GC requires intact retained manifests")
        manifest = object_value(parse(remote.raw))
        validate("Manifest", manifest, string(manifest["format_version"]))
        if manifest["data_version"] != entry["data_version"] or remote.raw != canonical(
            manifest
        ):
            raise PublishError("GC manifest differs from retained index entry")
        keep |= closure(key, manifest)
        if store.get(key + ".br") is not None:
            keep.add(key + ".br")
    return keep


def collect(
    ledger: Ledger, store: ObjectStore, *, revision: int, namespaces: frozenset[str]
) -> tuple[str, ...]:
    """Only explicit public namespaces; shared lease and revision guard every delete."""
    allowed = {
        "snapshots/blobs/",
        "snapshots/manifests/",
        "images/card_s/",
        "images/card_m/",
        "images/card_l/",
        "images/art_s/",
        "images/art_m/",
    }
    if not namespaces or not namespaces <= allowed:
        raise PublishError("GC namespace is not explicitly public")
    with ledger.exclusive(), store.exclusive():
        ledger.verify_backup()
        state = ledger.read()
        remote = store.get(INDEX)
        index = _index(remote)
        if (
            index is None
            or integer(revision) != index["revision"]
            or index != _receipt_index(state)
        ):
            raise PublishError("GC revision differs from committed current")
        assert remote is not None
        keep = _keep(store, index)
        current = object_value(index["current"])
        for raw in array(state["attempts"]):
            item = object_value(raw)
            if item["plan"] is None:
                continue
            plan = object_value(item["plan"])
            if (
                item["status"] == "committed"
                and object_value(plan["entry"])["data_version"]
                == current["data_version"]
            ) or item["status"] in {"sealed", "failed"}:
                keep |= {string(object_value(a)["path"]) for a in array(plan["assets"])}
            if item["status"] in {"sealed", "failed"}:
                keep |= {
                    string(object_value(m)["path"]) for m in array(plan["members"])
                }
        if any(store.get(key) is None for key in keep):
            raise PublishError("GC retained closure is incomplete")
        return _delete_candidates(store, remote, namespaces, keep)


def _image_conditions(
    state: dict[str, JsonValue], store: ObjectStore, release: Release
) -> dict[str, JsonValue]:
    known = _known_images(state)
    conditions: dict[str, JsonValue] = {}
    for asset in release.assets:
        key = string(asset["path"])
        existing = store.get(key)
        if existing is not None and digest(existing.raw) not in known.get(
            key, set()
        ) | {string(asset["sha256"])}:
            raise PublishError("Unknown origin image bytes; manual recovery required")
        if existing is not None and not existing.etag:
            raise PublishError("Origin image has no ETag")
        conditions[key] = {
            "etag": None if existing is None else existing.etag,
            "sha256": None if existing is None else digest(existing.raw),
        }
    return conditions


def _adjacent_changes(
    release: Release, prior_index: dict[str, JsonValue] | None
) -> None:
    if release.changes is None:
        if prior_index is not None:
            raise PublishError("Formal continuation requires adjacent changes")
    elif release.changes["from_data_version"] != (
        None
        if prior_index is None
        else object_value(prior_index["current"])["data_version"]
    ):
        raise PublishError("Changes do not describe the direct previous release")


def _commit_index(store: ObjectStore, plan: dict[str, JsonValue]) -> None:
    if not store.put(
        INDEX,
        canonical(plan["index"]),
        {"content-type": "application/json", "cache-control": "no-store"},
        expected=None if plan["prior_etag"] is None else string(plan["prior_etag"]),
    ):
        raise PublishError("Version index CAS failed")
    _verify_current(store, plan)


def _verify_current(store: ObjectStore, plan: dict[str, JsonValue]) -> None:
    if _index(store.get(INDEX)) != plan["index"]:
        raise PublishError("Version index commit is uncertain; recover from plan")


def _delete_candidates(
    store: ObjectStore, remote: Stored, namespaces: frozenset[str], keep: set[str]
) -> tuple[str, ...]:
    deleted = []
    for prefix in sorted(namespaces):
        for key in store.keys(prefix):
            if (
                not key.startswith(prefix)
                or not (JSON_KEY.fullmatch(key) or IMAGE_KEY.fullmatch(key))
                or key in keep
            ):
                continue
            value = store.get(key)
            if value is None:
                continue
            latest = store.get(INDEX)
            if latest is None or latest.etag != remote.etag or latest.raw != remote.raw:
                raise PublishError("GC index changed concurrently")
            if not store.delete(key, expected=value.etag):
                raise PublishError("GC object changed concurrently")
            deleted.append(key)
    return tuple(deleted)

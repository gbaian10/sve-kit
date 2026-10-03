"""Fake-S3 races, poisoned query caches, failed attempts and restricted window GC."""

import os
import zlib
from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

import sve_carddb.snapshot.publish as runner
import sve_carddb.snapshot.publish.state as state_module
from sve_carddb.snapshot.export import Batch, Brotli, export_snapshot
from sve_carddb.snapshot.publish import Ledger, PublishError, collect, publish
from sve_carddb.snapshot.publish.plan import INDEX, prepare
from sve_carddb.snapshot.publish.state import attempt
from sve_carddb.snapshot.publish.storage import Stored
from sve_carddb.snapshot.values import array, canonical, object_value, parse, string

from .snapshot_publish_fixtures import FakeCDN, FakeS3, candidate, index, version
from .snapshot_publish_fixtures import changed_images as changed_images  # ruff: ignore[useless-import-alias] -- shared module-scoped second synthetic corpus
from .snapshot_publish_fixtures import images as images  # ruff: ignore[useless-import-alias] -- shared module-scoped synthetic corpus
from .snapshot_publish_fixtures import ledger as ledger  # ruff: ignore[useless-import-alias] -- isolated durable state per test
from .snapshot_publish_fixtures import mixed_images as mixed_images  # ruff: ignore[useless-import-alias] -- shared one-size update corpus

if TYPE_CHECKING:
    from pathlib import Path

    from .test_snapshot_preview_images import PublicImages

PUBLIC = frozenset(
    {
        "snapshots/blobs/",
        "snapshots/manifests/",
        "images/card_s/",
        "images/card_m/",
        "images/card_l/",
        "images/art_s/",
        "images/art_m/",
    }
)


def test_order_backup_images_cdn_json_index_and_receipt(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    store = FakeS3()
    cdn = FakeCDN(store)

    def observe(key: str) -> None:
        assert store.leases == 1
        ledger.verify_backup()
        if key.startswith("snapshots/"):
            assert len(cdn.requests) >= 10
            assert len([k for k in store.objects if k.startswith("images/")]) == 5
        if key == INDEX:
            assert {m.key for m in release.members} <= set(store.objects)

    store.before_put = observe
    result = publish(ledger, store, release, cdn)
    assert result["revision"] == 1
    assert index(store)["previous"] is None
    assert store.operations[-1][:2] == ("put", INDEX)
    assert attempt(ledger.read(), 1)["status"] == "committed"
    assert all("?v=1" in u for u in cdn.requests)


@pytest.mark.parametrize(
    "failure", ["image", "manifest", "cached404", "wrongbytes", "opaque"]
)
def test_failure_never_switches_current_and_same_plan_resumes(
    ledger: Ledger, images: PublicImages, changed_images: PublicImages, failure: str
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images)
    publish(ledger, store, first, cdn)
    prior = store.objects[INDEX]
    second = candidate(ledger, changed_images, from_version=version(first))
    if failure in {"image", "manifest"}:

        def interrupt(key: str) -> None:
            if key.startswith(
                "images/" if failure == "image" else "snapshots/manifests/"
            ):
                raise OSError("synthetic interruption")

        store.before_put = interrupt
    else:
        url = string(second.assets[0]["url"])
        cdn.cache[url] = b"wrong bytes" if failure == "wrongbytes" else None
    with pytest.raises((OSError, PublishError)):
        publish(ledger, store, second, cdn)
    assert store.objects[INDEX] == prior
    assert attempt(ledger.read(), 2)["status"] == "failed"
    store.before_put = None
    cdn.cache.clear()
    publish(ledger, store, second, cdn)
    assert index(store)["revision"] == 2
    assert object_value(index(store)["previous"])["data_version"] == version(first)


def test_cdn_ignoring_query_is_detected(
    ledger: Ledger, images: PublicImages, changed_images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store, ignore_query=True)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    prior = store.objects[INDEX]
    b = candidate(ledger, changed_images, from_version=version(a))
    with pytest.raises(
        PublishError, match=r"^CDN full-URL freshness verification failed$"
    ):
        publish(ledger, store, b, cdn)
    assert store.objects[INDEX] == prior


@pytest.mark.parametrize("target", ["image", "index", "json"])
def test_conditional_races_fail_closed(
    ledger: Ledger, images: PublicImages, changed_images: PublicImages, target: str
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    old = store.objects[INDEX]
    b = candidate(ledger, changed_images, from_version=version(a))
    triggered = False

    def race(key: str) -> None:
        nonlocal triggered
        selected = (
            key.startswith("images/")
            if target == "image"
            else key == INDEX
            if target == "index"
            else key.startswith("snapshots/blobs/") and key not in store.objects
        )
        if selected and not triggered:
            triggered = True
            store.objects[key] = Stored(b"rogue bytes", '"rogue"', {})

    store.before_put = race
    messages = {
        "image": "Origin image conditional overwrite failed",
        "index": "Version index CAS failed",
        "json": "Immutable JSON member differs from sealed release",
    }
    with pytest.raises(PublishError, match=r"^" + messages[target] + "$"):
        publish(ledger, store, b, cdn)
    assert triggered
    if target != "index":
        assert store.objects[INDEX] == old


def test_unknown_origin_bytes_are_not_silently_overwritten(
    ledger: Ledger, images: PublicImages
) -> None:
    a = candidate(ledger, images)
    store = FakeS3()
    store.objects[string(a.assets[0]["path"])] = Stored(b"unknown", '"other"', {})
    with pytest.raises(
        PublishError, match=r"^Unknown origin image bytes; manual recovery required$"
    ):
        publish(ledger, store, a, FakeCDN(store))
    assert not store.operations


def test_retry_cannot_change_outputs_or_reuse_data_version(
    ledger: Ledger, images: PublicImages, changed_images: PublicImages
) -> None:
    a = candidate(ledger, images)
    store = FakeS3()
    cdn = FakeCDN(store)
    cdn.cache[string(a.assets[0]["url"])] = None
    with pytest.raises(PublishError):
        publish(ledger, store, a, cdn)
    changed = deepcopy(a)
    changed.entry["published_at"] = "2026-10-04T02:00:00Z"
    with pytest.raises(PublishError, match=r"^Sealed local release changed$"):
        publish(ledger, store, changed, cdn)
    with pytest.raises(PublishError, match=r"^Data version was already reserved$"):
        ledger.reserve(version(a))
    b = candidate(ledger, changed_images)
    with pytest.raises(
        PublishError,
        match=r"^Unfinished release requires forward recovery or supersession$",
    ):
        publish(ledger, store, b, FakeCDN(store))


def test_text_only_keeps_tokens_and_skips_image_puts(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    view = deepcopy(images.projection)
    view.tables["printing"][0]["rarity_raw"] = "Synthetic new rarity"
    b = candidate(ledger, images, projection=view, from_version=version(a))
    store.operations.clear()
    publish(ledger, store, b, cdn)
    assert not [x for x in store.operations if x[1].startswith("images/")]
    assert all("?v=1" in string(x["url"]) for x in b.assets)


def test_a_b_a_different_tokens_and_current_image_bytes(
    ledger: Ledger, images: PublicImages, changed_images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    b = candidate(ledger, changed_images, from_version=version(a))
    publish(ledger, store, b, cdn)
    c = candidate(ledger, images, from_version=version(b))
    publish(ledger, store, c, cdn)
    assert all("?v=3" in string(x["url"]) for x in c.assets)
    assert all(store.objects[k].raw == raw for k, raw in c.images())
    assert all(
        cdn.get(string(x["url"])) == store.objects[string(x["path"])].raw
        for x in c.assets
    )


def test_withdrawal_gc_ignores_previous_images_and_restore_uses_new_token(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    hidden = deepcopy(images.projection)
    hidden.tables["image_asset"][0].update(
        publication_state="withdrawn", withdrawal_reason="Synthetic withdrawal"
    )
    hidden.tables["image_variant"] = []
    b = candidate(ledger, images, projection=hidden, from_version=version(a))
    publish(ledger, store, b, cdn)
    removed = collect(ledger, store, revision=2, namespaces=PUBLIC)
    assert {x for x in removed if x.startswith("images/")} == {
        string(x["path"]) for x in a.assets
    }
    assert string(a.entry["manifest_path"]) in store.objects
    c = candidate(ledger, images, from_version=version(b))
    publish(ledger, store, c, cdn)
    assert all("?v=3" in string(x["url"]) for x in c.assets)


def test_window_gc_shared_blobs_no_recursive_changes_history(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    b = candidate(ledger, images, from_version=version(a))
    publish(ledger, store, b, cdn)
    c = candidate(ledger, images, from_version=version(b))
    publish(ledger, store, c, cdn)
    shared = {m.key for m in a.members} & {m.key for m in c.members}
    deleted = collect(ledger, store, revision=3, namespaces=PUBLIC)
    assert string(a.entry["manifest_path"]) in deleted
    assert string(b.entry["manifest_path"]) in store.objects
    assert string(c.entry["manifest_path"]) in store.objects
    assert shared <= store.objects.keys()
    ref = object_value(b.snapshot.manifest["changes_ref"])
    assert string(ref["path"]) in store.objects
    assert object_value(index(store)["previous"])["data_version"] == version(b)
    assert ledger.read()["text_keys"]


@pytest.mark.parametrize(
    "namespace",
    ["raw/", "inventory/", "manifest/", "authored/", "snapshots/", "images/", ""],
)
def test_gc_refuses_nonexplicit_namespace(
    ledger: Ledger, images: PublicImages, namespace: str
) -> None:
    store = FakeS3()
    a = candidate(ledger, images)
    publish(ledger, store, a, FakeCDN(store))
    with pytest.raises(PublishError, match=r"^GC namespace is not explicitly public$"):
        collect(ledger, store, revision=1, namespaces=frozenset({namespace}))


def test_gc_preserves_outside_and_unrecognized_objects(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    a = candidate(ledger, images)
    publish(ledger, store, a, FakeCDN(store))
    protected = [
        "raw/source.png",
        "inventory/input.json",
        "manifest/backup.sqlite",
        "authored/test.yaml",
        "snapshots/blobs/raw.json",
        "images/card_m/../raw.png",
    ]
    for key in protected:
        store.objects[key] = Stored(b"sentinel", '"protected"', {})
    collect(ledger, store, revision=1, namespaces=PUBLIC)
    assert all(store.objects[key].raw == b"sentinel" for key in protected)


def test_gc_revision_and_object_etag_races(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    a = candidate(ledger, images)
    publish(ledger, store, a, FakeCDN(store))
    extra = "snapshots/blobs/" + "f" * 64 + ".json"
    store.objects[extra] = Stored(b"extra", '"old"', {})
    with pytest.raises(
        PublishError, match=r"^GC revision differs from committed current$"
    ):
        collect(ledger, store, revision=2, namespaces=PUBLIC)

    def race(key: str) -> None:
        store.objects[key] = Stored(b"replaced", '"new"', {})

    store.before_delete = race
    with pytest.raises(PublishError, match=r"^GC object changed concurrently$"):
        collect(ledger, store, revision=1, namespaces=PUBLIC)
    assert extra in store.objects


def test_gc_retains_legal_inflight_members(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    view = deepcopy(images.projection)
    view.tables["printing"][0]["rarity_raw"] = "Synthetic staging"
    b = candidate(ledger, images, projection=view, from_version=version(a))

    def fail(key: str) -> None:
        if key.startswith("snapshots/manifests/"):
            raise OSError("synthetic interrupted staging")

    store.before_put = fail
    with pytest.raises(OSError, match="synthetic"):
        publish(ledger, store, b, cdn)
    staging = (
        set(store.objects)
        - {m.key for m in a.members}
        - {INDEX}
        - {string(x["path"]) for x in a.assets}
    )
    assert staging
    store.before_put = None
    # The incomplete staging manifest is pinned, so collection fails closed.
    with pytest.raises(PublishError, match=r"^GC retained closure is incomplete$"):
        collect(ledger, store, revision=1, namespaces=PUBLIC)
    assert staging <= store.objects.keys()


def test_uncertain_index_commit_recovers_receipt_without_new_number(
    ledger: Ledger, images: PublicImages, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    original = runner._finish

    def interrupt(*_args: object) -> dict[str, object]:
        raise OSError("synthetic receipt interruption")

    monkeypatch.setattr(runner, "_finish", interrupt)
    with pytest.raises(OSError, match="synthetic"):
        publish(ledger, store, a, cdn)
    assert index(store)["revision"] == 1
    assert attempt(ledger.read(), 1)["status"] == "sealed"
    monkeypatch.setattr(runner, "_finish", original)
    publish(ledger, store, a, cdn)
    assert attempt(ledger.read(), 1)["status"] == "committed"
    assert ledger.read()["high_water"] == 1


def test_collision_registry_survives_public_gc_and_rejects_new_digest(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    a = candidate(ledger, images)
    publish(ledger, store, a, cdn)
    state = ledger.read()
    texts = object_value(state["text_keys"])
    key = next(iter(texts))
    real = string(texts[key])
    texts[key] = real[:-1] + ("0" if real[-1] != "0" else "1")
    ledger.save(state)
    b = candidate(ledger, images, from_version=version(a))
    with pytest.raises(PublishError, match=r"^Published text ID collision$"):
        publish(ledger, store, b, cdn)
    assert index(store)["revision"] == 1


@pytest.mark.parametrize("case", ["missing", "stale", "incomplete", "too_low"])
def test_lost_state_requires_complete_backup_and_reservation_upper_bound(
    tmp_path: Path, case: str
) -> None:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    assert value.reserve("first") == 1
    assert value.reserve("failed-unpublished") == 2
    proof = value.checkpoint()
    value.path.unlink()
    with pytest.raises(
        PublishError, match=r"^Release state missing; verified recovery required$"
    ):
        value.reserve("third")
    if case == "missing":
        value.copy.unlink()
    elif case == "stale":
        state = object_value(parse(value.copy.read_bytes()))
        state["high_water"] = 1
        array(state["attempts"]).pop()
        value.copy.write_bytes(canonical(state))
    elif case == "incomplete":
        value.receipts.write_bytes(
            value.receipts.read_bytes().splitlines(keepends=True)[0]
        )
    with pytest.raises(PublishError):
        value.recover(observed_max=3 if case == "too_low" else 1, proof=proof)
    assert not value.path.exists()


def test_verified_recovery_includes_failed_unpublished_reservations(
    tmp_path: Path,
) -> None:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    assert value.reserve("published") == 1
    assert value.reserve("failed") == 2
    proof = value.checkpoint()
    value.path.unlink()
    value.recover(observed_max=1, proof=proof)
    assert value.reserve("next") == 3
    with pytest.raises(
        PublishError, match=r"^Release state already exists; recover instead$"
    ):
        value.initialize()


def test_backup_failure_never_releases_revision_to_external_io(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    original = state_module.atomic

    def failing(path: Path, raw: bytes) -> None:
        if path == value.copy:
            raise OSError("synthetic backup failure")
        original(path, raw)

    monkeypatch.setattr(state_module, "atomic", failing)
    with pytest.raises(OSError, match="synthetic"):
        value.reserve("failed")
    assert value.read()["high_water"] == 1
    with pytest.raises(PublishError, match=r"^Release backup is missing or stale$"):
        value.verify_backup()
    monkeypatch.setattr(state_module, "atomic", original)
    assert value.reserve("next") == 2


def test_preview_and_unreferenced_changes_refused_before_remote_io(
    ledger: Ledger, images: PublicImages
) -> None:
    a = candidate(ledger, images)
    preview = replace(
        a.snapshot,
        manifest=a.snapshot.manifest
        | {"data_version": "preview-20261004T010204Z-0001"},
    )
    with pytest.raises(ValueError, match=r"^Formal publish refuses preview artifacts$"):
        prepare(preview, a.media, a.source, cdn_root="https://cdn.invalid/")
    with pytest.raises(PublishError, match=r"^Unreferenced changes attachment$"):
        prepare(
            a.snapshot,
            a.media,
            a.source,
            cdn_root="https://cdn.invalid/",
            changes=canonical({}),
        )


def test_failed_partial_write_supersession_allocates_fresh_tokens(
    ledger: Ledger, images: PublicImages, changed_images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images)
    publish(ledger, store, first, cdn)
    second = candidate(ledger, changed_images, from_version=version(first))
    calls = 0

    def interrupt(key: str) -> None:
        nonlocal calls
        if key.startswith("images/"):
            calls += 1
            if calls == 2:
                raise OSError("synthetic partial image upload")

    store.before_put = interrupt
    with pytest.raises(OSError, match="synthetic partial"):
        publish(ledger, store, second, cdn)
    store.before_put = None
    ledger.supersede(2)
    restored = candidate(ledger, images, from_version=version(first))
    assert all("?v=3" in string(a["url"]) for a in restored.assets)
    publish(ledger, store, restored, cdn)
    assert object_value(index(store)["previous"])["data_version"] == version(first)
    assert attempt(ledger.read(), 2)["status"] == "superseded"
    assert all(store.objects[k].raw == raw for k, raw in restored.images())


def test_receipts_pin_first_identity_event_after_three_releases_and_gc(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images)
    publish(ledger, store, first, cdn)
    second = candidate(ledger, images, from_version=version(first))
    publish(ledger, store, second, cdn)
    third = candidate(ledger, images, from_version=version(second))
    publish(ledger, store, third, cdn)
    collect(ledger, store, revision=3, namespaces=PUBLIC)
    assert string(first.entry["manifest_path"]) not in store.objects
    assert ledger.read()["first_events"] == {"change": version(first)}
    assert (
        object_value(attempt(ledger.read(), 1)["receipt"])["manifest_sha256"]
        == first.entry["manifest_sha256"]
    )


def test_forged_media_state_rejected_before_remote_access(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    member = next(iter(object_value(release.media.state["members"]).values()))
    object_value(member)["binding"] = ["wrong-image", 123, 0]
    store = FakeS3()
    with pytest.raises(
        PublishError, match=r"^Media plan differs from durable committed basis$"
    ):
        publish(ledger, store, release, FakeCDN(store))
    assert not store.objects
    assert not store.operations


@pytest.mark.parametrize(
    "root",
    [
        "http://cdn.invalid/",
        "https://user:password@cdn.invalid/",
        "https://cdn.invalid/?v=1",
        "https://cdn.invalid/#x",
        "https://cdn.invalid",
    ],
)
def test_cdn_root_rejects_credentials_query_and_insecure_scheme(
    ledger: Ledger, images: PublicImages, root: str
) -> None:
    release = candidate(ledger, images)
    with pytest.raises(PublishError, match=r"^Explicit HTTPS CDN root required$"):
        prepare(release.snapshot, release.media, release.source, cdn_root=root)


def test_missing_changes_and_nonadjacent_changes_block_continuation(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images)
    publish(ledger, store, first, cdn)
    second = candidate(ledger, images)
    with pytest.raises(
        PublishError, match=r"^Formal continuation requires adjacent changes$"
    ):
        publish(ledger, store, second, cdn)
    third = candidate(ledger, images, from_version="20261001T000000Z-0001")
    with pytest.raises(
        PublishError, match=r"^Changes do not describe the direct previous release$"
    ):
        publish(ledger, store, third, cdn)
    assert index(store)["revision"] == 1


def test_current_and_newer_receipts_cannot_be_rolled_back_by_retry(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images)
    publish(ledger, store, first, cdn)
    second = candidate(ledger, images, from_version=version(first))
    publish(ledger, store, second, cdn)
    with pytest.raises(
        PublishError, match=r"^Committed release cannot replace a newer current$"
    ):
        publish(ledger, store, first, cdn)
    assert index(store)["revision"] == 2


def test_existing_json_is_never_overwritten_even_on_matching_path(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    store = FakeS3()
    member = release.members[0]
    store.objects[member.key] = Stored(
        b"different immutable bytes", '"existing"', member.headers
    )
    with pytest.raises(
        PublishError, match=r"^Immutable JSON member differs from sealed release$"
    ):
        publish(ledger, store, release, FakeCDN(store))
    assert store.objects[member.key].raw == b"different immutable bytes"
    assert not [op for op in store.operations if op[1] == member.key]


def test_index_change_during_gc_stops_before_delete(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    release = candidate(ledger, images)
    publish(ledger, store, release, FakeCDN(store))
    extra = "snapshots/blobs/" + "f" * 64 + ".json"
    store.objects[extra] = Stored(b"orphan", '"orphan"', {})
    triggered = False

    def race(key: str) -> None:
        nonlocal triggered
        if key == extra and not triggered:
            triggered = True
            old = store.objects[INDEX]
            store.objects[INDEX] = Stored(old.raw, '"other-index-etag"', old.headers)

    store.before_get = race
    with pytest.raises(PublishError, match=r"^GC index changed concurrently$"):
        collect(ledger, store, revision=1, namespaces=PUBLIC)
    assert extra in store.objects
    assert not [op for op in store.operations if op[0] == "delete"]


def test_gc_missing_retained_payload_fails_closed(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    release = candidate(ledger, images)
    publish(ledger, store, release, FakeCDN(store))
    missing = next(
        m.key for m in release.members if m.key.startswith("snapshots/blobs/")
    )
    del store.objects[missing]
    with pytest.raises(PublishError, match=r"^GC retained closure is incomplete$"):
        collect(ledger, store, revision=1, namespaces=PUBLIC)


def test_reservation_fsync_precedes_backup_and_external_use(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    original = os.fsync
    fsyncs = 0

    def observed(fd: int) -> None:
        nonlocal fsyncs
        fsyncs += 1
        original(fd)

    monkeypatch.setattr(os, "fsync", observed)
    value.reserve("synthetic-first")
    assert fsyncs == 5
    value.verify_backup()


@pytest.mark.parametrize("field", ["high_water", "attempts", "format"])
def test_corrupt_backup_is_not_a_new_allocator(tmp_path: Path, field: str) -> None:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    value.reserve("reserved")
    proof = value.checkpoint()
    state = object_value(parse(value.copy.read_bytes()))
    state[field] = True if field != "attempts" else []
    value.copy.write_bytes(canonical(state))
    value.path.unlink()
    with pytest.raises(PublishError):
        value.recover(observed_max=0, proof=proof)
    assert not value.path.exists()


def test_one_art_size_changes_both_art_urls_but_not_card_urls(
    ledger: Ledger, images: PublicImages, mixed_images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images)
    publish(ledger, store, first, cdn)
    second = candidate(ledger, mixed_images, from_version=version(first))
    unchanged_art = next(
        a for a in second.assets if string(a["path"]).startswith("images/art_m/")
    )
    cdn.cache[string(unchanged_art["url"])] = None
    store.operations.clear()
    with pytest.raises(
        PublishError, match=r"^CDN full-URL freshness verification failed$"
    ):
        publish(ledger, store, second, cdn)
    assert index(store)["revision"] == 1
    assert [op[1] for op in store.operations if op[0] == "put"] == [
        string(
            next(
                a
                for a in second.assets
                if string(a["path"]).startswith("images/art_s/")
            )["path"]
        )
    ]
    cdn.cache.clear()
    publish(ledger, store, second, cdn)
    assert all(
        string(a["url"]).endswith(
            "?v=2" if string(a["path"]).startswith("images/art_") else "?v=1"
        )
        for a in second.assets
    )
    assert string(unchanged_art["url"]) in cdn.requests


def test_origin_metadata_corruption_blocks_switch(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    release = candidate(ledger, images)
    cdn = FakeCDN(store)
    target = string(release.assets[0]["path"])
    calls = 0

    def corrupt(key: str) -> None:
        nonlocal calls
        if key == target and key in store.objects:
            calls += 1
            if calls == 1:
                original = store.objects[key]
                store.objects[key] = Stored(original.raw, original.etag, {})

    store.before_get = corrupt
    with pytest.raises(PublishError, match=r"^Origin image verification failed$"):
        publish(ledger, store, release, cdn)
    assert INDEX not in store.objects


def test_rolled_back_backup_and_receipts_cannot_satisfy_new_checkpoint(
    tmp_path: Path,
) -> None:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    value.reserve("first")
    older_copy, older_receipts = value.copy.read_bytes(), value.receipts.read_bytes()
    value.reserve("failed-unpublished")
    proof = value.checkpoint()
    value.path.unlink()
    value.copy.write_bytes(older_copy)
    value.receipts.write_bytes(older_receipts)
    with pytest.raises(
        PublishError,
        match=r"^Recovery backup differs from independently pinned checkpoint$",
    ):
        value.recover(observed_max=1, proof=proof)
    assert not value.path.exists()


@pytest.mark.parametrize("subdir", ["", "nested"])
def test_backup_cannot_share_primary_tree(tmp_path: Path, subdir: str) -> None:
    with pytest.raises(
        PublishError, match=r"^Release state and backup roots must be disjoint$"
    ):
        Ledger(tmp_path, tmp_path / subdir)


def test_reservation_journal_gap_after_crash_never_guesses_upper_bound(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    value = Ledger(tmp_path / "state", tmp_path / "backup")
    value.initialize()
    original = state_module.atomic

    def interrupt(path: Path, raw: bytes) -> None:
        if path == value.path:
            raise OSError("synthetic crash after receipt fsync")
        original(path, raw)

    monkeypatch.setattr(state_module, "atomic", interrupt)
    with pytest.raises(OSError, match="synthetic crash"):
        value.reserve("burned")
    assert len(value.receipts.read_bytes().splitlines()) == 1
    with pytest.raises(
        PublishError, match=r"^Release state and reservation evidence disagree$"
    ):
        value.reserve("next")


def test_invalid_index_message_does_not_echo_untrusted_values(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    store = FakeS3()
    store.objects[INDEX] = Stored(
        canonical({"secret": "Synthetic private sentinel"}),
        '"bad"',
        {"cache-control": "no-store"},
    )
    with pytest.raises(PublishError, match=r"^Invalid public version index$"):
        publish(ledger, store, release, FakeCDN(store))
    assert not store.operations


def test_retry_pins_valid_regenerated_manifest_and_cdn_root(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    store = FakeS3()
    cdn = FakeCDN(store)
    cdn.cache[string(release.assets[0]["url"])] = None
    with pytest.raises(PublishError):
        publish(ledger, store, release, cdn)
    regenerated = prepare(
        replace(
            release.snapshot,
            manifest=release.snapshot.manifest
            | {"published_at": "2026-10-04T02:00:00Z"},
        ),
        release.media,
        release.source,
        cdn_root="https://other-cdn.invalid/",
    )
    store.operations.clear()
    with pytest.raises(PublishError, match=r"^Retry changes reserved release outputs$"):
        publish(ledger, store, regenerated, FakeCDN(store))
    assert not store.operations
    assert INDEX not in store.objects


def test_three_representation_closure_and_pinned_compressor(
    ledger: Ledger, images: PublicImages
) -> None:
    # A synthetic codec exercises injection without installing a live Brotli adapter.
    codec = Brotli("synthetic-zlib-test-v1", zlib.compress)
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images, brotli=codec)
    with pytest.raises(
        PublishError, match=r"^Brotli requires the pinned producer compressor$"
    ):
        publish(ledger, store, first, cdn)
    assert not store.objects
    publish(ledger, store, first, cdn, brotli=codec)
    second = candidate(ledger, images, from_version=version(first), brotli=codec)
    publish(ledger, store, second, cdn, brotli=codec)
    third = candidate(ledger, images, from_version=version(second), brotli=codec)
    publish(ledger, store, third, cdn, brotli=codec)
    collect(ledger, store, revision=3, namespaces=PUBLIC)
    retained = {m.key for r in (second, third) for m in r.members}
    assert retained <= store.objects.keys()
    assert all(
        string(first.entry["manifest_path"]) + suffix not in store.objects
        for suffix in ("", ".gz", ".br")
    )
    assert any(k.endswith(".br") for k in retained)


def test_unsupported_preview_errors_are_redacted(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    malformed = replace(
        release.snapshot,
        manifest=release.snapshot.manifest
        | {"required_capabilities": ["Synthetic private sentinel"]},
    )
    with pytest.raises(PublishError, match=r"^Formal release validation failed$"):
        prepare(malformed, release.media, release.source, cdn_root=release.cdn_root)


def test_stale_backup_blocks_publish_before_origin_access(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    ledger.copy.write_bytes(b"stale synthetic backup")
    store = FakeS3()
    with pytest.raises(PublishError, match=r"^Release backup is missing or stale$"):
        publish(ledger, store, release, FakeCDN(store))
    assert not store.operations
    assert not store.objects


def test_first_event_cannot_claim_an_unproven_historical_release(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    projection = deepcopy(release.media.projection)
    projection.tables["identity_change"][0]["data_version"] = "20260101T000000Z-0001"
    media = replace(release.media, projection=projection)
    snapshot = export_snapshot(
        projection,
        images.ownership,
        Batch(
            "preview-" + version(release),
            string(release.snapshot.manifest["published_at"]),
            ("jp",),
        ),
        format_version="2.0.0",
    )
    snapshot = replace(
        snapshot, manifest=snapshot.manifest | {"data_version": version(release)}
    )
    forged = prepare(snapshot, media, release.source, cdn_root=release.cdn_root)
    store = FakeS3()
    with pytest.raises(
        PublishError, match=r"^Unproven or changed event first-publication version$"
    ):
        publish(ledger, store, forged, FakeCDN(store))
    assert not store.operations


@pytest.mark.parametrize("revision", [True, 0, -1, 2, 9007199254740992])
def test_guessed_and_bool_reservations_rejected(ledger: Ledger, revision: int) -> None:
    ledger.reserve("first")
    with pytest.raises(
        ValueError, match=r"^(?:Expected safe integer|Unknown release reservation)$"
    ):
        attempt(ledger.read(), revision)


def test_superseded_release_cannot_resume_its_old_tokens(
    ledger: Ledger, images: PublicImages
) -> None:
    release = candidate(ledger, images)
    store = FakeS3()
    cdn = FakeCDN(store)
    cdn.cache[string(release.assets[0]["url"])] = None
    with pytest.raises(PublishError):
        publish(ledger, store, release, cdn)
    ledger.supersede(1)
    store.operations.clear()
    with pytest.raises(PublishError, match=r"^Superseded release cannot be resumed$"):
        publish(ledger, store, release, FakeCDN(store))
    assert not store.operations
    assert INDEX not in store.objects


def test_gc_retains_complete_inflight_closure_after_failed_index_commit(
    ledger: Ledger, images: PublicImages
) -> None:
    store = FakeS3()
    cdn = FakeCDN(store)
    first = candidate(ledger, images)
    publish(ledger, store, first, cdn)
    view = deepcopy(images.projection)
    view.tables["printing"][0]["rarity_raw"] = "Synthetic complete staging"
    second = candidate(ledger, images, projection=view, from_version=version(first))

    def interrupt(key: str) -> None:
        if key == INDEX:
            raise OSError("synthetic failed index commit")

    store.before_put = interrupt
    with pytest.raises(OSError, match="synthetic failed index"):
        publish(ledger, store, second, cdn)
    staging = {m.key for m in second.members} - {m.key for m in first.members}
    assert staging
    assert staging <= store.objects.keys()
    store.before_put = None
    deleted = collect(ledger, store, revision=1, namespaces=PUBLIC)
    assert not staging.intersection(deleted)
    assert staging <= store.objects.keys()
    publish(ledger, store, second, cdn)
    assert index(store)["revision"] == 2

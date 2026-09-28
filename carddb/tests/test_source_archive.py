"""Synthetic, isolated acceptance tests for raw-source archival."""

import errno
import hashlib
import os
import sqlite3
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath

import pytest

import sve_carddb.source_archive as archive
from sve_carddb.crawl import list_root, sets_root
from sve_carddb.extract.jsonl import extract_cards
from sve_carddb.manifest import (
    AlreadyRunningError,
    ExclusiveLock,
    Kind,
    Link,
    Manifest,
    Outcome,
    Region,
    RequestResult,
    RequestStart,
    Resource,
)
from sve_carddb.source_archive import (
    ArchiveError,
    ArchiveRaceError,
    ArchiveReader,
    ArchiveStore,
    IncompleteBatchError,
    backup_batch,
    capacity_report,
    restore_backup,
    seal_batch,
    verify_batch,
)
from sve_carddb.sources import official_jp as jp
from sve_carddb.store import compress

NOW = datetime(2026, 9, 29, tzinfo=UTC)


def _store(tmp_path: Path, *, read_roots: tuple[Path, ...] = ()) -> ArchiveStore:
    data = tmp_path / "data"
    data.mkdir(parents=True)
    return ArchiveStore(
        data,
        data / "manifest" / "manifest.sqlite",
        data / "manifest" / ".lock",
        tmp_path / "archive",
        "test-store",
        read_roots,
    )


def _resource(url: str, path: str, raw: bytes, kind: Kind = Kind.IMAGE) -> Resource:
    return Resource(
        url=url,
        region=Region.JP,
        kind=kind,
        path=PurePosixPath(path),
        sha256=hashlib.sha256(raw).hexdigest(),
        raw_bytes=len(raw),
        stored_bytes=len(raw),
        content_type="image/png" if kind is Kind.IMAGE else "text/html",
        etag=None,
        last_modified=None,
        first_fetched_at=NOW,
        last_checked_at=NOW,
        last_changed_at=NOW,
        archived_at=None,
    )


def _put(store: ArchiveStore, resource: Resource, data: bytes) -> None:
    target = store.data_root / resource.path
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_suffix(".new")
    temporary.write_bytes(data)
    temporary.replace(target)
    with Manifest.open(store.manifest_path) as manifest, manifest.transaction():
        manifest.resources.put(resource)


def test_dedup_versions_and_backup_restore(tmp_path: Path) -> None:
    store = _store(tmp_path)
    first = b"\x89PNG\r\n\x1a\n" + b"synthetic-image" * 100
    second = b"\x89PNG\r\n\x1a\n" + b"different-image" * 100
    left = _resource("https://example.invalid/left.png", "raw/left.png", first)
    right = _resource("https://example.invalid/right.png", "raw/right.png", first)
    _put(store, left, first)
    _put(store, right, first)
    one = seal_batch(store)
    assert len(one.inventory.current) == 2
    assert len(list((store.root / "raw").rglob("*.raw"))) == 1
    assert (
        ArchiveReader(store.root, store.store_id, one.batch_id).read(left.url) == first
    )

    _put(
        store,
        replace(
            left,
            sha256=hashlib.sha256(second).hexdigest(),
            raw_bytes=len(second),
            stored_bytes=len(second),
        ),
        second,
    )
    two = seal_batch(store)
    assert len(two.inventory.entries) == 3
    _put(store, left, first)
    three = seal_batch(store)
    assert len(three.inventory.entries) == 3
    assert (
        three.inventory.current[0].source_version_id
        == one.inventory.current[0].source_version_id
    )
    assert len(list((store.root / "raw").rglob("*.raw"))) == 2
    report = capacity_report(store, three.batch_id)
    assert report.png_versions == 3
    assert report.unique_raw_blobs == 2
    assert report.dedup_saved_bytes == len(first)
    assert report.latest_missing == 0

    backup = tmp_path / "backup"
    receipt = backup_batch(store, backup, three.batch_id, require_separate_device=False)
    assert receipt.input_batch_ids == [three.batch_id]
    restored = tmp_path / "restored"
    assert (
        len(restore_backup(backup, restored, store.store_id, three.batch_id).entries)
        == 3
    )
    assert (
        verify_batch(restored, store.store_id, three.batch_id).current
        == three.inventory.current
    )
    assert len(list(restored.rglob("-wal"))) == 0
    (backup / three.inventory.entries[0].blob.path).unlink()
    with pytest.raises(ArchiveError):
        restore_backup(
            backup, tmp_path / "incomplete-restore", store.store_id, three.batch_id
        )


def test_zstd_raw_and_committed_wal(tmp_path: Path) -> None:
    store = _store(tmp_path)
    raw = b"<html>synthetic text</html>"
    stored = compress(raw)
    resource = replace(
        _resource("https://example.invalid/card", "raw/card.html.zst", raw, Kind.CARD),
        stored_bytes=len(stored),
    )
    _put(store, resource, stored)
    with sqlite3.connect(store.manifest_path) as connection:
        connection.execute("PRAGMA journal_mode=WAL")
    result = seal_batch(store)
    assert (
        ArchiveReader(store.root, store.store_id, result.batch_id).read(resource.url)
        == raw
    )
    with Manifest.open_snapshot(result.path / "manifest.sqlite") as snapshot:
        assert snapshot.resources.get(resource.url) == resource


def test_missing_and_unsafe_sources_never_seal(tmp_path: Path) -> None:
    store = _store(tmp_path)
    with pytest.raises(ArchiveError, match="manifest does not exist"):
        seal_batch(store)
    raw = b"synthetic"
    resource = _resource("https://example.invalid/missing", "raw/missing.png", raw)
    with Manifest.open(store.manifest_path) as manifest, manifest.transaction():
        manifest.resources.put(resource)
    with pytest.raises(IncompleteBatchError) as missing:
        seal_batch(store, retries=0)
    assert missing.value.missing[0].reason == "missing_raw"
    assert not list((store.root / "batches").glob("*"))

    outside = tmp_path / "outside.png"
    outside.write_bytes(raw)
    target = store.data_root / resource.path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.symlink_to(outside)
    with pytest.raises(IncompleteBatchError) as unsafe:
        seal_batch(store, retries=0)
    assert unsafe.value.missing[0].reason == "unsafe_path"


def test_hash_corruption_and_lock_conflict(tmp_path: Path) -> None:
    store = _store(tmp_path)
    raw = b"synthetic image"
    resource = _resource("https://example.invalid/card.png", "raw/card.png", raw)
    _put(store, resource, raw)
    with ExclusiveLock(store.lock_path), pytest.raises(AlreadyRunningError):
        seal_batch(store)
    result = seal_batch(store)
    blob = store.root / result.inventory.entries[0].blob.path
    blob.write_bytes(b"corrupt")
    with pytest.raises(ArchiveError):
        verify_batch(store.root, store.store_id, result.batch_id)


def test_allowed_root_symlink(tmp_path: Path) -> None:
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    raw = b"synthetic PNG bytes"
    (allowed / "card.png").write_bytes(raw)
    store = _store(tmp_path, read_roots=(allowed,))
    resource = _resource("https://example.invalid/linked.png", "raw/linked.png", raw)
    link = store.data_root / resource.path
    link.parent.mkdir(parents=True)
    link.symlink_to(allowed / "card.png")
    with Manifest.open(store.manifest_path) as manifest, manifest.transaction():
        manifest.resources.put(resource)
    result = seal_batch(store)
    assert (
        ArchiveReader(store.root, store.store_id, result.batch_id).read(resource.url)
        == raw
    )


def test_pre_archive_history_gap_is_reported_without_forging_bytes(
    tmp_path: Path,
) -> None:
    store = _store(tmp_path)
    raw = b"current version"
    resource = _resource("https://example.invalid/history.png", "raw/history.png", raw)
    _put(store, resource, raw)
    old_hash = "a" * 64
    with Manifest.open(store.manifest_path) as manifest:
        request_id = manifest.requests.start(
            RequestStart("run", "old", 1, 0, resource.url, resource.url, None)
        )
        with manifest.transaction():
            manifest.requests.finish(
                request_id,
                RequestResult(
                    outcome=Outcome.CHANGED, response_sha256=old_hash, response_bytes=7
                ),
            )
    result = seal_batch(store)
    assert len(result.inventory.entries) == 1
    assert [
        (item.url, item.expected_raw_sha256, item.reason)
        for item in result.inventory.history_gaps
    ] == [(resource.url, "sha256:" + old_hash, "missing_history")]


def test_offline_extract_repeats_without_touching_sources(tmp_path: Path) -> None:
    store = _store(tmp_path)
    number = "BP01-003"
    raw = (
        Path(__file__).parent / "fixtures" / "official_jp" / "card_BP01-003.html"
    ).read_bytes()
    stored = compress(raw)
    resource = replace(
        _resource(jp.card_url(number), "raw/card.html.zst", raw, Kind.CARD),
        stored_bytes=len(stored),
    )
    _put(store, resource, stored)
    with Manifest.open(store.manifest_path) as manifest:
        sets = manifest.generations.start(sets_root(Region.JP))
        with manifest.transaction():
            manifest.generations.add_page(
                sets.id,
                jp.sets_url(),
                "sets-hash",
                [Link("https://example.invalid/BP01", Kind.LIST, 0, "BP01")],
            )
        manifest.generations.validate(sets.id, declared_total=1)
        cards = manifest.generations.start(list_root("BP01"))
        with manifest.transaction():
            manifest.generations.add_page(
                cards.id,
                jp.list_url("BP01", 1),
                "list-hash",
                [Link(resource.url, Kind.CARD, 0, number)],
            )
        manifest.generations.validate(cards.id, declared_total=1)
    result = seal_batch(store)
    frozen = result.path / "manifest.sqlite"
    before = hashlib.sha256(frozen.read_bytes()).hexdigest()
    reader = ArchiveReader(store.root, store.store_id, result.batch_id)
    with Manifest.open_snapshot(frozen) as snapshot:
        first = extract_cards(snapshot, reader, tmp_path / "derived" / "one.jsonl")
        second = extract_cards(snapshot, reader, tmp_path / "derived" / "two.jsonl")
    assert first.written == second.written == 1
    assert (tmp_path / "derived" / "one.jsonl").read_bytes() == (
        tmp_path / "derived" / "two.jsonl"
    ).read_bytes()
    assert hashlib.sha256(frozen.read_bytes()).hexdigest() == before


def test_cross_device_copy_and_replace_race(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = _store(tmp_path)
    raw = b"synthetic image"
    resource = _resource("https://example.invalid/card.png", "raw/card.png", raw)
    _put(store, resource, raw)
    original_link = os.link

    def cross_device(
        source: str | os.PathLike[str], target: str | os.PathLike[str]
    ) -> None:
        if Path(target).parent.name == "pins":
            raise OSError(errno.EXDEV, "synthetic cross-device fallback")
        original_link(source, target)

    monkeypatch.setattr(os, "link", cross_device)
    monkeypatch.setattr(archive, "_try_reflink", lambda _fd, _path: False)
    result = seal_batch(store)
    assert result.copied_bytes == len(raw)

    another = _store(tmp_path / "other")
    _put(another, resource, raw)
    original_prepare = archive._prepare  # pyright: ignore[reportPrivateUsage] -- race injection

    def replace_during_prepare(pin: archive._Pinned) -> None:  # pyright: ignore[reportPrivateUsage] -- race injection
        original_prepare(pin)
        target = another.data_root / resource.path
        replacement = target.with_suffix(".new")
        replacement.write_bytes(b"changed")
        replacement.replace(target)

    monkeypatch.setattr(archive, "_prepare", replace_during_prepare)
    with pytest.raises(ArchiveRaceError):
        seal_batch(another, retries=0)
    assert not (another.root / "batches").exists()

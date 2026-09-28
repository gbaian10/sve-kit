"""Synthetic, isolated acceptance tests for raw-source archival."""

import errno
import hashlib
import os
import sqlite3
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath

import pytest
from typer.testing import CliRunner

import sve_carddb.source_archive as archive
from sve_carddb import cli
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


def _put_zst(
    store: ArchiveStore, url: str, path: str, raw: bytes, kind: Kind
) -> Resource:
    stored = compress(raw)
    resource = replace(_resource(url, path, raw, kind), stored_bytes=len(stored))
    _put(store, resource, stored)
    return resource


def test_archive_store_rejects_overlapping_latest_and_allowed_roots(
    tmp_path: Path,
) -> None:
    store = _store(tmp_path)
    with pytest.raises(ArchiveError, match="must not overlap"):
        replace(store, root=tmp_path)
    allowed = tmp_path / "allowed"
    allowed.mkdir()
    with pytest.raises(ArchiveError, match="allowed read roots"):
        replace(store, root=allowed / "archive", read_roots=(allowed,))


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


def test_zstd_reused_raw_seals_twice_and_deduplicates_urls(tmp_path: Path) -> None:
    store = _store(tmp_path)
    raw = b"<html>same raw page</html>"
    left = _put_zst(
        store, "https://example.invalid/one", "raw/one.html.zst", raw, Kind.CARD
    )
    right = _put_zst(
        store, "https://example.invalid/two", "raw/two.html.zst", raw, Kind.CARD
    )
    first = seal_batch(store)
    second = seal_batch(store)
    assert len(list((store.root / "raw").rglob("*.raw"))) == 1
    assert len(second.inventory.current) == 2
    assert {item.source_version_id for item in second.inventory.current} == {
        item.source_version_id for item in first.inventory.current
    }
    reader = ArchiveReader(store.root, store.store_id, second.batch_id)
    assert reader.read(left.url) == reader.read(right.url) == raw


@pytest.mark.parametrize(
    ("kind", "suffix", "first_raw", "second_raw"),
    [
        (Kind.CARD, "html", b"<html>A</html>", b"<html>B</html>"),
        (Kind.API, "json", b'{"version":"A"}', b'{"version":"B"}'),
    ],
    ids=["html", "json"],
)
def test_zstd_a_b_a_reuses_source_version_and_new_receipt(
    tmp_path: Path,
    kind: Kind,
    suffix: str,
    first_raw: bytes,
    second_raw: bytes,
) -> None:
    store = _store(tmp_path)
    url = f"https://example.invalid/source.{suffix}"
    path = f"raw/source.{suffix}.zst"
    _put_zst(store, url, path, first_raw, kind)
    first = seal_batch(store)
    _put_zst(store, url, path, second_raw, kind)
    middle = seal_batch(store)
    _put_zst(store, url, path, first_raw, kind)
    last = seal_batch(store)
    assert (
        first.inventory.current[0].source_version_id
        == last.inventory.current[0].source_version_id
    )
    assert (
        middle.inventory.current[0].source_version_id
        != first.inventory.current[0].source_version_id
    )
    assert first.inventory.entries[0].receipt_id != last.inventory.entries[0].receipt_id
    assert len(last.inventory.entries) == 2
    assert (
        ArchiveReader(store.root, store.store_id, last.batch_id).read(url) == first_raw
    )


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


def test_bad_latest_hash_stops_before_seal(tmp_path: Path) -> None:
    store = _store(tmp_path)
    resource = _resource(
        "https://example.invalid/damaged.png", "raw/damaged.png", b"expected"
    )
    _put(store, resource, b"different")
    with pytest.raises(IncompleteBatchError) as stopped:
        seal_batch(store, retries=0)
    assert [(item.url, item.reason) for item in stopped.value.missing] == [
        (resource.url, "hash_mismatch")
    ]
    assert not (store.root / "batches").exists()


def test_manifest_change_between_preparation_and_final_lock(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = _store(tmp_path)
    first = _resource("https://example.invalid/first.png", "raw/first.png", b"first")
    _put(store, first, b"first")
    original_prepare = archive._prepare  # pyright: ignore[reportPrivateUsage] -- race injection
    changed = False

    def change_manifest(pin: archive._Pinned) -> None:  # pyright: ignore[reportPrivateUsage] -- race injection
        nonlocal changed
        original_prepare(pin)
        if not changed:
            changed = True
            second = _resource(
                "https://example.invalid/second.png", "raw/second.png", b"second"
            )
            with Manifest.open(store.manifest_path) as manifest, manifest.transaction():
                manifest.resources.put(second)

    monkeypatch.setattr(archive, "_prepare", change_manifest)
    with pytest.raises(ArchiveRaceError, match="manifest changed"):
        seal_batch(store, retries=0)
    assert not (store.root / "batches").exists()


def test_metadata_installs_outside_manifest_lock(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = _store(tmp_path)
    resource = _resource(
        "https://example.invalid/source.png", "raw/source.png", b"source"
    )
    _put(store, resource, b"source")
    original_install = archive._install_bytes  # pyright: ignore[reportPrivateUsage] -- lock assertion
    original_link = archive._install_link  # pyright: ignore[reportPrivateUsage] -- lock assertion
    installed: list[str] = []

    def check_install(path: Path, data: bytes) -> None:
        with ExclusiveLock(store.lock_path):
            installed.append(path.parent.name)
        original_install(path, data)

    def check_link(
        source: Path, target: Path, *, trusted_existing: bool = False
    ) -> None:
        if (
            target.is_relative_to(store.root / "raw")
            or target.is_relative_to(store.root / "versions")
            or target.is_relative_to(store.root / "manifests")
        ):
            with ExclusiveLock(store.lock_path):
                installed.append(target.parent.name)
        original_link(source, target, trusted_existing=trusted_existing)

    monkeypatch.setattr(archive, "_install_bytes", check_install)
    monkeypatch.setattr(archive, "_install_link", check_link)
    seal_batch(store, retries=0)
    assert {"receipts", "descriptors", "versions", "manifests"} <= set(installed)


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


def test_backup_rejects_same_device_by_default(tmp_path: Path) -> None:
    store = _store(tmp_path)
    resource = _resource(
        "https://example.invalid/source.png", "raw/source.png", b"source"
    )
    _put(store, resource, b"source")
    result = seal_batch(store)
    with pytest.raises(ArchiveError, match="separate device"):
        backup_batch(store, tmp_path / "same-device-backup", result.batch_id)


def test_restore_rejects_symlink_and_changed_seal(tmp_path: Path) -> None:
    store = _store(tmp_path)
    resource = _resource(
        "https://example.invalid/source.png", "raw/source.png", b"source"
    )
    _put(store, resource, b"source")
    result = seal_batch(store)
    backup = tmp_path / "backup"
    backup_batch(store, backup, result.batch_id, require_separate_device=False)
    blob_path = result.inventory.entries[0].blob.path
    blob = backup / blob_path
    blob.unlink()
    outside = tmp_path / "outside.png"
    outside.write_bytes(b"source")
    blob.symlink_to(outside)
    with pytest.raises(ArchiveError, match="symlink"):
        restore_backup(
            backup, tmp_path / "bad-symlink-restore", store.store_id, result.batch_id
        )
    blob.unlink()
    blob.write_bytes((store.root / blob_path).read_bytes())
    seal = backup / "batches" / result.batch_id.removeprefix("sha256:") / "seal.json"
    seal.write_bytes(
        seal.read_bytes().replace(
            result.batch_id.encode(), ("sha256:" + "0" * 64).encode()
        )
    )
    with pytest.raises(ArchiveError, match="batch seal"):
        restore_backup(
            backup, tmp_path / "bad-seal-restore", store.store_id, result.batch_id
        )


def test_cli_restores_first_batch_and_allows_later_manual_check(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = _store(tmp_path)
    resource = _resource(
        "https://example.invalid/source.png", "raw/source.png", b"source"
    )
    _put(store, resource, b"source")
    backup = tmp_path / "backup"
    monkeypatch.setenv("SVE_DATA_DIR", str(store.data_root))
    real_backup = backup_batch
    real_restore = restore_backup
    restored: list[str] = []

    def local_backup(target: ArchiveStore, destination: Path, batch_id: str) -> object:
        return real_backup(target, destination, batch_id, require_separate_device=False)

    def track_restore(
        source: Path, destination: Path, store_id: str, batch_id: str
    ) -> object:
        restored.append(batch_id)
        return real_restore(source, destination, store_id, batch_id)

    monkeypatch.setattr(cli, "backup_batch", local_backup)
    monkeypatch.setattr(cli, "restore_backup", track_restore)
    runner = CliRunner()
    args = [
        "archive",
        "seal",
        str(store.root),
        str(backup),
        "--store-id",
        store.store_id,
    ]
    first = runner.invoke(cli.app, args)
    assert first.exit_code == 0, first.output
    assert "restore check: passed" in first.output
    second = runner.invoke(cli.app, args)
    assert second.exit_code == 0, second.output
    assert "restore check: passed" not in second.output
    assert len(restored) == 1
    third = runner.invoke(cli.app, [*args, "--restore-check"])
    assert third.exit_code == 0, third.output
    assert len(restored) == 2

    batch_id = "sha256:" + next((store.root / "batches").iterdir()).name
    backed_up = runner.invoke(
        cli.app,
        [
            "archive",
            "backup",
            str(store.root),
            str(backup),
            batch_id,
            "--store-id",
            store.store_id,
        ],
    )
    assert backed_up.exit_code == 0, backed_up.output
    destination = tmp_path / "manual-restore"
    checked = runner.invoke(
        cli.app,
        [
            "archive",
            "restore-check",
            str(backup),
            str(destination),
            batch_id,
            "--store-id",
            store.store_id,
        ],
    )
    assert checked.exit_code == 0, checked.output
    assert len(restored) == 3


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
    result = seal_batch(store, retries=0)
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

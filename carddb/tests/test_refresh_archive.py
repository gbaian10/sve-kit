"""Synthetic crash and replacement cases; no live stores or HTTP requests."""

import errno
import json
import os
import shutil
import sqlite3
from contextlib import ExitStack
from dataclasses import replace
from functools import partial
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError
from rich.console import Console
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.paths import UnsafePathError
from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive import source_archive as archive
from sve_carddb.ingest.archive.manifest import (
    AlreadyRunningError,
    ExclusiveLock,
    Kind,
    Link,
    Manifest,
    Outcome,
    RequestStart,
    Resource,
)
from sve_carddb.ingest.archive.source_archive import (
    ArchiveError,
    ArchiveRaceError,
    ArchiveStore,
    IncompleteBatchError,
    backup_batch,
    capacity_report,
    restore_backup,
    seal_batch,
    verify_batch,
)
from sve_carddb.ingest.archive.store import compress, decompress, resolve_within
from sve_carddb.ingest.config import Settings
from sve_carddb.ingest.http import refresh
from sve_carddb.ingest.http.refresh import RefreshWriter, Replacement
from sve_carddb.ingest.http.writer import (
    DiskFullError,
    Fetched,
    PathConflictError,
    Writer,
    sha256,
)

if TYPE_CHECKING:
    from collections.abc import Iterator

URL = "https://example.invalid/qa/one"
PATH = PurePosixPath("raw/jp/qa/one.html.zst")


def fetched(body: bytes = b"synthetic-A", *, compressed: bool = True) -> Fetched:
    return Fetched(
        URL,
        SourceRegion.JP,
        Kind.CARD,
        PATH,
        body,
        "text/html",
        "etag-A",
        None,
        compressed,
    )


def start(manifest: Manifest, url: str = URL) -> int:
    return manifest.requests.start(RequestStart("run", "fetch", 1, 0, url, url, None))


@pytest.fixture
def store(tmp_path: Path) -> ArchiveStore:
    data = tmp_path / "data"
    return ArchiveStore(
        data,
        data / "manifest" / "manifest.sqlite",
        data / "manifest" / ".lock",
        tmp_path / "archive",
        "synthetic",
    )


@pytest.fixture
def writer(store: ArchiveStore, tmp_path: Path) -> Iterator[RefreshWriter]:
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        yield RefreshWriter(
            store,
            manifest,
            lock,
            tmp_path / "backup",
            restore_root=tmp_path / "restore",
            require_separate_device=False,
        )


def write(writer: RefreshWriter, item: Fetched) -> None:
    writer.write(item, request_id=start(writer._manifest))


def test_a_b_c_versions_and_qa_links_restore_offline(
    store: ArchiveStore, writer: RefreshWriter, tmp_path: Path
) -> None:
    bodies = [b"synthetic-A", b"synthetic-B", b"synthetic-C"]

    def links(resource: Resource, index: int) -> None:
        writer._manifest.links.replace(
            resource.url,
            resource.sha256,
            [Link(f"https://example.invalid/qa/{index}", Kind.QA, 0, "synthetic-link")],
        )

    for index, body in enumerate(bodies):
        writer.write(
            fetched(body),
            request_id=start(writer._manifest),
            in_transaction=partial(links, index=index),
        )
    writer.checkpoint()
    assert writer.read(URL) == bodies[-1]
    assert len(list((store.root / "versions").glob("*.json"))) == 3
    assert {path.read_bytes() for path in (store.root / "raw").rglob("*.raw")} == set(
        bodies
    )
    assert len(writer._manifest.links.history(URL)) >= 3
    writer._lock.__exit__(None, None, None)
    result = seal_batch(store)
    assert (result.path / "manifest.sqlite").samefile(
        store.root / "manifests" / f"{result.inventory.manifest.sha256[7:]}.sqlite"
    )
    report = capacity_report(store, result.batch_id)
    assert report.source_versions == 3
    assert report.unique_raw_logical_bytes == sum(map(len, bodies))
    assert result.inventory.history_gaps == []
    backup = tmp_path / "backup"
    backup_batch(store, backup, result.batch_id, require_separate_device=False)
    backup_manifest = (
        backup / "manifests" / f"{result.inventory.manifest.sha256[7:]}.sqlite"
    )
    assert (backup / "batches" / result.batch_id[7:] / "manifest.sqlite").samefile(
        backup_manifest
    )
    assert not backup_manifest.samefile(result.path / "manifest.sqlite")
    expected_links = writer._manifest.links.history(URL)
    store.root.rename(tmp_path / "original-archive-unavailable")
    store.data_root.rename(tmp_path / "original-latest-unavailable")
    restored = tmp_path / "restored"
    restore_backup(backup, restored, store.store_id, result.batch_id)
    restored_manifest = restored / "batches" / result.batch_id[7:] / "manifest.sqlite"
    assert restored_manifest.samefile(
        restored / "manifests" / f"{result.inventory.manifest.sha256[7:]}.sqlite"
    )
    assert not restored_manifest.samefile(backup_manifest)
    assert verify_batch(restored, store.store_id, result.batch_id) == result.inventory
    with Manifest.open_snapshot(
        restored / "batches" / result.batch_id[7:] / "manifest.sqlite"
    ) as manifest:
        assert manifest.historical_raw_hashes() == sorted(
            (URL, sha256(body)) for body in bodies
        )
        assert manifest.links.history(URL) == expected_links
    assert {path.read_bytes() for path in (restored / "raw").rglob("*.raw")} == set(
        bodies
    )


def test_etag_only_304_and_a_b_a_reuse_version(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    writer.checkpoint()
    first_descriptor = next((store.root / "versions").glob("*.json")).read_bytes()
    before = (store.data_root / PATH).stat().st_ino
    result = writer.write(
        replace(fetched(), etag="etag-only"), request_id=start(writer._manifest)
    )
    assert not result.changed
    assert not result.rewritten
    assert result.resource.etag == "etag-only"
    assert (store.data_root / PATH).stat().st_ino == before
    writer.mark_not_modified(URL, request_id=start(writer._manifest))
    assert len(list((store.root / "versions").glob("*.json"))) == 1
    assert (
        next((store.root / "versions").glob("*.json")).read_bytes() == first_descriptor
    )
    writer.checkpoint()
    assert len(list((store.root / "receipts").glob("*.json"))) == 2
    write(writer, fetched(b"synthetic-B"))
    write(writer, fetched())
    assert len(list((store.root / "versions").glob("*.json"))) == 2
    assert first_descriptor in [
        path.read_bytes() for path in (store.root / "versions").glob("*.json")
    ]


@pytest.mark.parametrize(
    "point",
    [
        "before-rename",
        "after-rename",
        "in-transaction",
        "after-commit",
        "before-receipt",
    ],
)
def test_crash_recovers_committed_hash_only(  # ruff: ignore[complex-structure] -- independent crash points share the committed-hash assertions
    writer: RefreshWriter,
    store: ArchiveStore,
    monkeypatch: pytest.MonkeyPatch,
    point: str,
) -> None:
    write(writer, fetched())
    committed = writer._manifest.resources.get(URL)
    assert committed is not None
    original_replace = Path.replace
    original_record = writer._record
    original_observe = writer._backup_committed

    def crash_replace(path: Path, target: Path) -> Path:
        if str(target) == str(store.data_root / PATH):
            if point == "after-rename":
                original_replace(path, target)
            raise KeyboardInterrupt
        return original_replace(path, target)

    def crash_record(
        resource: Resource, request_id: int, unchanged: bool, callback: object
    ) -> None:
        del callback
        original_record(resource, request_id, unchanged, None)
        raise KeyboardInterrupt

    def crash_observe(resource: Resource, intent_id: str) -> None:
        if resource.sha256 != committed.sha256:
            raise KeyboardInterrupt
        original_observe(resource, intent_id)

    def crash_transaction(_resource: Resource) -> None:
        raise KeyboardInterrupt

    if point in {"before-rename", "after-rename"}:
        monkeypatch.setattr(Path, "replace", crash_replace)
    elif point == "after-commit":
        monkeypatch.setattr(writer, "_record", crash_record)
    elif point == "before-receipt":
        monkeypatch.setattr(writer, "_backup_committed", crash_observe)
    with pytest.raises(KeyboardInterrupt):
        writer.write(
            fetched(b"synthetic-B"),
            request_id=start(writer._manifest),
            in_transaction=crash_transaction if point == "in-transaction" else None,
        )
    monkeypatch.undo()
    after_commit = point in {"after-commit", "before-receipt"}
    assert len(list((store.root / "versions").glob("*.json"))) == 1
    assert {path.read_bytes() for path in (store.root / "raw").rglob("*.raw")} == {
        b"synthetic-A",
        b"synthetic-B",
    }
    assert writer.recover() == 1
    assert writer.read(URL) == (b"synthetic-B" if after_commit else b"synthetic-A")
    assert writer._manifest.requests.outcomes(URL)[-1] is (
        Outcome.CHANGED if after_commit else Outcome.STARTED
    )
    assert len(list((store.root / "versions").glob("*.json"))) == (
        2 if after_commit else 1
    )
    assert writer.recover() == 0


@pytest.mark.parametrize("damage", ["missing", "hash", "size", "zstd"])
def test_missing_unarchived_old_raw_blocks_replacement(
    writer: RefreshWriter, store: ArchiveStore, damage: str
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    target = store.data_root / PATH
    if damage == "missing":
        target.unlink()
    elif damage == "hash":
        target.write_bytes(compress(b"synthetic-X"))
    elif damage == "size":
        target.write_bytes(target.read_bytes()[:3])
    else:
        target.write_bytes(b"not-zstd")
    original = target.read_bytes() if target.exists() else None
    with pytest.raises(IncompleteBatchError) as caught:
        write(writer, fetched(b"synthetic-B"))
    assert caught.value.missing[0].expected_raw_sha256 == "sha256:" + sha256(
        b"synthetic-A"
    )
    assert (target.read_bytes() if target.exists() else None) == original
    assert writer._manifest.resources.get(URL) == ordinary._manifest.resources.get(URL)
    assert not list((store.root / "replacements").glob("*.json"))


def test_corrupt_latest_blocks_even_with_existing_blob(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    writer.checkpoint()
    (store.data_root / PATH).write_bytes(compress(b"synthetic-X"))
    with pytest.raises(IncompleteBatchError):
        write(writer, fetched(b"synthetic-B"))
    assert len(list((store.root / "versions").glob("*.json"))) == 1


def test_source_change_during_unlocked_preparation_blocks_replace(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    original = writer._candidate

    def change_latest(
        item: Fetched, previous: Resource | None, stage: Path
    ) -> tuple[Resource, Path]:
        result = original(item, previous, stage)
        (store.data_root / PATH).write_bytes(compress(b"synthetic-X"))
        return result

    monkeypatch.setattr(writer, "_candidate", change_latest)
    with pytest.raises(ArchiveRaceError):
        write(writer, fetched(b"synthetic-B"))
    assert decompress((store.data_root / PATH).read_bytes()) == b"synthetic-X"


def test_manifest_change_during_unlocked_preparation_blocks_replace(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    original = writer._candidate

    def change_manifest(
        item: Fetched, previous: Resource | None, stage: Path
    ) -> tuple[Resource, Path]:
        result = original(item, previous, stage)
        assert previous is not None
        with Manifest.open(store.manifest_path) as live, live.transaction():
            live.resources.put(replace(previous, etag="other-writer"))
        return result

    monkeypatch.setattr(writer, "_candidate", change_manifest)
    with pytest.raises(ArchiveRaceError):
        write(writer, fetched(b"synthetic-B"))
    assert writer.read(URL) == b"synthetic-A"


def test_heavy_preparation_releases_manifest_lock(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = writer._candidate
    calls: list[bool] = []

    def check_lock(
        item: Fetched, previous: Resource | None, stage: Path
    ) -> tuple[Resource, Path]:
        with ExclusiveLock(store.lock_path):
            calls.append(True)
        return original(item, previous, stage)

    monkeypatch.setattr(writer, "_candidate", check_lock)
    write(writer, fetched())
    assert calls == [True]
    with pytest.raises(AlreadyRunningError), ExclusiveLock(store.lock_path):
        pass


def test_archive_disk_full_leaves_old_latest(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    before = (store.data_root / PATH).read_bytes()

    def disk_full(_fd: int) -> None:
        raise OSError(errno.ENOSPC, "synthetic disk full")

    monkeypatch.setattr(os, "fsync", disk_full)
    with pytest.raises(DiskFullError):
        write(writer, fetched(b"synthetic-B"))
    assert (store.data_root / PATH).read_bytes() == before


def test_backup_interruption_blocks_replace(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    original = archive._copy_immutable

    def fail_copy(source: Path, target: Path) -> None:
        if target.is_relative_to(writer.backup_root):
            raise OSError("synthetic backup interruption")
        original(source, target)

    monkeypatch.setattr(archive, "_copy_immutable", fail_copy)
    with pytest.raises(OSError, match="backup interruption"):
        write(writer, fetched(b"synthetic-B"))
    assert writer.read(URL) == b"synthetic-A"


def test_archive_collision_and_symlink_are_refused(
    writer: RefreshWriter, store: ArchiveStore, tmp_path: Path
) -> None:
    write(writer, fetched())
    blob = next((store.root / "raw").rglob("*.raw"))
    body = blob.read_bytes()
    blob.unlink()
    outside = tmp_path / "outside"
    outside.write_bytes(body)
    blob.symlink_to(outside)
    with pytest.raises(ArchiveError, match="symlink"):
        write(writer, fetched(b"synthetic-B"))
    assert writer.read(URL) == b"synthetic-A"


def test_allow_root_does_not_authorize_latest_writes(
    writer: RefreshWriter, store: ArchiveStore, tmp_path: Path
) -> None:
    write(writer, fetched())
    outside = tmp_path / "outside"
    (store.data_root / "raw").rename(outside)
    (store.data_root / "raw").symlink_to(outside)
    writer.store = replace(store, read_roots=(outside,))
    writer._read_roots = (outside,)
    with pytest.raises(UnsafePathError):
        write(writer, fetched(b"synthetic-B"))
    assert decompress((outside / "jp/qa/one.html.zst").read_bytes()) == b"synthetic-A"


def test_new_uncompressed_image_uses_new_inode(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    image = replace(
        fetched(b"synthetic-image-A", compressed=False),
        kind=Kind.IMAGE,
        path=PurePosixPath("media/one.png"),
        content_type="image/png",
    )
    write(writer, image)
    old = (store.data_root / image.path).stat().st_ino
    write(writer, replace(image, body=b"synthetic-image-B"))
    assert (store.data_root / image.path).stat().st_ino != old
    assert {path.read_bytes() for path in (store.root / "raw").rglob("*.raw")} == {
        b"synthetic-image-A",
        b"synthetic-image-B",
    }


def test_backup_configuration_requires_independent_device(
    store: ArchiveStore, tmp_path: Path
) -> None:
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        with pytest.raises(ArchiveError, match="separate device"):
            RefreshWriter(store, manifest, lock, tmp_path / "backup")
        with pytest.raises(ArchiveError, match="outside"):
            RefreshWriter(
                store,
                manifest,
                lock,
                store.root / "backup",
                require_separate_device=False,
            )


def test_missing_historical_raw_is_explicit(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(b"lost-prearchive"), request_id=start(writer._manifest))
    ordinary.write(fetched(), request_id=start(writer._manifest))
    write(writer, fetched(b"synthetic-B"))
    writer._lock.__exit__(None, None, None)
    batch = seal_batch(store)
    assert batch.inventory.history_gaps == [
        archive.Missing(
            url=URL,
            expected_raw_sha256="sha256:" + sha256(b"lost-prearchive"),
            reason="missing_history",
        )
    ]


def test_finish_refresh_runs_seal_backup_restore_and_capacity(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    writer._lock.__exit__(None, None, None)
    original = backup_batch

    def same_device_test_backup(
        store: ArchiveStore, root: Path, batch_id: str
    ) -> archive.BackupReceipt:
        return original(store, root, batch_id, require_separate_device=False)

    monkeypatch.setattr(cli, "backup_batch", same_device_test_backup)
    cli._finish_refresh(writer)
    assert list((writer.backup_root / "restore-checks").glob("*.json"))


def test_cross_device_old_copy_is_verified_before_replace(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    image = replace(
        fetched(b"synthetic-image-A", compressed=False),
        kind=Kind.IMAGE,
        path=PurePosixPath("media/one.png"),
    )
    ordinary.write(image, request_id=start(writer._manifest))
    original_link = os.link

    def cross_device(source: Path, target: Path) -> None:
        if target.parent.name == "pins":
            raise OSError(errno.EXDEV, "synthetic cross device")
        original_link(source, target)

    monkeypatch.setattr(os, "link", cross_device)
    monkeypatch.setattr(archive, "_try_reflink", lambda _fd, _target: False)
    write(writer, replace(image, body=b"synthetic-image-B"))
    assert {path.read_bytes() for path in (store.root / "raw").rglob("*.raw")} == {
        b"synthetic-image-A",
        b"synthetic-image-B",
    }


def test_corrupt_prepared_latest_blocks_replace(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    original = writer._check_unchanged

    def damage_temporary(
        snapshot: tuple[Path, str],
        stage: Path,
        pin: archive._Pinned | None,
        item: Fetched,
        previous: Resource | None,
    ) -> None:
        original(snapshot, stage, pin, item, previous)
        for path in store.data_root.rglob("*.tmp-*"):
            path.write_bytes(b"broken-candidate")

    monkeypatch.setattr(writer, "_check_unchanged", damage_temporary)
    with pytest.raises(ArchiveRaceError, match="prepared latest"):
        write(writer, fetched(b"synthetic-B"))
    assert writer.read(URL) == b"synthetic-A"


def test_reacquire_conflict_stops_without_replace(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    original = writer._candidate
    other = ExitStack()

    def take_lock(
        item: Fetched, previous: Resource | None, stage: Path
    ) -> tuple[Resource, Path]:
        result = original(item, previous, stage)
        other.enter_context(ExclusiveLock(store.lock_path))
        return result

    monkeypatch.setattr(writer, "_candidate", take_lock)
    try:
        with pytest.raises(AlreadyRunningError):
            write(writer, fetched(b"synthetic-B"))
        assert writer.read(URL) == b"synthetic-A"
    finally:
        other.close()


def test_formal_receipt_requires_matching_committed_snapshot(
    writer: RefreshWriter, tmp_path: Path
) -> None:
    write(writer, fetched())
    current = writer._manifest.resources.get(URL)
    assert current is not None
    snapshot = writer._snapshot(tmp_path, "snapshot.sqlite")
    with pytest.raises(ArchiveError, match="observation differs"):
        writer._observe(replace(current, sha256=sha256(b"synthetic-B")), snapshot)


@pytest.mark.parametrize("damage", ["missing", "hash", "size"])
def test_recovery_rejects_unverifiable_old_blob(
    writer: RefreshWriter,
    store: ArchiveStore,
    monkeypatch: pytest.MonkeyPatch,
    damage: str,
) -> None:
    write(writer, fetched())
    original = writer._record

    def before_commit(
        _resource: Resource, _request_id: int, _unchanged: bool, _callback: object
    ) -> None:
        raise KeyboardInterrupt

    monkeypatch.setattr(writer, "_record", before_commit)
    with pytest.raises(KeyboardInterrupt):
        write(writer, fetched(b"synthetic-B"))
    monkeypatch.setattr(writer, "_record", original)
    blob = store.root / archive._blob_path("sha256:" + sha256(b"synthetic-A"))
    if damage == "missing":
        blob.unlink()
    else:
        blob.write_bytes(b"synthetic-X" if damage == "hash" else b"short")
    with pytest.raises(ArchiveError):
        writer.recover()
    assert decompress((store.data_root / PATH).read_bytes()) == b"synthetic-B"
    assert writer._manifest.requests.outcomes(URL)[-1] is Outcome.STARTED


def test_new_source_crash_does_not_promote_candidate(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    def before_commit(
        _resource: Resource, _request_id: int, _unchanged: bool, _callback: object
    ) -> None:
        raise KeyboardInterrupt

    monkeypatch.setattr(writer, "_record", before_commit)
    with pytest.raises(KeyboardInterrupt):
        write(writer, fetched())
    assert writer.recover() == 1
    assert writer._manifest.resources.get(URL) is None
    assert not list((store.root / "versions").glob("*.json"))
    assert next((store.root / "raw").rglob("*.raw")).read_bytes() == b"synthetic-A"


def test_metadata_install_is_atomic_and_never_overwrites(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    target = tmp_path / "metadata" / "record.json"
    original = archive._write_new

    def interrupted(path: Path, data: bytes) -> None:
        if path.suffix == ".tmp":
            path.write_bytes(data[:2])
            raise KeyboardInterrupt
        original(path, data)

    monkeypatch.setattr(archive, "_write_new", interrupted)
    with pytest.raises(KeyboardInterrupt):
        archive._install_bytes(target, b"synthetic-metadata")
    assert not target.exists()
    monkeypatch.undo()
    archive._install_bytes(target, b"synthetic-metadata")
    with pytest.raises(ArchiveError, match="differs"):
        archive._install_bytes(target, b"other-metadata")
    assert target.read_bytes() == b"synthetic-metadata"


def test_archive_mkdir_does_not_write_through_symlink(tmp_path: Path) -> None:
    outside = tmp_path / "outside"
    outside.mkdir()
    alias = tmp_path / "alias"
    alias.symlink_to(outside)
    with pytest.raises(ArchiveError, match="symlink"):
        archive._mkdir_safe(alias / "unexpected" / "child")
    assert list(outside.iterdir()) == []


@pytest.mark.parametrize(
    "field", ["archive_root", "archive_backup_root", "archive_restore_root"]
)
def test_archive_settings_require_absolute_paths(tmp_path: Path, field: str) -> None:
    with pytest.raises(ValidationError, match="absolute"):
        Settings.model_validate({"data_dir": tmp_path, field: Path("relative")})


@pytest.mark.parametrize("mode", ["refresh", "resume", "repair"])
def test_configured_cli_refresh_uses_protection_and_finishes_backup(
    store: ArchiveStore, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mode: str
) -> None:
    monkeypatch.setenv("FORCE_COLOR", "1")
    monkeypatch.setenv("TERM", "xterm-256color")
    monkeypatch.delenv("NO_COLOR", raising=False)
    monkeypatch.setattr(cli, "console", Console(force_terminal=True, soft_wrap=True))
    monkeypatch.setenv("SVE_DATA_DIR", str(store.data_root))
    monkeypatch.setenv("SVE_ARCHIVE_ROOT", str(store.root))
    monkeypatch.setenv("SVE_ARCHIVE_STORE_ID", store.store_id)
    monkeypatch.setenv("SVE_ARCHIVE_BACKUP_ROOT", str(tmp_path / "backup"))
    monkeypatch.setenv("SVE_ARCHIVE_RESTORE_ROOT", str(tmp_path / "restore"))
    original = cli._refresh_writer

    def temporary_writer(
        settings: Settings, manifest: Manifest, lock: ExclusiveLock
    ) -> RefreshWriter:
        del settings
        return RefreshWriter(
            store,
            manifest,
            lock,
            tmp_path / "backup",
            restore_root=tmp_path / "restore",
            require_separate_device=False,
        )

    async def fake_crawl(  # ruff: ignore[unused-async] -- matches the CLI async crawl boundary
        _job: cli.Job, _settings: Settings, _manifest: Manifest, writer: Writer
    ) -> int:
        assert isinstance(writer, RefreshWriter)
        write(writer, fetched())
        write(writer, fetched(b"synthetic-B"))
        return 0

    def temporary_backup(
        store: ArchiveStore, root: Path, batch_id: str
    ) -> archive.BackupReceipt:
        return backup_batch(store, root, batch_id, require_separate_device=False)

    monkeypatch.setattr(cli, "_refresh_writer", temporary_writer)
    monkeypatch.setattr(cli, "_crawl", fake_crawl)
    monkeypatch.setattr(cli, "backup_batch", temporary_backup)
    result = CliRunner().invoke(
        cli.app, ["crawl", "p2", "--mode", mode, "--set", "SYNTHETIC"]
    )
    assert result.exit_code == 0, result.output
    assert '"source_versions": 2' in result.output
    report_json = result.stdout[result.stdout.index("{") :]
    assert "\x1b" not in report_json
    assert archive.CapacityReport.model_validate_json(report_json).source_versions == 2
    assert list((tmp_path / "backup" / "restore-checks").glob("*.json"))
    monkeypatch.setattr(cli, "_refresh_writer", original)


def test_partial_archive_configuration_stops_before_manifest(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setenv("SVE_DATA_DIR", str(tmp_path / "data"))
    monkeypatch.setenv("SVE_ARCHIVE_ROOT", str(tmp_path / "archive"))
    monkeypatch.delenv("SVE_ARCHIVE_STORE_ID", raising=False)
    monkeypatch.delenv("SVE_ARCHIVE_BACKUP_ROOT", raising=False)
    result = CliRunner().invoke(cli.app, ["crawl", "p0"])
    assert result.exit_code == 1
    assert not (tmp_path / "data" / "manifest" / "manifest.sqlite").exists()


def test_same_raw_different_url_has_distinct_source_versions(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    other = replace(
        fetched(),
        url="https://example.invalid/qa/two",
        path=PurePosixPath("raw/jp/qa/two.html.zst"),
    )
    writer.write(other, request_id=start(writer._manifest, other.url))
    writer.checkpoint()
    assert len(list((store.root / "versions").glob("*.json"))) == 2
    assert len(list((store.root / "raw").rglob("*.raw"))) == 1


def test_suspending_unheld_manifest_lock_is_rejected(tmp_path: Path) -> None:
    with (
        pytest.raises(AlreadyRunningError),
        ExclusiveLock(tmp_path / "lock").suspended(),
    ):
        pass


def test_semantic_hash_does_not_replace_exact_raw_hash(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    bodies = [b"synthetic text", b"synthetic  text"]
    observation_hashes = [sha256(b" ".join(body.split())) for body in bodies]
    assert observation_hashes[0] == observation_hashes[1]
    for body in bodies:
        write(writer, fetched(body))
    writer.checkpoint()
    assert len(list((store.root / "versions").glob("*.json"))) == 2
    assert {path.read_bytes() for path in (store.root / "raw").rglob("*.raw")} == set(
        bodies
    )


def test_not_modified_metadata_preparation_renews_sqlite_read_view(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    original = writer._preserve_old

    def concurrent_metadata(
        pin: archive._Pinned | None, snapshot: tuple[Path, str]
    ) -> None:
        original(pin, snapshot)
        assert pin is not None
        with Manifest.open(store.manifest_path) as live, live.transaction():
            live.resources.put(replace(pin.resource, etag="concurrent-etag"))

    monkeypatch.setattr(writer, "_preserve_old", concurrent_metadata)
    writer.mark_not_modified(URL, request_id=start(writer._manifest))
    current = writer._manifest.resources.get(URL)
    assert current is not None
    assert current.etag == "concurrent-etag"
    monkeypatch.undo()
    write(writer, fetched(b"synthetic-B"))


def test_failed_sqlite_commit_rolls_back_before_recovery(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    previous = writer._manifest.resources.get(URL)
    connection = writer._manifest._conn

    class FailedCommit:
        def commit(self) -> None:
            raise sqlite3.OperationalError("synthetic commit failure")

        def rollback(self) -> None:
            connection.rollback()

    def fail_commit(_resource: Resource) -> None:
        monkeypatch.setattr(writer._manifest, "_conn", FailedCommit())

    with pytest.raises(sqlite3.OperationalError, match="commit failure"):
        writer.write(
            fetched(b"synthetic-B"),
            request_id=start(writer._manifest),
            in_transaction=fail_commit,
        )
    monkeypatch.undo()
    assert writer._manifest.resources.get(URL) == previous
    assert writer.recover() == 1
    assert writer.read(URL) == b"synthetic-A"


def test_recovery_preserves_only_copy_when_candidate_blob_is_missing(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())

    def before_commit(
        _resource: Resource, _request_id: int, _unchanged: bool, _callback: object
    ) -> None:
        raise KeyboardInterrupt

    monkeypatch.setattr(writer, "_record", before_commit)
    with pytest.raises(KeyboardInterrupt):
        write(writer, fetched(b"synthetic-B"))
    (store.root / archive._blob_path("sha256:" + sha256(b"synthetic-B"))).unlink()
    with pytest.raises(ArchiveError, match="missing"):
        writer.recover()
    assert decompress((store.data_root / PATH).read_bytes()) == b"synthetic-B"


@pytest.mark.parametrize("damage", ["missing", "corrupt"])
def test_identical_refetch_repairs_missing_prearchive_version(
    writer: RefreshWriter, store: ArchiveStore, damage: str
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    target = store.data_root / PATH
    if damage == "missing":
        target.unlink()
    else:
        target.write_bytes(b"broken-zstd")
    writer.write(fetched(), request_id=start(writer._manifest))
    assert writer.read(URL) == b"synthetic-A"
    assert len(list((store.root / "versions").glob("*.json"))) == 1
    write(writer, fetched(b"synthetic-B"))
    writer.checkpoint()
    assert len(list((store.root / "versions").glob("*.json"))) == 2


def test_explicit_gap_keeps_historical_reference_and_reports_missing_history(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    (store.data_root / PATH).unlink()
    with pytest.raises(IncompleteBatchError):
        write(writer, fetched(b"synthetic-B"))
    writer.acknowledge_gap(
        URL, "sha256:" + sha256(b"synthetic-A"), "independent backups checked"
    )
    write(writer, fetched(b"synthetic-B"))
    writer._lock.__exit__(None, None, None)
    result = seal_batch(store)
    assert [
        (gap.url, gap.expected_raw_sha256) for gap in result.inventory.history_gaps
    ] == [(URL, "sha256:" + sha256(b"synthetic-A"))]
    verify_batch(store.root, store.store_id, result.batch_id)
    assert (URL, sha256(b"synthetic-A")) in writer._manifest.historical_raw_hashes()


def test_gap_cannot_override_available_or_already_archived_version(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    with pytest.raises(ArchiveError, match="available"):
        writer.acknowledge_gap(URL, "sha256:" + sha256(b"synthetic-A"), "operator")
    writer.checkpoint()
    (store.data_root / PATH).unlink()
    with pytest.raises(ArchiveError, match="archived"):
        writer.acknowledge_gap(URL, "sha256:" + sha256(b"synthetic-A"), "operator")


def test_many_urls_share_before_and_final_manifests(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    for index in range(12):
        item = replace(
            fetched(),
            url=f"https://example.invalid/{index}",
            path=PurePosixPath(f"raw/jp/qa/{index}.html.zst"),
        )
        writer.write(item, request_id=start(writer._manifest, item.url))
    assert len(list((store.root / "manifests").glob("*.sqlite"))) == 1
    assert len(list((writer.backup_root / "manifests").glob("*.sqlite"))) == 1
    assert len(list((writer.backup_root / "replacement-commits").glob("*.json"))) == 12
    writer.checkpoint()
    assert len(list((store.root / "manifests").glob("*.sqlite"))) == 2
    assert len(list((writer.backup_root / "manifests").glob("*.sqlite"))) == 2
    before = len(list((store.root / "manifests").glob("*.sqlite")))
    for _ in range(12):
        writer.mark_not_modified(
            "https://example.invalid/0",
            request_id=start(writer._manifest, "https://example.invalid/0"),
        )
    assert len(list((store.root / "manifests").glob("*.sqlite"))) == before

    assert len(list((store.root / "observations").glob("*.json"))) == 24


def test_unarchived_old_urls_share_one_preparation_snapshot(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    items = [
        replace(
            fetched(),
            url=f"https://example.invalid/{index}",
            path=PurePosixPath(f"raw/jp/qa/{index}.html.zst"),
        )
        for index in range(8)
    ]
    for item in items:
        ordinary.write(item, request_id=start(writer._manifest, item.url))
    for item in items:
        writer.write(
            replace(item, body=b"synthetic-B"),
            request_id=start(writer._manifest, item.url),
        )
    assert len(list((store.root / "manifests").glob("*.sqlite"))) == 1
    writer.checkpoint()
    assert len(list((store.root / "manifests").glob("*.sqlite"))) == 2
    assert len(list((store.root / "versions").glob("*.json"))) == 16


@pytest.mark.parametrize("prior_write", [False, True])
def test_unarchived_304s_share_exact_pre_update_evidence(
    writer: RefreshWriter, store: ArchiveStore, *, prior_write: bool
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    items = [
        replace(
            fetched(),
            url=f"https://example.invalid/{index}",
            path=PurePosixPath(f"raw/jp/qa/{index}.html.zst"),
        )
        for index in range(3)
    ]
    previous = {}
    for item in items:
        result = ordinary.write(item, request_id=start(writer._manifest, item.url))
        previous[item.url] = result.resource
    if prior_write:
        write(writer, fetched(b"synthetic-unrelated"))
    for item in items:
        updated = writer.mark_not_modified(
            item.url, request_id=start(writer._manifest, item.url)
        )
        assert updated.last_checked_at > previous[item.url].last_checked_at
    snapshots = list((store.root / "manifests").glob("*.sqlite"))
    assert len(snapshots) == 1
    assert len(list((writer.backup_root / "manifests").glob("*.sqlite"))) == 1
    with Manifest.open_snapshot(snapshots[0]) as frozen:
        for url, resource in previous.items():
            assert frozen.resources.get(url) == resource
    receipts = [
        archive._load_model(archive.Receipt, path)
        for path in (store.root / "receipts").glob("*.json")
    ]
    assert {receipt.resource.url for receipt in receipts} == set(previous)
    for receipt in receipts:
        assert receipt.resource == archive._evidence(previous[receipt.resource.url])
        assert receipt.manifest_sha256 == "sha256:" + snapshots[0].stem
    writer.checkpoint()
    assert len(list((store.root / "manifests").glob("*.sqlite"))) == 2
    assert len(list((store.root / "versions").glob("*.json"))) == 3 + prior_write


def test_gap_rejects_an_orphan_old_blob(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    (store.data_root / PATH).unlink()
    raw_hash = "sha256:" + sha256(b"synthetic-A")
    blob = store.root / archive._blob_path(raw_hash)
    blob.parent.mkdir(parents=True, exist_ok=True)
    blob.write_bytes(b"synthetic-A")
    assert not list((store.root / "versions").glob("*.json"))
    with pytest.raises(ArchiveError, match="preserved old blob"):
        writer.acknowledge_gap(URL, raw_hash, "operator")
    assert not list((store.root / "history-gaps").glob("*.json"))


def test_recovery_cleans_abandoned_staging(writer: RefreshWriter) -> None:
    abandoned = writer.store.root / "staging" / "abandoned"
    abandoned.mkdir(parents=True)
    (abandoned / "uncommitted.sqlite").write_bytes(b"synthetic-staging")
    assert writer.recover() == 0
    assert not list(abandoned.parent.iterdir())


def test_recovery_retains_committed_candidate_encoding(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    def interrupted(_resource: Resource, _intent_id: str) -> None:
        raise KeyboardInterrupt

    monkeypatch.setattr(writer, "_backup_committed", interrupted)
    with pytest.raises(KeyboardInterrupt):
        write(writer, fetched())
    target = store.data_root / PATH
    encoded = target.read_bytes()
    target.unlink()
    monkeypatch.undo()
    monkeypatch.setattr(refresh, "compress", lambda raw: compress(raw) + b"longer")
    assert writer.recover() == 1
    assert target.read_bytes() == encoded
    assert writer.read(URL) == b"synthetic-A"
    intent = archive._load_model(
        Replacement, next((store.root / "replacements").glob("*.json"))
    )
    assert intent.proposed_stored_sha256 == digest(encoded)


def test_finish_batch_leaves_uncovered_observations_pending(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    other = replace(
        fetched(),
        region=SourceRegion.EN,
        url="https://example.invalid/other",
        path=PurePosixPath("raw/en/other.html.zst"),
    )
    writer.write(other, request_id=start(writer._manifest, other.url))
    writer._lock.__exit__(None, None, None)
    result = seal_batch(store, scope=[archive.Scope(provider="jp", kind="card")])
    writer.finish_batch(result)
    writer._unfinished_observations()
    assert writer.touched == {("en", "card")}
    assert len(list((store.root / "observation-seals").glob("*.json"))) == 1


@pytest.mark.parametrize("root", [None, "overlap"])
def test_finish_refresh_validates_its_restore_boundary(
    writer: RefreshWriter, root: str | None
) -> None:
    writer.touched.add(("jp", "card"))
    writer.restore_root = None if root is None else writer.store.root
    with pytest.raises(ArchiveError, match=r"SVE_ARCHIVE_RESTORE_ROOT|separate"):
        cli._finish_refresh(writer)
    assert not list((writer.store.root / "batches").glob("*"))


@pytest.mark.parametrize("damage", ["symlink", "corrupt"])
def test_backup_rejects_an_unsafe_manifest_alias(
    writer: RefreshWriter, store: ArchiveStore, *, damage: str
) -> None:
    write(writer, fetched())
    writer._lock.__exit__(None, None, None)
    result = seal_batch(store)
    target = writer.backup_root / "batches" / result.batch_id[7:] / "manifest.sqlite"
    target.parent.mkdir(parents=True)
    if damage == "symlink":
        target.symlink_to(result.path / "manifest.sqlite")
    else:
        target.write_bytes(b"corrupted-manifest")
    with pytest.raises(ArchiveError):
        backup_batch(
            store, writer.backup_root, result.batch_id, require_separate_device=False
        )
    assert not list((writer.backup_root / "backups").glob("*.json"))


def test_finish_refresh_ignores_unrelated_missing_scope(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    other = replace(
        fetched(),
        region=SourceRegion.JP,
        kind=Kind.IMAGE,
        url="https://example.invalid/other",
        path=PurePosixPath("media/other.png"),
        compressed=False,
    )
    ordinary.write(other, request_id=start(writer._manifest, other.url))
    (store.data_root / other.path).unlink()
    write(writer, fetched())
    writer._lock.__exit__(None, None, None)
    monkeypatch.setattr(
        cli, "backup_batch", partial(backup_batch, require_separate_device=False)
    )
    cli._finish_refresh(writer)
    batches = list((store.root / "batches").iterdir())
    assert len(batches) == 1
    inventory = verify_batch(store.root, store.store_id, "sha256:" + batches[0].name)
    assert inventory.scope == [archive.Scope(provider="jp", kind="card")]
    assert [item.url for item in inventory.current] == [URL]
    assert writer.restore_root is not None
    assert not list(writer.restore_root.iterdir())


def test_restore_low_space_stops_before_copy(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    writer._lock.__exit__(None, None, None)
    monkeypatch.setattr(
        cli, "backup_batch", partial(backup_batch, require_separate_device=False)
    )
    assert writer.restore_root is not None
    usage = shutil.disk_usage(writer.restore_root)
    monkeypatch.setattr(shutil, "disk_usage", lambda _root: usage._replace(free=0))
    with pytest.raises(ArchiveError, match="free bytes"):
        cli._finish_refresh(writer)
    assert not list((writer.backup_root / "restore-checks").glob("*.json"))


def test_restart_remembers_committed_observations_until_batch_is_restored(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    writer.touched.clear()
    assert writer.recover() == 0
    assert writer.touched == {("jp", "card")}
    writer._lock.__exit__(None, None, None)
    monkeypatch.setattr(
        cli, "backup_batch", partial(backup_batch, require_separate_device=False)
    )
    cli._finish_refresh(writer)
    assert writer.touched == set()
    with ExclusiveLock(store.lock_path):
        writer._unfinished_observations()
    assert writer.touched == set()


def interrupt_before_commit(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    def interrupted(
        _resource: Resource, _request_id: int, _unchanged: bool, _callback: object
    ) -> None:
        raise KeyboardInterrupt

    monkeypatch.setattr(writer, "_record", interrupted)
    with pytest.raises(KeyboardInterrupt):
        write(writer, fetched(b"synthetic-B"))
    monkeypatch.undo()


def test_recovery_preserves_unexpected_latest_bytes(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    interrupt_before_commit(writer, monkeypatch)
    target = store.data_root / PATH
    target.write_bytes(compress(b"synthetic-UNIQUE"))
    with pytest.raises(ArchiveError, match="outside the replacement history"):
        writer.recover()
    assert decompress(target.read_bytes()) == b"synthetic-UNIQUE"


def test_recovery_rejects_unarchived_third_committed_version(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    interrupt_before_commit(writer, monkeypatch)
    current = writer._manifest.resources.get(URL)
    assert current is not None
    raw = b"synthetic-C"
    target = store.data_root / PATH
    target.write_bytes(compress(raw))
    blob = store.root / archive._blob_path("sha256:" + sha256(raw))
    blob.parent.mkdir(parents=True, exist_ok=True)
    blob.write_bytes(raw)
    with writer._manifest.transaction():
        writer._manifest.resources.put(
            replace(
                current,
                sha256=sha256(raw),
                raw_bytes=len(raw),
                stored_bytes=target.stat().st_size,
            )
        )
    with pytest.raises(ArchiveError, match="committed version is outside"):
        writer.recover()
    assert decompress(target.read_bytes()) == raw


@pytest.mark.parametrize("side", ["source", "target"])
def test_immutable_backup_rejects_file_symlinks(tmp_path: Path, side: str) -> None:
    source, target, unrelated = [
        tmp_path / name for name in ("source", "target", "unrelated")
    ]
    unrelated.write_bytes(b"same")
    source.write_bytes(b"same")
    alias = source if side == "source" else target
    alias.unlink(missing_ok=True)
    alias.symlink_to(unrelated)
    with pytest.raises(ArchiveError, match="symlink"):
        archive._copy_immutable(source, target)
    assert unrelated.read_bytes() == b"same"


def test_ordinary_unchanged_200_updates_conditional_headers(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    result = ordinary.write(
        replace(fetched(), etag="next", last_modified="new-date"),
        request_id=start(writer._manifest),
    )
    assert not result.rewritten
    assert result.resource.etag == "next"
    assert result.resource.last_modified == "new-date"


def test_final_path_owner_recheck_prevents_collision(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = writer._manifest.resources.path_owner
    calls = 0

    def owner(path: PurePosixPath) -> str | None:
        nonlocal calls
        calls += 1
        return original(path) if calls == 1 else "https://example.invalid/other-owner"

    monkeypatch.setattr(writer._manifest.resources, "path_owner", owner)
    with pytest.raises(PathConflictError, match="already belongs"):
        write(writer, fetched())
    assert writer._manifest.resources.get(URL) is None


def test_corrupt_reused_candidate_blob_stops_before_commit(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    previous = writer._manifest.resources.get(URL)
    blob = store.root / archive._blob_path("sha256:" + sha256(b"synthetic-B"))
    blob.parent.mkdir(parents=True, exist_ok=True)
    blob.write_bytes(b"synthetic-X")
    with pytest.raises(ArchiveError):
        write(writer, fetched(b"synthetic-B"))
    assert writer._manifest.resources.get(URL) == previous


def pending_intent(store: ArchiveStore) -> Path:
    return next(
        path
        for path in (store.root / "replacements").glob("*.json")
        if not (store.root / "replacement-completions" / path.name).exists()
    )


def rewrite_intent(path: Path, changes: dict[str, object]) -> None:
    data = json.loads(path.read_bytes())
    data.update(changes)
    body = canonical(data)
    path.with_name(digest(body)[7:] + ".json").write_bytes(body)
    path.unlink()


def test_intent_must_match_previous_snapshot(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    interrupt_before_commit(writer, monkeypatch)
    path = pending_intent(store)
    intent = archive._load_model(Replacement, path)
    assert intent.previous is not None
    forged = intent.previous.model_dump(mode="json")
    forged["etag"] = "forged"
    rewrite_intent(path, {"previous": forged})
    with pytest.raises(ArchiveError, match="differs from its manifest snapshot"):
        writer.recover()


def test_completed_intent_marker_content_is_checked(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    marker = next((store.root / "replacement-completions").glob("*.json"))
    marker.write_text('{"replacement_sha256":"wrong"}')
    with pytest.raises(ArchiveError, match="completion mismatch"):
        writer.recover()


def test_recovery_rechecks_live_manifest_after_preparation(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    interrupt_before_commit(writer, monkeypatch)
    original = writer._prepare_restore

    def concurrent(current: Resource) -> Path | None:
        result = original(current)
        with Manifest.open(store.manifest_path) as live, live.transaction():
            live.resources.put(replace(current, etag="another-crawler"))
        return result

    monkeypatch.setattr(writer, "_prepare_restore", concurrent)
    with pytest.raises(ArchiveRaceError, match="manifest changed during recovery"):
        writer.recover()


def test_final_destination_recheck(
    writer: RefreshWriter,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    original = resolve_within
    calls = 0

    def resolve(root: Path, path: PurePosixPath) -> Path:
        nonlocal calls
        calls += 1
        return original(root, path) if calls == 1 else tmp_path / "unexpected.html.zst"

    monkeypatch.setattr(refresh, "resolve_within", resolve)
    with pytest.raises(ArchiveRaceError, match="destination changed"):
        write(writer, fetched())
    assert writer._manifest.resources.get(URL) is None
    assert not (tmp_path / "unexpected.html.zst").exists()


def test_postcommit_new_raw_is_backed_up_before_return(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    write(writer, fetched())
    blob = writer.backup_root / archive._blob_path("sha256:" + sha256(b"synthetic-A"))
    assert blob.read_bytes() == b"synthetic-A"
    assert list((writer.backup_root / "replacement-commits").glob("*.json"))
    writer.checkpoint()
    descriptor = archive._load_model(
        archive.Descriptor, next((store.root / "versions").glob("*.json"))
    )
    entry = archive.Entry(
        source_version_id=descriptor.id,
        receipt_id=descriptor.first_receipt_id,
        descriptor_sha256=digest(archive._canonical_model(descriptor)),
        blob=archive.Blob(
            store_id=store.store_id,
            path=str(archive._blob_path(descriptor.raw_sha256)),
            sha256=descriptor.raw_sha256,
            bytes=descriptor.raw_bytes,
        ),
    )
    archive._verify_entry(writer.backup_root, store.store_id, entry, set(), set())


def test_backup_entry_is_verified_after_copy(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    original = archive._copy_immutable

    def corrupt(source: Path, target: Path) -> None:
        original(source, target)
        if target.suffix == ".raw":
            target.write_bytes(b"synthetic-X")

    monkeypatch.setattr(archive, "_copy_immutable", corrupt)
    with pytest.raises(ArchiveError, match="hash or size mismatch"):
        writer.checkpoint()


def test_legacy_recovery_requires_matching_encoded_length(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    interrupt_before_commit(writer, monkeypatch)
    rewrite_intent(
        pending_intent(store),
        {"previous_stored_sha256": None, "proposed_stored_sha256": None},
    )
    monkeypatch.setattr(refresh, "compress", lambda raw: compress(raw) + b"longer")
    with pytest.raises(ArchiveError, match="encoding differs"):
        writer.recover()


def test_new_recovery_uses_original_encoding_after_codec_changes(
    writer: RefreshWriter, monkeypatch: pytest.MonkeyPatch
) -> None:
    write(writer, fetched())
    interrupt_before_commit(writer, monkeypatch)
    monkeypatch.setattr(refresh, "compress", lambda raw: compress(raw) + b"longer")
    assert writer.recover() == 1
    assert writer.read(URL) == b"synthetic-A"


def test_plain_raw_needs_no_duplicate_encoding(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    item = replace(
        fetched(),
        compressed=False,
        path=PurePosixPath("media/synthetic.png"),
        kind=Kind.IMAGE,
    )
    write(writer, item)
    writer.checkpoint()
    result = writer.write(item, request_id=start(writer._manifest))
    assert not result.changed
    assert not result.rewritten
    assert not list((store.root / "encodings").glob("*.bin"))


def test_gap_cli_records_exception_with_complete_configuration(
    writer: RefreshWriter,
    store: ArchiveStore,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    (store.data_root / PATH).unlink()
    writer._lock.__exit__(None, None, None)
    monkeypatch.setenv("SVE_DATA_DIR", str(store.data_root))
    monkeypatch.setenv("SVE_ARCHIVE_ROOT", str(store.root))
    monkeypatch.setenv("SVE_ARCHIVE_STORE_ID", store.store_id)
    monkeypatch.setenv("SVE_ARCHIVE_BACKUP_ROOT", str(tmp_path / "backup"))
    monkeypatch.setenv("SVE_ARCHIVE_RESTORE_ROOT", str(tmp_path / "restore"))

    def temporary_writer(
        settings: Settings, manifest: Manifest, lock: ExclusiveLock
    ) -> RefreshWriter:
        assert settings.archive_restore_root is not None
        return RefreshWriter(
            store,
            manifest,
            lock,
            tmp_path / "backup",
            restore_root=settings.archive_restore_root,
            require_separate_device=False,
        )

    monkeypatch.setattr(cli, "_refresh_writer", temporary_writer)
    result = CliRunner().invoke(
        cli.app,
        [
            "archive",
            "acknowledge-gap",
            URL,
            "--expected-hash",
            "sha256:" + sha256(b"synthetic-A"),
            "--reason",
            "independent backups checked",
        ],
    )
    assert result.exit_code == 0, result.output
    assert "history remains incomplete" in result.output
    assert len(list((store.root / "history-gaps").glob("*.json"))) == 1
    assert len(list((tmp_path / "backup/history-gaps").glob("*.json"))) == 1


def test_gap_cannot_discard_unreferenced_resource_or_accept_empty_reason(
    writer: RefreshWriter, store: ArchiveStore
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    result = ordinary.write(fetched(), request_id=start(writer._manifest))
    (store.data_root / PATH).unlink()
    with pytest.raises(ArchiveError, match="operator reason"):
        writer.acknowledge_gap(URL, "sha256:" + result.resource.sha256, "  ")
    with pytest.raises(ArchiveError, match="does not match"):
        writer.acknowledge_gap(URL, "sha256:" + sha256(b"synthetic-X"), "operator")
    other = replace(
        result.resource,
        url="https://example.invalid/unreferenced",
        path=PurePosixPath("raw/unreferenced.zst"),
    )
    with writer._manifest.transaction():
        writer._manifest.resources.put(other)
    with pytest.raises(ArchiveError, match="persistent manifest reference"):
        writer.acknowledge_gap(other.url, "sha256:" + other.sha256, "operator")


@pytest.mark.parametrize("location", ["latest", "blob"])
def test_found_old_bytes_override_a_previously_acknowledged_gap(
    writer: RefreshWriter, store: ArchiveStore, location: str
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    target = store.data_root / PATH
    target.unlink()
    writer.acknowledge_gap(
        URL, "sha256:" + sha256(b"synthetic-A"), "independent backups checked"
    )
    if location == "latest":
        target.write_bytes(compress(b"synthetic-A"))
    else:
        blob = store.root / archive._blob_path("sha256:" + sha256(b"synthetic-A"))
        blob.parent.mkdir(parents=True, exist_ok=True)
        blob.write_bytes(b"synthetic-A")
    write(writer, fetched(b"synthetic-B"))
    writer.checkpoint()
    assert len(list((store.root / "versions").glob("*.json"))) == 2
    writer._lock.__exit__(None, None, None)
    result = seal_batch(store)
    assert result.inventory.history_gaps == []
    assert (
        writer.backup_root / archive._blob_path("sha256:" + sha256(b"synthetic-A"))
    ).read_bytes() == b"synthetic-A"


def test_recovery_uses_actual_old_encoding_after_a_metadata_only_rewrite(
    writer: RefreshWriter, store: ArchiveStore, monkeypatch: pytest.MonkeyPatch
) -> None:
    ordinary = Writer(store.data_root, writer._manifest)
    ordinary.write(fetched(), request_id=start(writer._manifest))
    monkeypatch.setattr(refresh, "compress", lambda raw: compress(raw) + compress(b""))
    writer.write(fetched(), request_id=start(writer._manifest), rewrite=True)
    rewritten = writer._manifest.resources.get(URL)
    assert rewritten is not None
    assert rewritten.stored_bytes > len(compress(b"synthetic-A"))
    interrupt_before_commit(writer, monkeypatch)
    monkeypatch.setattr(
        refresh, "compress", lambda raw: compress(raw) + b"codec-changed"
    )
    assert writer.recover() == 1
    assert writer.read(URL) == b"synthetic-A"
    assert writer._manifest.resources.get(URL) == rewritten

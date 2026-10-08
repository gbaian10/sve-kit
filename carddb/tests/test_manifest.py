import hashlib
import sqlite3
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath

import pytest

from sve_carddb.ingest.archive.manifest import (
    AlreadyRunningError,
    ExclusiveLock,
    Kind,
    Manifest,
    ManifestError,
    Outcome,
    Region,
    RequestResult,
    RequestStart,
    Resource,
)

T0 = datetime(2026, 9, 24, 12, 0, tzinfo=UTC)
CARD_URL = "https://shadowverse-evolve.com/cardlist/?cardno=BP01-001"


def make_resource(
    url: str = CARD_URL, path: str = "raw/jp/card/BP01-001.html.zst"
) -> Resource:
    return Resource(
        url=url,
        region=Region.JP,
        kind=Kind.CARD,
        path=PurePosixPath(path),
        sha256="a" * 64,
        raw_bytes=1000,
        stored_bytes=200,
        content_type="text/html; charset=UTF-8",
        etag=None,
        last_modified=None,
        first_fetched_at=T0,
        last_checked_at=T0,
        last_changed_at=T0,
        archived_at=None,
    )


def make_start(url: str = CARD_URL, attempt: int = 1) -> RequestStart:
    return RequestStart(
        run_id="run-1",
        logical_fetch_id="fetch-1",
        attempt=attempt,
        hop=0,
        url=url,
        requested_url=url,
        sent_if_none_match=None,
    )


def test_resource_round_trip(manifest: Manifest) -> None:
    resource = make_resource()
    with manifest.transaction():
        manifest.resources.put(resource)
    assert manifest.resources.get(CARD_URL) == resource
    assert manifest.resources.get("https://example.com/") is None


def test_path_owner(manifest: Manifest) -> None:
    with manifest.transaction():
        manifest.resources.put(make_resource())
    assert (
        manifest.resources.path_owner(PurePosixPath("raw/jp/card/BP01-001.html.zst"))
        == CARD_URL
    )
    assert (
        manifest.resources.path_owner(PurePosixPath("raw/jp/card/other.html.zst"))
        is None
    )


def test_put_resource_updates_same_url(manifest: Manifest) -> None:
    with manifest.transaction():
        manifest.resources.put(make_resource())
    updated = replace(make_resource(), sha256="b" * 64)
    with manifest.transaction():
        manifest.resources.put(updated)
    assert manifest.resources.get(CARD_URL) == updated


def test_path_clash_keeps_the_original_owner(manifest: Manifest) -> None:
    with manifest.transaction():
        manifest.resources.put(make_resource())
    other = make_resource(url="https://shadowverse-evolve.com/cardlist/?cardno=X")
    with pytest.raises(sqlite3.IntegrityError), manifest.transaction():
        manifest.resources.put(other)
    assert manifest.resources.get(CARD_URL) == make_resource()


def test_transaction_rolls_back_on_error(manifest: Manifest) -> None:
    def put_then_fail() -> None:
        with manifest.transaction():
            manifest.resources.put(make_resource())
            raise RuntimeError

    with pytest.raises(RuntimeError):
        put_then_fail()
    assert manifest.resources.get(CARD_URL) is None


def test_request_lifecycle(manifest: Manifest) -> None:
    request_id = manifest.requests.start(make_start())
    assert manifest.requests.outcomes(CARD_URL) == [Outcome.STARTED]
    with manifest.transaction():
        manifest.requests.finish(
            request_id, RequestResult(outcome=Outcome.CHANGED, status=200)
        )
    assert manifest.requests.outcomes(CARD_URL) == [Outcome.CHANGED]


def test_interrupted_requests_become_unknown(tmp_path: Path) -> None:
    path = tmp_path / "manifest.sqlite"
    with Manifest.open(path) as first:
        first.requests.start(make_start())
    with Manifest.open(path) as second:
        assert second.requests.mark_interrupted() == 1
        assert second.requests.outcomes(CARD_URL) == [Outcome.UNKNOWN]


def test_finish_request_rejects_unknown_id(manifest: Manifest) -> None:
    with pytest.raises(ManifestError, match="no fetch_log row"), manifest.transaction():
        manifest.requests.finish(999, RequestResult(outcome=Outcome.FAILED))


def test_rejects_unsupported_schema_version(tmp_path: Path) -> None:
    path = tmp_path / "manifest.sqlite"
    Manifest.open(path).close()
    conn = sqlite3.connect(path)
    conn.execute("PRAGMA user_version = 99")
    conn.close()
    with pytest.raises(ManifestError, match="schema 99"):
        Manifest.open(path)


def test_backup_is_consistent_and_restorable(
    manifest: Manifest, tmp_path: Path
) -> None:
    with manifest.transaction():
        manifest.resources.put(make_resource())
    live_path = tmp_path / "manifest" / "manifest.sqlite"
    assert live_path.with_name("manifest.sqlite-wal").exists()
    info = manifest.backup(tmp_path / "backup" / "manifest.sqlite")
    before = info.path.read_bytes()
    assert info.sha256 == hashlib.sha256(before).hexdigest()
    with Manifest.open_snapshot(info.path) as restored:
        assert restored.resources.get(CARD_URL) == make_resource()
        assert restored.integrity_check() == "ok"
    assert info.path.read_bytes() == before
    assert not info.path.with_name("manifest.sqlite-wal").exists()
    assert not info.path.with_name("manifest.sqlite-shm").exists()


def test_live_reader_does_not_initialize_database(tmp_path: Path) -> None:
    path = tmp_path / "manifest.sqlite"
    with sqlite3.connect(path) as conn:
        conn.execute("PRAGMA user_version = 1")
    before = path.read_bytes()
    with Manifest.open_live(path) as manifest:
        with pytest.raises(sqlite3.OperationalError):
            manifest.resources.get(CARD_URL)
    assert path.read_bytes() == before
    assert not path.with_name("manifest.sqlite-wal").exists()


def test_live_reader_preserves_wal_manifest(tmp_path: Path) -> None:
    path = tmp_path / "manifest.sqlite"
    with Manifest.open(path) as manifest, manifest.transaction():
        manifest.resources.put(make_resource())
    before = path.read_bytes()
    with Manifest.open_live(path) as reader:
        assert reader.resources.get(CARD_URL) == make_resource()
    assert path.read_bytes() == before
    assert {item.name for item in tmp_path.iterdir()} <= {
        "manifest.sqlite",
        "manifest.sqlite-wal",
        "manifest.sqlite-shm",
    }
    wal = path.with_name("manifest.sqlite-wal")
    assert not wal.exists() or wal.stat().st_size == 0


def test_live_reader_requires_existing_supported_schema(tmp_path: Path) -> None:
    missing = tmp_path / "missing.sqlite"
    with pytest.raises(sqlite3.OperationalError):
        Manifest.open_live(missing)
    assert not missing.exists()
    unsupported = tmp_path / "unsupported.sqlite"
    with sqlite3.connect(unsupported) as conn:
        conn.execute("PRAGMA user_version = 99")
    before = unsupported.read_bytes()
    with pytest.raises(ManifestError, match="schema 99"):
        Manifest.open_live(unsupported)
    assert unsupported.read_bytes() == before


def test_backup_refuses_to_overwrite(manifest: Manifest, tmp_path: Path) -> None:
    dest = tmp_path / "existing.sqlite"
    dest.write_bytes(b"")
    with pytest.raises(FileExistsError):
        manifest.backup(dest)


def test_second_lock_fails_until_first_is_released(tmp_path: Path) -> None:
    lock_path = tmp_path / "manifest" / ".lock"
    with ExclusiveLock(lock_path):
        with pytest.raises(AlreadyRunningError), ExclusiveLock(lock_path):
            pass
    with ExclusiveLock(lock_path):
        pass

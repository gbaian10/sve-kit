import sqlite3
from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath

import pytest

from sve_carddb.manifest import (
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


@pytest.fixture
def manifest(tmp_path: Path) -> Manifest:
    return Manifest.open(tmp_path / "manifest" / "manifest.sqlite")


def test_resource_round_trip(manifest: Manifest) -> None:
    resource = make_resource()
    with manifest.transaction():
        manifest.put_resource(resource)
    assert manifest.get_resource(CARD_URL) == resource
    assert manifest.get_resource("https://example.com/") is None


def test_path_owner(manifest: Manifest) -> None:
    with manifest.transaction():
        manifest.put_resource(make_resource())
    assert (
        manifest.path_owner(PurePosixPath("raw/jp/card/BP01-001.html.zst")) == CARD_URL
    )
    assert manifest.path_owner(PurePosixPath("raw/jp/card/other.html.zst")) is None


def test_put_resource_updates_same_url(manifest: Manifest) -> None:
    with manifest.transaction():
        manifest.put_resource(make_resource())
    updated = replace(make_resource(), sha256="b" * 64)
    with manifest.transaction():
        manifest.put_resource(updated)
    assert manifest.get_resource(CARD_URL) == updated


def test_path_clash_keeps_the_original_owner(manifest: Manifest) -> None:
    with manifest.transaction():
        manifest.put_resource(make_resource())
    other = make_resource(url="https://shadowverse-evolve.com/cardlist/?cardno=X")
    with pytest.raises(sqlite3.IntegrityError), manifest.transaction():
        manifest.put_resource(other)
    assert manifest.get_resource(CARD_URL) == make_resource()


def test_transaction_rolls_back_on_error(manifest: Manifest) -> None:
    def put_then_fail() -> None:
        with manifest.transaction():
            manifest.put_resource(make_resource())
            raise RuntimeError

    with pytest.raises(RuntimeError):
        put_then_fail()
    assert manifest.get_resource(CARD_URL) is None


def test_request_lifecycle(manifest: Manifest) -> None:
    request_id = manifest.start_request(make_start())
    assert manifest.request_outcomes(CARD_URL) == [Outcome.STARTED]
    with manifest.transaction():
        manifest.finish_request(
            request_id, RequestResult(outcome=Outcome.CHANGED, status=200)
        )
    assert manifest.request_outcomes(CARD_URL) == [Outcome.CHANGED]


def test_interrupted_requests_become_unknown(tmp_path: Path) -> None:
    path = tmp_path / "manifest.sqlite"
    with Manifest.open(path) as first:
        first.start_request(make_start())
    with Manifest.open(path) as second:
        assert second.mark_interrupted_requests() == 1
        assert second.request_outcomes(CARD_URL) == [Outcome.UNKNOWN]


def test_finish_request_rejects_unknown_id(manifest: Manifest) -> None:
    with pytest.raises(ManifestError, match="no fetch_log row"), manifest.transaction():
        manifest.finish_request(999, RequestResult(outcome=Outcome.FAILED))


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
        manifest.put_resource(make_resource())
    info = manifest.backup(tmp_path / "backup" / "manifest.sqlite")
    assert len(info.sha256) == 64
    with Manifest.open(info.path) as restored:
        assert restored.get_resource(CARD_URL) == make_resource()


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

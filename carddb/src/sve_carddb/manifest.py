"""The crawl manifest: one SQLite file recording what was fetched, when, and where it lives.

sqlite3 returns rows as `Any`; every row is converted to a typed object here and
nowhere else, so the rest of the code never sees untyped values.
"""

import fcntl
import hashlib
import os
import sqlite3
from contextlib import contextmanager
from dataclasses import dataclass
from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path, PurePosixPath
from typing import IO, TYPE_CHECKING, Self

if TYPE_CHECKING:
    from collections.abc import Generator, Iterator
    from types import TracebackType

SCHEMA_VERSION = 1

_SCHEMA = """
CREATE TABLE IF NOT EXISTS resource (
    url               TEXT PRIMARY KEY,
    region            TEXT NOT NULL,
    kind              TEXT NOT NULL,
    path              TEXT NOT NULL UNIQUE,
    sha256            TEXT NOT NULL,
    raw_bytes         INTEGER NOT NULL,
    stored_bytes      INTEGER NOT NULL,
    content_type      TEXT NOT NULL,
    etag              TEXT,
    last_modified     TEXT,
    first_fetched_at  TEXT NOT NULL,
    last_checked_at   TEXT NOT NULL,
    last_changed_at   TEXT NOT NULL,
    archived_at       TEXT
) STRICT;

CREATE TABLE IF NOT EXISTS fetch_log (
    id                     INTEGER PRIMARY KEY,
    run_id                 TEXT NOT NULL,
    logical_fetch_id       TEXT NOT NULL,
    attempt                INTEGER NOT NULL,
    hop                    INTEGER NOT NULL,
    url                    TEXT NOT NULL,
    requested_url          TEXT NOT NULL,
    final_url              TEXT,
    started_at             TEXT NOT NULL,
    finished_at            TEXT,
    elapsed_ms             INTEGER,
    status                 INTEGER,
    error_class            TEXT,
    sent_if_none_match     TEXT,
    response_sha256        TEXT,
    response_bytes         INTEGER,
    response_etag          TEXT,
    response_last_modified TEXT,
    outcome                TEXT NOT NULL,
    validation_error       TEXT
) STRICT;

CREATE INDEX IF NOT EXISTS fetch_log_url ON fetch_log (url);
"""


class Region(StrEnum):
    JP = "jp"
    EN = "en"


class Kind(StrEnum):
    SETS = "sets"
    LIST = "list"
    CARD = "card"
    QA = "qa"
    LIMIT = "limit"
    RULES = "rules"
    ERRATA_INDEX = "errata_index"
    ERRATA = "errata"
    NEWS = "news"
    SITEMAP = "sitemap"
    IMAGE = "image"


class Outcome(StrEnum):
    STARTED = "started"
    CHANGED = "changed"
    UNCHANGED = "unchanged"
    NOT_MODIFIED = "not_modified"
    FAILED = "failed"
    UNKNOWN = "unknown"


class ManifestError(RuntimeError):
    """The manifest holds data this code cannot interpret."""


class AlreadyRunningError(RuntimeError):
    """Another crawler process holds the manifest lock."""


@dataclass(frozen=True, slots=True)
class Resource:
    """The latest successful state of one canonical URL."""

    url: str
    region: Region
    kind: Kind
    path: PurePosixPath
    sha256: str
    raw_bytes: int
    stored_bytes: int
    content_type: str
    etag: str | None
    last_modified: str | None
    first_fetched_at: datetime
    last_checked_at: datetime
    last_changed_at: datetime
    archived_at: datetime | None


@dataclass(frozen=True, slots=True)
class RequestStart:
    """Identifies one actual HTTP request before it is sent."""

    run_id: str
    logical_fetch_id: str
    attempt: int
    hop: int
    url: str
    requested_url: str
    sent_if_none_match: str | None


@dataclass(frozen=True, slots=True)
class RequestResult:
    """What came back from one HTTP request."""

    outcome: Outcome
    final_url: str | None = None
    status: int | None = None
    error_class: str | None = None
    response_sha256: str | None = None
    response_bytes: int | None = None
    response_etag: str | None = None
    response_last_modified: str | None = None
    validation_error: str | None = None


@dataclass(frozen=True, slots=True)
class BackupInfo:
    """A verified manifest snapshot."""

    path: Path
    sha256: str
    created_at: datetime


def utcnow() -> datetime:
    """Return the current time in UTC."""
    return datetime.now(UTC)


class Manifest:
    """Typed access to the manifest database."""

    def __init__(self, conn: sqlite3.Connection) -> None:
        """Wrap an open connection; use `Manifest.open` instead."""
        self._conn = conn

    @classmethod
    def open(cls, path: Path) -> Self:
        """Open or create the manifest at `path`."""
        path.parent.mkdir(parents=True, exist_ok=True)
        # Pragmas such as journal_mode cannot change inside a transaction, so set
        # everything up in autocommit mode and switch to explicit transactions after.
        conn = sqlite3.connect(path, autocommit=True)
        conn.execute("PRAGMA foreign_keys = ON")
        conn.execute("PRAGMA journal_mode = WAL")
        version = _int(conn.execute("PRAGMA user_version").fetchone()[0])
        if version not in {0, SCHEMA_VERSION}:
            conn.close()
            msg = f"manifest schema {version} is not supported (expected {SCHEMA_VERSION})"
            raise ManifestError(msg)
        conn.executescript(_SCHEMA)
        conn.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
        conn.autocommit = False
        return cls(conn)

    def close(self) -> None:
        """Close the connection."""
        self._conn.close()

    def __enter__(self) -> Self:
        """Return self for use in a `with` block."""
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        """Close the connection."""
        self.close()

    @contextmanager
    def transaction(self) -> Generator[None]:
        """Commit everything in the block together, or nothing on error."""
        try:
            yield
        except BaseException:
            self._conn.rollback()
            raise
        self._conn.commit()

    # --- resource -------------------------------------------------------

    def get_resource(self, url: str) -> Resource | None:
        """Return the latest successful state of `url`, if any."""
        row = self._conn.execute(
            "SELECT * FROM resource WHERE url = ?", (url,)
        ).fetchone()
        return None if row is None else _resource(row)

    def path_owner(self, path: PurePosixPath) -> str | None:
        """Return the URL that owns `path`, if any."""
        row = self._conn.execute(
            "SELECT url FROM resource WHERE path = ?", (str(path),)
        ).fetchone()
        return None if row is None else _str(row[0])

    def put_resource(self, resource: Resource) -> None:
        """Insert or replace a resource. Call inside `transaction()`."""
        self._conn.execute(
            # Upsert on url only: OR REPLACE would also resolve a clash on the UNIQUE
            # path by deleting the other URL's row, silently losing its record.
            "INSERT INTO resource VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"
            " ON CONFLICT (url) DO UPDATE SET region = excluded.region,"
            " kind = excluded.kind, path = excluded.path, sha256 = excluded.sha256,"
            " raw_bytes = excluded.raw_bytes, stored_bytes = excluded.stored_bytes,"
            " content_type = excluded.content_type, etag = excluded.etag,"
            " last_modified = excluded.last_modified,"
            " first_fetched_at = excluded.first_fetched_at,"
            " last_checked_at = excluded.last_checked_at,"
            " last_changed_at = excluded.last_changed_at,"
            " archived_at = excluded.archived_at",
            (
                resource.url,
                resource.region.value,
                resource.kind.value,
                str(resource.path),
                resource.sha256,
                resource.raw_bytes,
                resource.stored_bytes,
                resource.content_type,
                resource.etag,
                resource.last_modified,
                resource.first_fetched_at.isoformat(),
                resource.last_checked_at.isoformat(),
                resource.last_changed_at.isoformat(),
                None
                if resource.archived_at is None
                else resource.archived_at.isoformat(),
            ),
        )

    def resources(self) -> Iterator[Resource]:
        """Yield every resource, ordered by URL."""
        for row in self._conn.execute("SELECT * FROM resource ORDER BY url"):
            yield _resource(row)

    # --- fetch_log ------------------------------------------------------

    def start_request(self, start: RequestStart) -> int:
        """Record a request before it is sent and commit at once.

        If the process dies mid-request the row stays `started`; see
        `mark_interrupted_requests`.
        """
        cursor = self._conn.execute(
            "INSERT INTO fetch_log (run_id, logical_fetch_id, attempt, hop, url,"
            " requested_url, started_at, sent_if_none_match, outcome)"
            " VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            (
                start.run_id,
                start.logical_fetch_id,
                start.attempt,
                start.hop,
                start.url,
                start.requested_url,
                utcnow().isoformat(),
                start.sent_if_none_match,
                Outcome.STARTED.value,
            ),
        )
        self._conn.commit()
        if cursor.lastrowid is None:
            msg = "fetch_log insert returned no row id"
            raise ManifestError(msg)
        return cursor.lastrowid

    def finish_request(self, request_id: int, result: RequestResult) -> None:
        """Record the result of a request. Call inside `transaction()`."""
        finished = utcnow()
        row = self._conn.execute(
            "SELECT started_at FROM fetch_log WHERE id = ?", (request_id,)
        ).fetchone()
        if row is None:
            msg = f"no fetch_log row {request_id}"
            raise ManifestError(msg)
        elapsed = finished - _datetime(row[0])
        self._conn.execute(
            "UPDATE fetch_log SET final_url = ?, finished_at = ?, elapsed_ms = ?,"
            " status = ?, error_class = ?, response_sha256 = ?, response_bytes = ?,"
            " response_etag = ?, response_last_modified = ?, outcome = ?,"
            " validation_error = ? WHERE id = ?",
            (
                result.final_url,
                finished.isoformat(),
                round(elapsed.total_seconds() * 1000),
                result.status,
                result.error_class,
                result.response_sha256,
                result.response_bytes,
                result.response_etag,
                result.response_last_modified,
                result.outcome.value,
                result.validation_error,
                request_id,
            ),
        )

    def mark_interrupted_requests(self) -> int:
        """Turn leftover `started` rows into `unknown`; return how many."""
        cursor = self._conn.execute(
            "UPDATE fetch_log SET outcome = ? WHERE outcome = ?",
            (Outcome.UNKNOWN.value, Outcome.STARTED.value),
        )
        self._conn.commit()
        return cursor.rowcount

    def request_outcomes(self, url: str) -> list[Outcome]:
        """Return the outcomes of all requests for `url`, oldest first."""
        rows = self._conn.execute(
            "SELECT outcome FROM fetch_log WHERE url = ? ORDER BY id", (url,)
        ).fetchall()
        return [Outcome(_str(row[0])) for row in rows]

    # --- backup ---------------------------------------------------------

    def backup(self, dest: Path) -> BackupInfo:
        """Write a consistent snapshot to `dest` and verify it.

        Uses SQLite's backup API instead of copying the file, which could miss
        data still in the WAL.
        """
        if dest.exists():
            msg = f"backup target already exists: {dest}"
            raise FileExistsError(msg)
        dest.parent.mkdir(parents=True, exist_ok=True)
        target = sqlite3.connect(dest)
        try:
            self._conn.backup(target)
            check = _str(target.execute("PRAGMA integrity_check").fetchone()[0])
        finally:
            target.close()
        if check != "ok":
            msg = f"backup failed integrity_check: {check}"
            raise ManifestError(msg)
        with dest.open("rb") as file:
            digest = hashlib.file_digest(file, "sha256").hexdigest()
        return BackupInfo(path=dest, sha256=digest, created_at=utcnow())


class ExclusiveLock:
    """Hold an exclusive, non-blocking lock so only one crawler runs at a time."""

    def __init__(self, path: Path) -> None:
        """Prepare a lock on `path`; the lock is taken on `__enter__`."""
        self._path = path
        self._file: IO[str] | None = None

    def __enter__(self) -> Self:
        """Take the lock, or raise `AlreadyRunningError`."""
        self._path.parent.mkdir(parents=True, exist_ok=True)
        file = self._path.open("a")
        try:
            fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            file.close()
            msg = f"another crawler holds {self._path}"
            raise AlreadyRunningError(msg) from exc
        file.truncate(0)
        file.write(f"{os.getpid()}\n")
        file.flush()
        self._file = file
        return self

    def __exit__(
        self,
        exc_type: type[BaseException] | None,
        exc: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        """Release the lock."""
        if self._file is not None:
            fcntl.flock(self._file, fcntl.LOCK_UN)
            self._file.close()
            self._file = None


# --- row conversion -----------------------------------------------------


def _str(value: object) -> str:
    if not isinstance(value, str):
        msg = f"expected text in manifest, got {type(value).__name__}"
        raise ManifestError(msg)
    return value


def _opt_str(value: object) -> str | None:
    return None if value is None else _str(value)


def _int(value: object) -> int:
    if not isinstance(value, int) or isinstance(value, bool):
        msg = f"expected integer in manifest, got {type(value).__name__}"
        raise ManifestError(msg)
    return value


def _datetime(value: object) -> datetime:
    return datetime.fromisoformat(_str(value))


def _opt_datetime(value: object) -> datetime | None:
    return None if value is None else _datetime(value)


def _resource(row: tuple[object, ...]) -> Resource:
    return Resource(
        url=_str(row[0]),
        region=Region(_str(row[1])),
        kind=Kind(_str(row[2])),
        path=PurePosixPath(_str(row[3])),
        sha256=_str(row[4]),
        raw_bytes=_int(row[5]),
        stored_bytes=_int(row[6]),
        content_type=_str(row[7]),
        etag=_opt_str(row[8]),
        last_modified=_opt_str(row[9]),
        first_fetched_at=_datetime(row[10]),
        last_checked_at=_datetime(row[11]),
        last_changed_at=_datetime(row[12]),
        archived_at=_opt_datetime(row[13]),
    )

"""The crawl manifest: one SQLite file recording what was fetched, when, and where it lives.

sqlite3 returns rows as `Any`; every row is converted to a typed object here and
nowhere else, so the rest of the code never sees untyped values.
"""

import fcntl
import hashlib
import os
import sqlite3
from contextlib import AbstractContextManager, contextmanager
from dataclasses import dataclass
from datetime import UTC, datetime
from enum import StrEnum
from pathlib import Path, PurePosixPath
from typing import IO, TYPE_CHECKING, Self

if TYPE_CHECKING:
    from collections.abc import Callable, Generator, Iterator, Sequence
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

-- Current links of single-page sources (card, limit, errata, news, rules).
CREATE TABLE IF NOT EXISTS link (
    from_url     TEXT NOT NULL,
    to_url       TEXT NOT NULL,
    to_kind      TEXT NOT NULL,
    position     INTEGER NOT NULL,
    original     TEXT NOT NULL,
    from_sha256  TEXT NOT NULL,
    PRIMARY KEY (from_url, to_kind, position)
) STRICT;

CREATE TABLE IF NOT EXISTS link_log (
    id           INTEGER PRIMARY KEY,
    from_url     TEXT NOT NULL,
    to_url       TEXT NOT NULL,
    to_kind      TEXT NOT NULL,
    position     INTEGER NOT NULL,
    original     TEXT NOT NULL,
    from_sha256  TEXT NOT NULL,
    event        TEXT NOT NULL,
    at           TEXT NOT NULL
) STRICT;

-- Multi-page discoveries (sets, list per set, errata index, paged Q&A).
-- Each generation keeps its own immutable snapshot of pages and edges.
CREATE TABLE IF NOT EXISTS discovery_generation (
    id              INTEGER PRIMARY KEY,
    root            TEXT NOT NULL,
    status          TEXT NOT NULL,
    started_at      TEXT NOT NULL,
    finished_at     TEXT,
    declared_total  INTEGER,
    max_page        INTEGER
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS one_validated_generation_per_root
    ON discovery_generation (root) WHERE status = 'validated';

CREATE TABLE IF NOT EXISTS generation_page (
    generation_id  INTEGER NOT NULL REFERENCES discovery_generation (id),
    page_url       TEXT NOT NULL,
    page_sha256    TEXT NOT NULL,
    PRIMARY KEY (generation_id, page_url)
) STRICT;

CREATE TABLE IF NOT EXISTS generation_edge (
    generation_id  INTEGER NOT NULL,
    from_url       TEXT NOT NULL,
    to_url         TEXT NOT NULL,
    to_kind        TEXT NOT NULL,
    position       INTEGER NOT NULL,
    original       TEXT NOT NULL,
    PRIMARY KEY (generation_id, from_url, to_kind, position),
    FOREIGN KEY (generation_id, from_url)
        REFERENCES generation_page (generation_id, page_url)
) STRICT;
"""


class Region(StrEnum):
    JP = "jp"
    EN = "en"
    # The digital games' official card lists, kept for mapping SVE cards to digital ones.
    SV1 = "sv1"
    SVWB = "svwb"


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
    API = "api"


class Outcome(StrEnum):
    STARTED = "started"
    CHANGED = "changed"
    UNCHANGED = "unchanged"
    NOT_MODIFIED = "not_modified"
    REDIRECTED = "redirected"
    FAILED = "failed"
    UNKNOWN = "unknown"


class GenerationStatus(StrEnum):
    IN_PROGRESS = "in_progress"
    VALIDATED = "validated"
    FAILED = "failed"
    SUPERSEDED = "superseded"


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


@dataclass(frozen=True, slots=True)
class Link:
    """A link found on a page. `original` is the exact string on the page."""

    to_url: str
    to_kind: Kind
    position: int
    original: str


@dataclass(frozen=True, slots=True)
class Edge:
    """A link recorded in a discovery generation."""

    from_url: str
    link: Link


@dataclass(frozen=True, slots=True)
class Generation:
    """One discovery of a root such as `jp:list:BP15`."""

    id: int
    root: str
    status: GenerationStatus
    started_at: datetime
    finished_at: datetime | None
    declared_total: int | None
    max_page: int | None


def utcnow() -> datetime:
    """Return the current time in UTC."""
    return datetime.now(UTC)


@dataclass(frozen=True)
class _Store:
    _conn: sqlite3.Connection
    _transaction: Callable[[], AbstractContextManager[None]]


class ResourceStore(_Store):
    """Latest successful state of each canonical URL."""

    def get(self, url: str) -> Resource | None:
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

    def put(self, resource: Resource) -> None:
        """Insert or replace a resource. Call inside `Manifest.transaction()`."""
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

    def all(self) -> Iterator[Resource]:
        """Yield every resource, ordered by URL."""
        for row in self._conn.execute("SELECT * FROM resource ORDER BY url"):
            yield _resource(row)


class RequestLog(_Store):
    """One row per actual HTTP request."""

    def start(self, start: RequestStart) -> int:
        """Record a request before it is sent and commit at once.

        If the process dies mid-request the row stays `started`; see
        `mark_interrupted`.
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

    def finish(self, request_id: int, result: RequestResult) -> None:
        """Record the result of a request. Call inside `Manifest.transaction()`."""
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

    def mark_interrupted(self) -> int:
        """Turn leftover `started` rows into `unknown`; return how many."""
        cursor = self._conn.execute(
            "UPDATE fetch_log SET outcome = ? WHERE outcome = ?",
            (Outcome.UNKNOWN.value, Outcome.STARTED.value),
        )
        self._conn.commit()
        return cursor.rowcount

    def outcomes(self, url: str) -> list[Outcome]:
        """Return the outcomes of all requests for `url`, oldest first."""
        rows = self._conn.execute(
            "SELECT outcome FROM fetch_log WHERE url = ? ORDER BY id", (url,)
        ).fetchall()
        return [Outcome(_str(row[0])) for row in rows]


class LinkStore(_Store):
    """Current links of single-page sources, with history."""

    def current(self, from_url: str) -> list[Link]:
        """Return the current links of a single-page source, in position order."""
        rows = self._conn.execute(
            "SELECT to_url, to_kind, position, original FROM link"
            " WHERE from_url = ? ORDER BY to_kind, position",
            (from_url,),
        ).fetchall()
        return [_link(row) for row in rows]

    def replace(self, from_url: str, from_sha256: str, links: Sequence[Link]) -> None:
        """Replace a page's current links and log what changed.

        Call inside `Manifest.transaction()`, together with the page's resource update.
        """
        old = set(self.current(from_url))
        new = set(links)
        if len(new) != len(links):
            msg = f"duplicate links from {from_url}"
            raise ManifestError(msg)
        now = utcnow().isoformat()
        for event, changed in (("removed", old - new), ("added", new - old)):
            for link in sorted(changed, key=_link_order):
                self._conn.execute(
                    "INSERT INTO link_log (from_url, to_url, to_kind, position,"
                    " original, from_sha256, event, at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                    (from_url, *_link_values(link), from_sha256, event, now),
                )
        self._conn.execute("DELETE FROM link WHERE from_url = ?", (from_url,))
        self._conn.executemany(
            "INSERT INTO link VALUES (?, ?, ?, ?, ?, ?)",
            [(from_url, *_link_values(link), from_sha256) for link in links],
        )

    def history(self, from_url: str) -> list[tuple[str, Link]]:
        """Return `(event, link)` pairs for a page, oldest first."""
        rows = self._conn.execute(
            "SELECT event, to_url, to_kind, position, original FROM link_log"
            " WHERE from_url = ? ORDER BY id",
            (from_url,),
        ).fetchall()
        return [(_str(row[0]), _link(row[1:])) for row in rows]


class GenerationStore(_Store):
    """Discovery generations and their immutable snapshots."""

    def start(self, root: str) -> Generation:
        """Begin a new discovery of `root` and commit at once.

        An unfinished generation of the same root is marked failed; the
        validated one, if any, stays in use until this one is validated.
        """
        now = utcnow()
        with self._transaction():
            self._conn.execute(
                "UPDATE discovery_generation SET status = ?, finished_at = ?"
                " WHERE root = ? AND status = ?",
                (
                    GenerationStatus.FAILED.value,
                    now.isoformat(),
                    root,
                    GenerationStatus.IN_PROGRESS.value,
                ),
            )
            cursor = self._conn.execute(
                "INSERT INTO discovery_generation (root, status, started_at)"
                " VALUES (?, ?, ?)",
                (root, GenerationStatus.IN_PROGRESS.value, now.isoformat()),
            )
        if cursor.lastrowid is None:
            msg = "discovery_generation insert returned no row id"
            raise ManifestError(msg)
        return self.get(cursor.lastrowid)

    def get(self, generation_id: int) -> Generation:
        """Return a generation by id."""
        row = self._conn.execute(
            "SELECT * FROM discovery_generation WHERE id = ?", (generation_id,)
        ).fetchone()
        if row is None:
            msg = f"no generation {generation_id}"
            raise ManifestError(msg)
        return _generation(row)

    def current(self, root: str) -> Generation | None:
        """Return the validated generation of `root`, the only one scheduling may use."""
        row = self._conn.execute(
            "SELECT * FROM discovery_generation WHERE root = ? AND status = ?",
            (root, GenerationStatus.VALIDATED.value),
        ).fetchone()
        return None if row is None else _generation(row)

    def add_page(
        self, generation_id: int, page_url: str, page_sha256: str, links: Sequence[Link]
    ) -> None:
        """Record a page and its links in an unfinished generation.

        Call inside `Manifest.transaction()`. A page can be added once; generations
        are immutable snapshots, so a recheck of the same page is not added.
        """
        self._require_status(generation_id, GenerationStatus.IN_PROGRESS)
        self._conn.execute(
            "INSERT INTO generation_page VALUES (?, ?, ?)",
            (generation_id, page_url, page_sha256),
        )
        self._conn.executemany(
            "INSERT INTO generation_edge VALUES (?, ?, ?, ?, ?, ?)",
            [(generation_id, page_url, *_link_values(link)) for link in links],
        )

    def validate(
        self,
        generation_id: int,
        *,
        declared_total: int | None = None,
        max_page: int | None = None,
    ) -> None:
        """Publish a finished generation and retire the previous validated one, atomically."""
        generation = self._require_status(generation_id, GenerationStatus.IN_PROGRESS)
        with self._transaction():
            self._conn.execute(
                "UPDATE discovery_generation SET status = ? WHERE root = ? AND status = ?",
                (
                    GenerationStatus.SUPERSEDED.value,
                    generation.root,
                    GenerationStatus.VALIDATED.value,
                ),
            )
            self._conn.execute(
                "UPDATE discovery_generation SET status = ?, finished_at = ?,"
                " declared_total = ?, max_page = ? WHERE id = ?",
                (
                    GenerationStatus.VALIDATED.value,
                    utcnow().isoformat(),
                    declared_total,
                    max_page,
                    generation_id,
                ),
            )

    def fail(self, generation_id: int) -> None:
        """Mark an unfinished generation as failed; it will never be scheduled from."""
        self._require_status(generation_id, GenerationStatus.IN_PROGRESS)
        with self._transaction():
            self._conn.execute(
                "UPDATE discovery_generation SET status = ?, finished_at = ? WHERE id = ?",
                (GenerationStatus.FAILED.value, utcnow().isoformat(), generation_id),
            )

    def edges(self, generation_id: int) -> list[Edge]:
        """Return a generation's edges in page and position order."""
        rows = self._conn.execute(
            "SELECT from_url, to_url, to_kind, position, original FROM generation_edge"
            " WHERE generation_id = ? ORDER BY from_url, to_kind, position",
            (generation_id,),
        ).fetchall()
        return [Edge(from_url=_str(row[0]), link=_link(row[1:])) for row in rows]

    def _require_status(
        self, generation_id: int, status: GenerationStatus
    ) -> Generation:
        generation = self.get(generation_id)
        if generation.status is not status:
            msg = (
                f"generation {generation_id} is {generation.status}, expected {status}"
            )
            raise ManifestError(msg)
        return generation


class Manifest:
    """Typed access to the manifest database."""

    def __init__(self, conn: sqlite3.Connection) -> None:
        """Wrap an open connection; use `Manifest.open` instead."""
        self._conn = conn
        self.resources = ResourceStore(conn, self.transaction)
        self.requests = RequestLog(conn, self.transaction)
        self.links = LinkStore(conn, self.transaction)
        self.generations = GenerationStore(conn, self.transaction)

    @classmethod
    def open(cls, path: Path) -> Self:
        """Open or create the manifest at `path`, applying the current schema."""
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

    @classmethod
    def open_empty(cls) -> Self:
        """Create an in-memory manifest for a preview before the first crawl."""
        conn = sqlite3.connect(":memory:", autocommit=True)
        conn.executescript(_SCHEMA)
        conn.execute(f"PRAGMA user_version = {SCHEMA_VERSION}")
        conn.autocommit = False
        return cls(conn)

    @classmethod
    def open_live(cls, path: Path) -> Self:
        """Read an existing live manifest without initializing or migrating it.

        SQLite may create or retain WAL sidecars while reading; the manifest is not written.
        """
        return cls._open_readonly(path, immutable=False)

    @classmethod
    def open_snapshot(cls, path: Path) -> Self:
        """Read a closed backup without creating or changing SQLite sidecars."""
        return cls._open_readonly(path, immutable=True)

    @classmethod
    def _open_readonly(cls, path: Path, *, immutable: bool) -> Self:
        uri = f"{path.resolve().as_uri()}?mode=ro"
        if immutable:
            uri += "&immutable=1"
        conn = sqlite3.connect(uri, uri=True, autocommit=True)
        try:
            conn.execute("PRAGMA query_only = ON")
            version = _int(conn.execute("PRAGMA user_version").fetchone()[0])
        except BaseException:
            conn.close()
            raise
        if version != SCHEMA_VERSION:
            conn.close()
            msg = (
                f"manifest schema {version} is not supported "
                f"(expected {SCHEMA_VERSION}); run a writing command to migrate it"
            )
            raise ManifestError(msg)
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

    def integrity_check(self) -> str:
        """Return SQLite's integrity check result."""
        return _str(self._conn.execute("PRAGMA integrity_check").fetchone()[0])

    def historical_raw_hashes(self) -> list[tuple[str, str]]:
        """Return known successful source hashes that may predate current rows."""
        rows = self._conn.execute(
            "SELECT url, response_sha256 FROM fetch_log"
            " WHERE outcome IN ('changed', 'unchanged') AND response_sha256 IS NOT NULL"
            " UNION SELECT page_url, page_sha256 FROM generation_page"
            " UNION SELECT from_url, from_sha256 FROM link"
            " UNION SELECT from_url, from_sha256 FROM link_log"
            " ORDER BY 1, 2"
        ).fetchall()
        return [(_str(row[0]), _str(row[1])) for row in rows]

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
        finally:
            target.close()
        with self.open_snapshot(dest) as snapshot:
            check = snapshot.integrity_check()
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


def _opt_int(value: object) -> int | None:
    return None if value is None else _int(value)


def _link(row: Sequence[object]) -> Link:
    return Link(
        to_url=_str(row[0]),
        to_kind=Kind(_str(row[1])),
        position=_int(row[2]),
        original=_str(row[3]),
    )


def _link_values(link: Link) -> tuple[str, str, int, str]:
    return (link.to_url, link.to_kind.value, link.position, link.original)


def _link_order(link: Link) -> tuple[str, int]:
    return (link.to_kind.value, link.position)


def _generation(row: tuple[object, ...]) -> Generation:
    return Generation(
        id=_int(row[0]),
        root=_str(row[1]),
        status=GenerationStatus(_str(row[2])),
        started_at=_datetime(row[3]),
        finished_at=_opt_datetime(row[4]),
        declared_total=_opt_int(row[5]),
        max_page=_opt_int(row[6]),
    )


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

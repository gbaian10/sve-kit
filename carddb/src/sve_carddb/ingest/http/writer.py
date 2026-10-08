"""Write fetched content to disk and record it, in an order that survives crashes.

Order: check path ownership -> write a temp file -> verify -> fsync -> atomic
rename -> fsync the directory -> update the manifest in one transaction.
A crash between the rename and the commit leaves a file whose hash no longer
matches the manifest; `local_state` then reports it as untrusted and it is
fetched again. This is recoverable, not atomic.
"""

import errno
import hashlib
import os
import secrets
from dataclasses import dataclass, replace
from enum import StrEnum
from typing import TYPE_CHECKING

from sve_carddb.ingest.archive.manifest import (
    Kind,
    Manifest,
    Outcome,
    Region,
    RequestResult,
    Resource,
    utcnow,
)
from sve_carddb.ingest.archive.store import (
    CorruptDataError,
    UnsafePathError,
    compress,
    decompress,
    resolve_within,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Sequence
    from pathlib import Path, PurePosixPath

_TEMP_MARKER = ".tmp-"


class PathConflictError(RuntimeError):
    """Two different URLs would be stored at the same path."""


class DiskFullError(OSError):
    """The disk is full. Fatal: every stage must stop."""


class LocalState(StrEnum):
    TRUSTED = "trusted"
    UNTRUSTED = "untrusted"
    MISSING = "missing"
    ARCHIVED = "archived"


@dataclass(frozen=True, slots=True)
class Fetched:
    """Content that passed validation and is ready to store."""

    url: str
    region: Region
    kind: Kind
    path: PurePosixPath
    body: bytes
    content_type: str
    etag: str | None
    last_modified: str | None
    compressed: bool


@dataclass(frozen=True, slots=True)
class WriteResult:
    """What `Writer.write` did."""

    resource: Resource
    changed: bool
    """The content differs from the previous copy."""
    rewritten: bool
    """The file on disk was written (new, changed, damaged, or repair mode)."""


class RefreshProtectionError(RuntimeError):
    """Replacing a recorded raw version requires the archive replacement protocol."""


def sha256(data: bytes) -> str:
    """Return the hex SHA-256 of `data`."""
    return hashlib.sha256(data).hexdigest()


class Writer:
    """Store fetched content under the data root and record it in the manifest."""

    def __init__(
        self,
        root: Path,
        manifest: Manifest,
        *,
        read_roots: Sequence[Path] = (),
        protect_history: bool = False,
        create_only: bool = False,
    ) -> None:
        """Write under `root`, recording into `manifest`.

        Reads may also follow symlinks into `read_roots`; writes never do.
        """
        self._root = root
        self._manifest = manifest
        self._read_roots = tuple(read_roots)
        self._protect_history = protect_history
        self._create_only = create_only

    @property
    def manifest(self) -> Manifest:
        """Expose the typed boundary used by protected source adapters."""
        return self._manifest

    @property
    def create_only(self) -> bool:
        """Whether this writer refuses existing resources and destination files."""
        return self._create_only

    def check_path(self, url: str, path: PurePosixPath) -> None:
        """Raise `PathConflictError` if another URL owns `path`. Call before downloading."""
        owner = self._manifest.resources.path_owner(path)
        if owner is not None and owner != url:
            msg = f"{path} already belongs to {owner}, not {url}"
            raise PathConflictError(msg)

    def local_state(self, url: str) -> LocalState:
        """Classify the local copy of `url`; only a trusted copy may be skipped."""
        resource = self._manifest.resources.get(url)
        if resource is None:
            return LocalState.MISSING
        if resource.archived_at is not None:
            return LocalState.ARCHIVED
        target = self._resolve_for_read(resource.path)
        try:
            stored = target.read_bytes()
        except FileNotFoundError:
            return LocalState.UNTRUSTED
        if len(stored) != resource.stored_bytes:
            return LocalState.UNTRUSTED
        try:
            raw = decompress(stored) if _is_compressed(resource) else stored
        except CorruptDataError:
            return LocalState.UNTRUSTED
        return (
            LocalState.TRUSTED
            if sha256(raw) == resource.sha256
            else LocalState.UNTRUSTED
        )

    def check_new(self, url: str, path: PurePosixPath) -> None:
        """Refuse recorded sources and any existing destination, without repairing them."""
        self.check_path(url, path)
        if self._manifest.resources.get(url) is not None:
            raise PathConflictError(f"new-only source already recorded: {url}")
        target = self._new_target(path)
        if target.exists() or target.is_symlink():
            raise PathConflictError(f"new-only destination already exists: {path}")

    def _new_target(self, path: PurePosixPath) -> Path:
        target = resolve_within(self._root, path)
        if target != (self._root / path).absolute():
            raise UnsafePathError(f"new-only destination traverses a symlink: {path}")
        return target

    def read(self, url: str) -> bytes:
        """Return the stored content of a trusted local copy, decompressed."""
        resource = self._manifest.resources.get(url)
        if resource is None or self.local_state(url) is not LocalState.TRUSTED:
            msg = f"no trusted local copy of {url}"
            raise RuntimeError(msg)
        stored = self._resolve_for_read(resource.path).read_bytes()
        return decompress(stored) if _is_compressed(resource) else stored

    def write(
        self,
        fetched: Fetched,
        *,
        request_id: int,
        rewrite: bool = False,
        in_transaction: Callable[[Resource], None] | None = None,
    ) -> WriteResult:
        """Store `fetched`, finish its request, and run `in_transaction` in the same commit.

        Unchanged content from a trusted copy leaves the file alone. `rewrite`
        (repair mode) writes the file even when the hash matches.
        """
        self.check_path(fetched.url, fetched.path)
        if self._create_only:
            self.check_new(fetched.url, fetched.path)
        now = utcnow()
        digest = sha256(fetched.body)
        previous = self._manifest.resources.get(fetched.url)
        if self._protect_history and previous is not None and previous.sha256 != digest:
            msg = f"raw history protection stopped replacement of {fetched.url}"
            raise RefreshProtectionError(msg)
        unchanged = (
            previous is not None
            and previous.sha256 == digest
            and self.local_state(fetched.url) is LocalState.TRUSTED
        )
        rewritten = not (unchanged and not rewrite and previous is not None)
        if not rewritten and previous is not None:
            resource = replace(
                previous,
                last_checked_at=now,
                etag=fetched.etag,
                last_modified=fetched.last_modified,
            )
        else:
            stored = self._write_file(fetched)
            resource = Resource(
                url=fetched.url,
                region=fetched.region,
                kind=fetched.kind,
                path=fetched.path,
                sha256=digest,
                raw_bytes=len(fetched.body),
                stored_bytes=stored,
                content_type=fetched.content_type,
                etag=fetched.etag,
                last_modified=fetched.last_modified,
                first_fetched_at=now if previous is None else previous.first_fetched_at,
                last_checked_at=now,
                last_changed_at=now
                if previous is None or previous.sha256 != digest
                else previous.last_changed_at,
                archived_at=None,
            )
        self._record(resource, request_id, unchanged, in_transaction)
        return WriteResult(
            resource=resource, changed=not unchanged, rewritten=rewritten
        )

    def _record(
        self,
        resource: Resource,
        request_id: int,
        unchanged: bool,
        in_transaction: Callable[[Resource], None] | None,
    ) -> None:
        outcome = Outcome.UNCHANGED if unchanged else Outcome.CHANGED
        with self._manifest.transaction():
            self._manifest.resources.put(resource)
            self._manifest.requests.finish(
                request_id,
                RequestResult(
                    outcome=outcome,
                    final_url=resource.url,
                    status=200,
                    response_sha256=resource.sha256,
                    response_bytes=resource.raw_bytes,
                    response_etag=resource.etag,
                    response_last_modified=resource.last_modified,
                ),
            )
            if in_transaction is not None:
                in_transaction(resource)

    def mark_not_modified(self, url: str, *, request_id: int) -> Resource:
        """Record a 304 for a trusted local copy; nothing is written to disk."""
        resource = self._manifest.resources.get(url)
        if resource is None or self.local_state(url) is not LocalState.TRUSTED:
            msg = f"304 for {url} without a trusted local copy"
            raise RuntimeError(msg)
        updated = replace(resource, last_checked_at=utcnow())
        with self._manifest.transaction():
            self._manifest.resources.put(updated)
            self._manifest.requests.finish(
                request_id, RequestResult(outcome=Outcome.NOT_MODIFIED, status=304)
            )
        return updated

    def _resolve_for_read(self, path: PurePosixPath) -> Path:
        return resolve_within(self._root, path, also_allowed=self._read_roots)

    def _write_file(self, fetched: Fetched) -> int:
        target = (
            self._new_target(fetched.path)
            if self._create_only
            else resolve_within(self._root, fetched.path)
        )
        data = compress(fetched.body) if fetched.compressed else fetched.body
        if fetched.compressed and decompress(data) != fetched.body:
            msg = f"compression round trip failed for {fetched.url}"
            raise CorruptDataError(msg)
        target.parent.mkdir(parents=True, exist_ok=True)
        temp = target.with_name(f"{target.name}{_TEMP_MARKER}{secrets.token_hex(4)}")
        created = False
        try:
            with temp.open("xb") as file:
                created = True
                file.write(data)
                file.flush()
                os.fsync(file.fileno())
            _require_size(temp, len(data))
            if self._create_only:
                # link publishes complete bytes atomically and cannot replace a raced-in file.
                self.check_new(fetched.url, fetched.path)
                os.link(temp, target, follow_symlinks=False)
            else:
                temp.replace(target)
            _fsync_dir(target.parent)
        except OSError as exc:
            if exc.errno == errno.ENOSPC:
                msg = f"disk full while writing {fetched.path}"
                raise DiskFullError(errno.ENOSPC, msg) from exc
            raise
        finally:
            if created:
                temp.unlink(missing_ok=True)
        return len(data)


def remove_temp_files(root: Path) -> list[Path]:
    """Delete temp files left by an interrupted write; return what was removed."""
    removed: list[Path] = []
    if not root.exists():
        return removed
    for path in root.rglob(f"*{_TEMP_MARKER}*"):
        if path.is_file():
            path.unlink()
            removed.append(path)
    return removed


def _require_size(path: Path, expected: int) -> None:
    actual = path.stat().st_size
    if actual != expected:
        msg = f"short write: {path} has {actual} bytes, expected {expected}"
        raise OSError(msg)


def _is_compressed(resource: Resource) -> bool:
    return resource.path.suffix == ".zst"


def _fsync_dir(directory: Path) -> None:
    fd = os.open(directory, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)

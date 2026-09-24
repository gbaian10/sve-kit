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

from sve_carddb.manifest import (
    Kind,
    Manifest,
    Outcome,
    Region,
    RequestResult,
    Resource,
    utcnow,
)
from sve_carddb.store import CorruptDataError, compress, decompress, resolve_within

if TYPE_CHECKING:
    from collections.abc import Callable
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


def sha256(data: bytes) -> str:
    """Return the hex SHA-256 of `data`."""
    return hashlib.sha256(data).hexdigest()


class Writer:
    """Store fetched content under the data root and record it in the manifest."""

    def __init__(self, root: Path, manifest: Manifest) -> None:
        """Write under `root`, recording into `manifest`."""
        self._root = root
        self._manifest = manifest

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
        target = resolve_within(self._root, resource.path)
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
        now = utcnow()
        digest = sha256(fetched.body)
        previous = self._manifest.resources.get(fetched.url)
        unchanged = (
            previous is not None
            and previous.sha256 == digest
            and self.local_state(fetched.url) is LocalState.TRUSTED
        )
        rewritten = not (unchanged and not rewrite and previous is not None)
        if not rewritten and previous is not None:
            resource = replace(previous, last_checked_at=now)
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
        outcome = Outcome.UNCHANGED if unchanged else Outcome.CHANGED
        with self._manifest.transaction():
            self._manifest.resources.put(resource)
            self._manifest.requests.finish(
                request_id,
                RequestResult(
                    outcome=outcome,
                    final_url=fetched.url,
                    status=200,
                    response_sha256=digest,
                    response_bytes=len(fetched.body),
                    response_etag=fetched.etag,
                    response_last_modified=fetched.last_modified,
                ),
            )
            if in_transaction is not None:
                in_transaction(resource)
        return WriteResult(
            resource=resource, changed=not unchanged, rewritten=rewritten
        )

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

    def _write_file(self, fetched: Fetched) -> int:
        target = resolve_within(self._root, fetched.path)
        data = compress(fetched.body) if fetched.compressed else fetched.body
        if fetched.compressed and decompress(data) != fetched.body:
            msg = f"compression round trip failed for {fetched.url}"
            raise CorruptDataError(msg)
        target.parent.mkdir(parents=True, exist_ok=True)
        temp = target.with_name(f"{target.name}{_TEMP_MARKER}{secrets.token_hex(4)}")
        try:
            with temp.open("xb") as file:
                file.write(data)
                file.flush()
                os.fsync(file.fileno())
            _require_size(temp, len(data))
            temp.replace(target)
            _fsync_dir(target.parent)
        except OSError as exc:
            temp.unlink(missing_ok=True)
            if exc.errno == errno.ENOSPC:
                msg = f"disk full while writing {fetched.path}"
                raise DiskFullError(errno.ENOSPC, msg) from exc
            raise
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

"""Archive both sides of a latest replacement and recover from the committed hash."""

# ruff: file-ignore[private-member-access] -- share the archive's immutable installation and verification primitives

import errno
import os
import shutil
import uuid
from contextlib import contextmanager
from dataclasses import replace
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, Literal, override

from sve_carddb import source_archive as archive
from sve_carddb.fetch.writer import (
    DiskFullError,
    Fetched,
    LocalState,
    Writer,
    WriteResult,
    sha256,
)
from sve_carddb.manifest import ExclusiveLock, Manifest, Resource, utcnow
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.store import (
    CorruptDataError,
    UnsafePathError,
    compress,
    decompress,
    resolve_within,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Generator


class Replacement(archive._Model):
    """A durable intent, not evidence that a fetch was committed."""

    replacement_format: Literal[1] = 1
    previous: archive.ResourceEvidence | None
    proposed: archive.ResourceEvidence
    manifest_sha256: str


class SourceBackup(archive._Model):
    """Completion evidence for one verified source closure in the backup store."""

    manifest_hashes: list[str]
    entry: archive.Entry


class RefreshWriter(Writer):
    """Replace latest only after durable old-source and candidate preservation."""

    def __init__(
        self,
        store: archive.ArchiveStore,
        manifest: Manifest,
        lock: ExclusiveLock,
        backup_root: Path,
        *,
        require_separate_device: bool = True,
    ) -> None:
        super().__init__(store.data_root, manifest, read_roots=store.read_roots)
        self.store = store
        self.backup_root = backup_root
        self._lock = lock
        archive._mkdir_safe(store.root)
        if any(
            left.resolve().is_relative_to(right.resolve())
            for left, right in (
                (backup_root, store.root),
                (store.root, backup_root),
                (backup_root, store.data_root),
                (store.data_root, backup_root),
            )
        ):
            raise archive.ArchiveError(
                "backup root must be outside latest data and the archive store"
            )
        archive._mkdir_safe(backup_root)
        if (
            require_separate_device
            and store.root.stat().st_dev == backup_root.stat().st_dev
        ):
            raise archive.ArchiveError("archive backup must be on a separate device")

    @contextmanager
    def _stage(self) -> Generator[Path]:
        with ExclusiveLock(self.store.root / ".lock"):
            stage = self.store.root / "staging" / uuid.uuid4().hex
            try:
                archive._mkdir_safe(stage)
                yield stage
            except OSError as exc:
                if exc.errno == errno.ENOSPC:
                    raise DiskFullError(
                        errno.ENOSPC, "disk full during source preservation"
                    ) from exc
                raise
            finally:
                if stage.exists():
                    shutil.rmtree(stage)

    def _snapshot(self, stage: Path, name: str) -> tuple[Path, str]:
        path = stage / name
        info = self._manifest.backup(path)
        with path.open("rb") as file:
            os.fsync(file.fileno())
        archive._fsync_dir(path.parent)
        return path, "sha256:" + info.sha256

    def _save_snapshot(self, snapshot: tuple[Path, str]) -> None:
        target = self.store.root / "manifests" / f"{snapshot[1][7:]}.sqlite"
        if target.exists():
            archive._require_hash(self.store.root, target, snapshot[1])
        else:
            archive._install_link(snapshot[0], target)

    def _existing(
        self, resource: Resource
    ) -> dict[str, tuple[archive.Descriptor, str]]:
        source_id = archive._version_id(
            archive._source_key(resource), archive._raw_hash(resource)
        )
        path = (
            self.store.root / "versions" / f"{source_id.removeprefix('src:v1:')}.json"
        )
        if not path.exists():
            return {}
        archive._require_safe_file(self.store.root, path)
        descriptor = archive._load_model(archive.Descriptor, path)
        descriptor_hash = digest(archive._canonical_model(descriptor))
        entry = archive.Entry(
            source_version_id=source_id,
            receipt_id=descriptor.first_receipt_id,
            descriptor_sha256=descriptor_hash,
            blob=archive.Blob(
                store_id=self.store.store_id,
                path=str(archive._blob_path(archive._raw_hash(resource))),
                sha256=archive._raw_hash(resource),
                bytes=resource.raw_bytes,
            ),
        )
        archive._verify_entry(self.store.root, self.store.store_id, entry, set(), set())
        return {source_id: (descriptor, descriptor_hash)}

    def _observe(self, resource: Resource, snapshot: tuple[Path, str]) -> archive.Entry:
        with Manifest.open_snapshot(snapshot[0]) as frozen:
            if frozen.resources.get(resource.url) != resource:
                raise archive.ArchiveError(
                    "observation differs from committed manifest snapshot"
                )
        blob = self.store.root / archive._blob_path(archive._raw_hash(resource))
        archive._require_hash(
            self.store.root, blob, archive._raw_hash(resource), resource.raw_bytes
        )
        pin = archive._Pinned(resource, None, None, blob, "reused")
        archive._prepare(pin)
        existing = self._existing(resource)
        metadata = archive._prepare_metadata(
            self.store, [pin], existing, snapshot[1], snapshot[0]
        )
        entry = metadata.entries[metadata.current[0].source_version_id]
        self._backup_entry(entry)
        return entry

    def _backup_entry(self, entry: archive.Entry) -> None:
        descriptor = archive._load_model(
            archive.Descriptor,
            self.store.root / "descriptors" / f"{entry.descriptor_sha256[7:]}.json",
        )
        paths = {
            PurePosixPath(entry.blob.path),
            PurePosixPath("descriptors", f"{entry.descriptor_sha256[7:]}.json"),
            PurePosixPath(
                "versions", f"{entry.source_version_id.removeprefix('src:v1:')}.json"
            ),
        }
        manifests: set[str] = set()
        for receipt_id in {entry.receipt_id, descriptor.first_receipt_id}:
            receipt_path = PurePosixPath("receipts", f"{receipt_id[7:]}.json")
            paths.add(receipt_path)
            receipt = archive._load_model(
                archive.Receipt, self.store.root / receipt_path
            )
            manifests.add(receipt.manifest_sha256)
            paths.add(
                PurePosixPath("manifests", f"{receipt.manifest_sha256[7:]}.sqlite")
            )
        for path in sorted(paths):
            archive._require_safe_file(self.store.root, self.store.root / path)
            archive._copy_immutable(self.store.root / path, self.backup_root / path)
        archive._verify_entry(
            self.backup_root, self.store.store_id, entry, set(), set()
        )
        receipt_data = archive._canonical_model(
            SourceBackup(manifest_hashes=sorted(manifests), entry=entry)
        )
        archive._install_bytes(
            self.backup_root / "refresh-backups" / f"{digest(receipt_data)[7:]}.json",
            receipt_data,
        )

    def _prepare_old(
        self, previous: Resource | None, stage: Path
    ) -> archive._Pinned | None:
        if previous is None:
            return None
        try:
            return archive._pin(self.store, stage, previous, 0)
        except (UnsafePathError, FileNotFoundError) as exc:
            raise archive.IncompleteBatchError(
                [
                    archive.Missing(
                        url=previous.url,
                        expected_raw_sha256=archive._raw_hash(previous),
                        reason="unsafe_path"
                        if isinstance(exc, UnsafePathError)
                        else "missing_raw",
                    )
                ]
            ) from exc

    def _preserve_old(
        self, pin: archive._Pinned | None, snapshot: tuple[Path, str]
    ) -> None:
        if pin is None:
            return
        try:
            self._verify_old_latest(pin)
            archive._prepare(pin)
        except archive.ArchiveRaceError:
            raise
        except (FileNotFoundError, CorruptDataError, archive.ArchiveError) as exc:
            raise archive.IncompleteBatchError(
                [
                    archive.Missing(
                        url=pin.resource.url,
                        expected_raw_sha256=archive._raw_hash(pin.resource),
                        reason="missing_raw"
                        if isinstance(exc, FileNotFoundError)
                        else "hash_mismatch",
                    )
                ]
            ) from exc
        existing = self._existing(pin.resource)
        metadata = archive._prepare_metadata(
            self.store, [pin], existing, snapshot[1], snapshot[0]
        )
        self._backup_entry(metadata.entries[metadata.current[0].source_version_id])

    def _verify_old_latest(self, pin: archive._Pinned) -> None:
        if (
            pin.target is not None
            and self.local_state(pin.resource.url) is not LocalState.TRUSTED
        ):
            raise CorruptDataError("old latest does not match the committed resource")

    def _candidate(
        self, fetched: Fetched, previous: Resource | None, stage: Path
    ) -> tuple[Resource, Path]:
        raw_hash = sha256(fetched.body)
        stored = compress(fetched.body) if fetched.compressed else fetched.body
        if fetched.compressed and decompress(stored) != fetched.body:
            raise CorruptDataError("compression round trip failed")
        now = utcnow()
        resource = Resource(
            url=fetched.url,
            region=fetched.region,
            kind=fetched.kind,
            path=fetched.path,
            sha256=raw_hash,
            raw_bytes=len(fetched.body),
            stored_bytes=len(stored),
            content_type=fetched.content_type,
            etag=fetched.etag,
            last_modified=fetched.last_modified,
            first_fetched_at=now if previous is None else previous.first_fetched_at,
            last_checked_at=now,
            last_changed_at=now
            if previous is None or previous.sha256 != raw_hash
            else previous.last_changed_at,
            archived_at=None,
        )
        raw_path = stage / "new.raw"
        archive._write_new(raw_path, fetched.body)
        blob = self.store.root / archive._blob_path(archive._raw_hash(resource))
        if blob.exists():
            archive._require_hash(
                self.store.root, blob, archive._raw_hash(resource), resource.raw_bytes
            )
        else:
            archive._require_hash(
                self.store.root,
                raw_path,
                archive._raw_hash(resource),
                resource.raw_bytes,
            )
            archive._install_link(raw_path, blob)
        target = resolve_within(self.store.data_root, fetched.path)
        target.parent.mkdir(parents=True, exist_ok=True)
        temporary = target.with_name(f"{target.name}.tmp-{uuid.uuid4().hex}")
        archive._write_new(temporary, stored)
        actual = temporary.read_bytes()
        if (decompress(actual) if fetched.compressed else actual) != fetched.body:
            raise CorruptDataError("prepared latest differs from fetched bytes")
        return resource, temporary

    @override
    def write(
        self,
        fetched: Fetched,
        *,
        request_id: int,
        rewrite: bool = False,
        in_transaction: Callable[[Resource], None] | None = None,
    ) -> WriteResult:
        """Preserve old evidence and new intent before replacing the latest inode."""
        self.check_path(fetched.url, fetched.path)
        previous = self._manifest.resources.get(fetched.url)
        with self._stage() as stage:
            snapshot = self._snapshot(stage, "before.sqlite")
            pin = self._prepare_old(previous, stage)
            try:
                with self._lock.suspended():
                    self._preserve_old(pin, snapshot)
                    resource, temporary = self._candidate(fetched, previous, stage)
                    self._save_snapshot(snapshot)
                    intent = Replacement(
                        previous=None
                        if previous is None
                        else archive._evidence(previous),
                        proposed=archive._evidence(resource),
                        manifest_sha256=snapshot[1],
                    )
                    intent_bytes = canonical(intent.model_dump(mode="json"))
                    intent_id = digest(intent_bytes)
                    archive._install_bytes(
                        self.store.root / "replacements" / f"{intent_id[7:]}.json",
                        intent_bytes,
                    )
                    candidate_stat = archive._file_identity(temporary.stat())
                self._check_unchanged(snapshot, stage, pin, fetched, previous)
                if archive._file_identity(temporary.stat()) != candidate_stat:
                    raise archive.ArchiveRaceError("prepared latest changed")
                target = resolve_within(self.store.data_root, fetched.path)
                if target != temporary.parent / fetched.path.name:
                    raise archive.ArchiveRaceError(
                        "latest destination changed during preparation"
                    )
                unchanged = (
                    previous is not None
                    and previous.sha256 == resource.sha256
                    and pin is not None
                    and pin.target is not None
                )
                rewritten = (
                    not unchanged
                    or rewrite
                    or previous is None
                    or previous.path != fetched.path
                )
                if rewritten:
                    temporary.replace(target)
                    archive._fsync_dir(temporary.parent)
                else:
                    assert previous is not None
                    temporary.unlink()
                    resource = replace(
                        resource, path=previous.path, stored_bytes=previous.stored_bytes
                    )
                self._record(resource, request_id, unchanged, in_transaction)
                after = self._snapshot(stage, "after.sqlite")
                with self._lock.suspended():
                    self._observe(resource, after)
                    self._complete(intent_id)
                return WriteResult(resource, changed=not unchanged, rewritten=rewritten)
            finally:
                if pin is not None and pin.fd is not None:
                    os.close(pin.fd)

    def _check_unchanged(
        self,
        snapshot: tuple[Path, str],
        stage: Path,
        pin: archive._Pinned | None,
        fetched: Fetched,
        previous: Resource | None,
    ) -> None:
        with Manifest.open_live(self.store.manifest_path) as live:
            final = live.backup(stage / "rechecked.sqlite")
            if (
                "sha256:" + final.sha256 != snapshot[1]
                or live.resources.get(fetched.url) != previous
            ):
                raise archive.ArchiveRaceError(
                    "manifest changed during replacement preparation"
                )
        if pin is not None and not archive._still_pinned(self.store, pin):
            raise archive.ArchiveRaceError(
                "old latest changed during replacement preparation"
            )
        self.check_path(fetched.url, fetched.path)

    def _complete(self, intent_id: str) -> None:
        archive._install_bytes(
            self.store.root / "replacement-completions" / f"{intent_id[7:]}.json",
            canonical({"replacement_sha256": intent_id}),
        )

    def _resume_reads(self) -> None:
        # Another crawler may have committed while metadata was being preserved.
        with self._manifest.transaction():
            pass

    def recover(self) -> int:
        """Restore the committed version; never commit an interrupted candidate."""
        recovered = 0
        with self._stage() as stage:
            for path in sorted((self.store.root / "replacements").glob("*.json")):
                archive._require_hash(self.store.root, path, "sha256:" + path.stem)
                intent = archive._load_model(Replacement, path)
                completed = self.store.root / "replacement-completions" / path.name
                if completed.exists():
                    archive._require_safe_file(self.store.root, completed)
                    if completed.read_bytes() != canonical(
                        {"replacement_sha256": "sha256:" + path.stem}
                    ):
                        raise archive.ArchiveError("replacement completion mismatch")
                    continue
                with self._lock.suspended():
                    self._validate_pending(intent)
                self._resume_reads()
                current = self._manifest.resources.get(intent.proposed.url)
                if current is not None:
                    self._recover_current(current, stage, recovered, intent)
                self._complete("sha256:" + path.stem)
                recovered += 1
        return recovered

    def _validate_pending(self, intent: Replacement) -> None:
        snapshot = (
            self.store.root
            / "manifests"
            / f"{archive._hex(intent.manifest_sha256)}.sqlite"
        )
        archive._require_hash(self.store.root, snapshot, intent.manifest_sha256)
        with Manifest.open_snapshot(snapshot) as frozen:
            previous = frozen.resources.get(intent.proposed.url)
            evidence = None if previous is None else archive._evidence(previous)
            if evidence != intent.previous:
                raise archive.ArchiveError(
                    "replacement intent differs from its manifest snapshot"
                )
        raw_hash = "sha256:" + intent.proposed.sha256
        archive._require_hash(
            self.store.root,
            self.store.root / archive._blob_path(raw_hash),
            raw_hash,
            intent.proposed.raw_bytes,
        )
        resolve_within(self.store.data_root, PurePosixPath(intent.proposed.path))

    def _recover_current(
        self, current: Resource, stage: Path, index: int, intent: Replacement
    ) -> None:
        allowed = {intent.proposed.sha256}
        if intent.previous is not None:
            allowed.add(intent.previous.sha256)
        snapshot = self._snapshot(stage, f"recovery-{index}.sqlite")
        target = resolve_within(self.store.data_root, current.path)
        before = archive._identity(target, target.stat()) if target.exists() else None
        with self._lock.suspended():
            if current.sha256 not in allowed and not self._existing(current):
                raise archive.ArchiveError(
                    "committed version is outside the replacement history"
                )
            temporary = self._prepare_restore(current)
            temporary_stat = (
                archive._file_identity(temporary.stat())
                if temporary is not None
                else None
            )
            self._observe(current, snapshot)
        with Manifest.open_live(self.store.manifest_path) as live:
            if live.resources.get(current.url) != current:
                raise archive.ArchiveRaceError("manifest changed during recovery")
        resolved = resolve_within(self.store.data_root, current.path)
        actual = (
            archive._identity(resolved, resolved.stat()) if resolved.exists() else None
        )
        if resolved != target or actual != before:
            raise archive.ArchiveRaceError("latest changed during recovery")
        if temporary is not None:
            if archive._file_identity(temporary.stat()) != temporary_stat:
                raise archive.ArchiveRaceError("prepared recovery file changed")
            temporary.replace(target)
            archive._fsync_dir(target.parent)
        self._resume_reads()

    def _prepare_restore(self, current: Resource) -> Path | None:
        raw_hash = archive._raw_hash(current)
        blob = self.store.root / archive._blob_path(raw_hash)
        archive._require_hash(self.store.root, blob, raw_hash, current.raw_bytes)
        if self.local_state(current.url) is LocalState.TRUSTED:
            return None
        raw = blob.read_bytes()
        stored = compress(raw) if current.path.suffix == ".zst" else raw
        if len(stored) != current.stored_bytes:
            raise archive.ArchiveError(
                "recovery encoding differs from committed resource"
            )
        target = resolve_within(self.store.data_root, current.path)
        target.parent.mkdir(parents=True, exist_ok=True)
        temporary = target.with_name(f"{target.name}.tmp-{uuid.uuid4().hex}")
        archive._write_new(temporary, stored)
        return temporary

    @override
    def mark_not_modified(self, url: str, *, request_id: int) -> Resource:
        """Keep the version ID while preserving the new committed 304 observation."""
        updated = super().mark_not_modified(url, request_id=request_id)
        with self._stage() as stage:
            snapshot = self._snapshot(stage, "not-modified.sqlite")
            pin = self._prepare_old(updated, stage)
            try:
                with self._lock.suspended():
                    self._preserve_old(pin, snapshot)
                self._resume_reads()
            finally:
                if pin is not None and pin.fd is not None:
                    os.close(pin.fd)
        return updated

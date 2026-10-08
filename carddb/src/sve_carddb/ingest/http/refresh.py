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

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.paths import UnsafePathError
from sve_carddb.ingest.archive import source_archive as archive
from sve_carddb.ingest.archive.manifest import ExclusiveLock, Manifest, Resource, utcnow
from sve_carddb.ingest.archive.store import (
    CorruptDataError,
    compress,
    decompress,
    resolve_within,
)
from sve_carddb.ingest.http.writer import (
    DiskFullError,
    Fetched,
    LocalState,
    Writer,
    WriteResult,
    sha256,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Generator


class Replacement(archive._Model):
    """A durable intent, not evidence that a fetch was committed."""

    replacement_format: Literal[1] = 1
    previous: archive.ResourceEvidence | None
    proposed: archive.ResourceEvidence
    manifest_sha256: str
    previous_stored_sha256: str | None = None
    proposed_stored_sha256: str | None = None


class CommittedReplacement(archive._Model):
    """A backed-up commit journal; formal observations are made at checkpoints."""

    replacement_sha256: str
    resource: archive.ResourceEvidence


class HistoryGap(archive._Model):
    """Explicit permission to start history after a known, unavailable version."""

    resource: archive.ResourceEvidence
    reason: str
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
        restore_root: Path | None = None,
        require_separate_device: bool = True,
    ) -> None:
        super().__init__(store.data_root, manifest, read_roots=store.read_roots)
        self.store = store
        self.backup_root = backup_root
        self._lock = lock
        self.restore_root = restore_root
        self.touched: set[tuple[str, str]] = set()
        self._baseline: tuple[Path, str] | None = None
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

        if restore_root is not None:
            if not restore_root.is_absolute():
                raise archive.ArchiveError("restore root must be absolute")
            for protected in (store.root, store.data_root, backup_root):
                if restore_root.resolve().is_relative_to(
                    protected.resolve()
                ) or protected.resolve().is_relative_to(restore_root.resolve()):
                    raise archive.ArchiveError(
                        "restore root must be separate from latest, archive and backup"
                    )
            archive._mkdir_safe(restore_root)

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
        if existing:
            descriptor, descriptor_hash = next(iter(existing.values()))
            self._backup_entry(
                archive.Entry(
                    source_version_id=descriptor.id,
                    receipt_id=descriptor.first_receipt_id,
                    descriptor_sha256=descriptor_hash,
                    blob=archive.Blob(
                        store_id=self.store.store_id,
                        path=str(archive._blob_path(archive._raw_hash(pin.resource))),
                        sha256=archive._raw_hash(pin.resource),
                        bytes=pin.resource.raw_bytes,
                    ),
                )
            )
            return
        with Manifest.open_snapshot(snapshot[0]) as frozen:
            if frozen.resources.get(pin.resource.url) != pin.resource:
                raise archive.ArchiveError(
                    "old observation differs from committed manifest snapshot"
                )
        metadata = archive._prepare_metadata(
            self.store, [pin], existing, snapshot[1], snapshot[0]
        )
        self._backup_entry(metadata.entries[metadata.current[0].source_version_id])

    def _verify_old_latest(self, pin: archive._Pinned) -> None:
        if (
            pin.target is not None
            and pin.candidate.name != "refetched.raw"
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
                    resource, temporary, intent_id, candidate_stat, old_trusted = (
                        self._prepare_intent(fetched, previous, pin, snapshot, stage)
                    )
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
                    and old_trusted
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
                self.touched.add((resource.region.value, resource.kind.value))
                with self._lock.suspended():
                    self._backup_committed(resource, intent_id)
                    self._journal(resource)
                    self._complete(intent_id)
                return WriteResult(resource, changed=not unchanged, rewritten=rewritten)
            finally:
                if pin is not None and pin.fd is not None:
                    os.close(pin.fd)

    def _prepare_intent(
        self,
        fetched: Fetched,
        previous: Resource | None,
        pin: archive._Pinned | None,
        snapshot: tuple[Path, str],
        stage: Path,
    ) -> tuple[Resource, Path, str, tuple[int, int, int, int, int], bool]:
        evidence_snapshot, old_evidence = self._intent_snapshot(
            snapshot, previous, fetched.url
        )
        gap = self._gap(previous)
        if gap is not None and pin is not None:
            if not pin.candidate.exists():
                archive._write_new(pin.candidate, b"")
            pin.candidate_stat = archive._file_identity(pin.candidate.stat())
        else:
            self._refetch_old(pin, fetched, stage)
            self._preserve_old(pin, evidence_snapshot)
        previous_stored = None
        if (
            previous is not None
            and pin is not None
            and pin.target is not None
            and pin.candidate.name != "refetched.raw"
            and gap is None
        ):
            previous_stored = self._save_encoding(pin.target, previous)
        resource, temporary = self._candidate(fetched, previous, stage)
        proposed_stored = self._save_encoding(temporary, resource)
        intent = Replacement(
            previous=old_evidence,
            proposed=archive._evidence(resource),
            manifest_sha256=evidence_snapshot[1],
            previous_stored_sha256=previous_stored,
            proposed_stored_sha256=proposed_stored,
        )
        intent_bytes = canonical(intent.model_dump(mode="json"))
        intent_id = digest(intent_bytes)
        archive._install_bytes(
            self.store.root / "replacements" / f"{intent_id[7:]}.json",
            intent_bytes,
        )
        candidate_stat = archive._file_identity(temporary.stat())
        old_trusted = (
            pin is not None
            and pin.target is not None
            and pin.candidate.name != "refetched.raw"
            and gap is None
        )
        return resource, temporary, intent_id, candidate_stat, old_trusted

    def _intent_snapshot(
        self, snapshot: tuple[Path, str], previous: Resource | None, url: str
    ) -> tuple[tuple[Path, str], archive.ResourceEvidence | None]:
        baseline = self._baseline
        if baseline is not None:
            with Manifest.open_snapshot(baseline[0]) as frozen:
                old = frozen.resources.get(url)
            same_version = (
                old is not None
                and previous is not None
                and (archive._source_key(old), old.sha256)
                == (archive._source_key(previous), previous.sha256)
            )
            if same_version and old != previous:
                assert previous is not None
                source_id = archive._version_id(
                    archive._source_key(previous), archive._raw_hash(previous)
                )
                same_version = (
                    self.store.root
                    / "versions"
                    / f"{source_id.removeprefix('src:v1:')}.json"
                ).exists()
            if (old is None and previous is None) or same_version:
                return baseline, None if old is None else archive._evidence(old)
        self._save_snapshot(snapshot)
        self._baseline = (
            self.store.root / "manifests" / f"{snapshot[1][7:]}.sqlite",
            snapshot[1],
        )
        return self._baseline, None if previous is None else archive._evidence(previous)

    def _backup_committed(self, resource: Resource, intent_id: str) -> None:
        intent_path = PurePosixPath("replacements", f"{intent_id[7:]}.json")
        archive._require_hash(self.store.root, self.store.root / intent_path, intent_id)
        intent = archive._load_model(Replacement, self.store.root / intent_path)
        paths = [
            archive._blob_path(archive._raw_hash(resource)),
            intent_path,
            PurePosixPath("manifests", f"{intent.manifest_sha256[7:]}.sqlite"),
        ]
        paths.extend(
            PurePosixPath("encodings", f"{archive._hex(raw_hash)}.bin")
            for raw_hash in (
                intent.previous_stored_sha256,
                intent.proposed_stored_sha256,
            )
            if raw_hash is not None
        )
        for path in paths:
            archive._require_safe_file(self.store.root, self.store.root / path)
            archive._copy_immutable(self.store.root / path, self.backup_root / path)
        archive._require_hash(
            self.backup_root,
            self.backup_root / paths[0],
            archive._raw_hash(resource),
            resource.raw_bytes,
        )
        archive._require_hash(
            self.backup_root, self.backup_root / intent_path, intent_id
        )
        archive._require_hash(
            self.backup_root, self.backup_root / paths[2], intent.manifest_sha256
        )
        data = archive._canonical_model(
            CommittedReplacement(
                replacement_sha256=intent_id, resource=archive._evidence(resource)
            )
        )
        path = PurePosixPath("replacement-commits", f"{digest(data)[7:]}.json")
        archive._install_bytes(self.store.root / path, data)
        archive._copy_immutable(self.store.root / path, self.backup_root / path)
        archive._require_hash(self.backup_root, self.backup_root / path, digest(data))
        for path in paths[3:]:
            archive._require_hash(
                self.backup_root, self.backup_root / path, "sha256:" + path.stem
            )

    def _save_encoding(self, path: Path, resource: Resource) -> str | None:
        if resource.path.suffix != ".zst":
            return None
        stored = path.read_bytes()
        raw = decompress(stored) if resource.path.suffix == ".zst" else stored
        if sha256(raw) != resource.sha256 or len(stored) != resource.stored_bytes:
            raise archive.ArchiveError(
                "stored representation differs from committed resource"
            )
        stored_hash = digest(stored)
        archive._install_bytes(
            self.store.root / "encodings" / f"{stored_hash[7:]}.bin", stored
        )
        return stored_hash

    def _journal(self, resource: Resource) -> None:
        data = archive._canonical_model(archive._evidence(resource))
        path = PurePosixPath("observations", f"{digest(data)[7:]}.json")
        archive._install_bytes(self.store.root / path, data)
        archive._copy_immutable(self.store.root / path, self.backup_root / path)
        archive._require_hash(self.backup_root, self.backup_root / path, digest(data))

    def finish_batch(self, result: archive.BatchResult) -> None:
        """Mark journal observations covered by a verified and restored sealed closure."""
        versions = {entry.source_version_id for entry in result.inventory.entries}
        with ExclusiveLock(self.store.root / ".lock"):
            for path in (self.store.root / "observations").glob("*.json"):
                archive._require_hash(self.store.root, path, "sha256:" + path.stem)
                evidence = archive._load_model(archive.ResourceEvidence, path)
                source_key = digest(
                    canonical(
                        {
                            "provider": evidence.region,
                            "kind": evidence.kind,
                            "url": evidence.url,
                        }
                    )
                )
                if (
                    archive._version_id(source_key, "sha256:" + evidence.sha256)
                    in versions
                ):
                    marker = self.store.root / "observation-seals" / path.name
                    if not marker.exists():
                        archive._install_bytes(
                            marker, canonical({"batch_id": result.batch_id})
                        )
        self.touched.clear()

    def _unfinished_observations(self) -> None:
        for path in (self.store.root / "observations").glob("*.json"):
            marker = self.store.root / "observation-seals" / path.name
            if marker.exists():
                archive._require_safe_file(self.store.root, marker)
                continue
            archive._require_hash(self.store.root, path, "sha256:" + path.stem)
            resource = archive._load_model(archive.ResourceEvidence, path)
            self.touched.add((resource.region, resource.kind))

    def checkpoint(self) -> None:
        """Observe touched current resources in one consistent committed snapshot."""
        if not self.touched:
            return
        with self._stage() as stage:
            snapshot = self._snapshot(stage, "checkpoint.sqlite")
            with Manifest.open_snapshot(snapshot[0]) as frozen:
                resources = [
                    resource
                    for resource in frozen.resources.all()
                    if (resource.region.value, resource.kind.value) in self.touched
                ]
            with self._lock.suspended():
                for resource in resources:
                    self._observe(resource, snapshot)
            self._resume_reads()

    def _refetch_old(
        self, pin: archive._Pinned | None, fetched: Fetched, stage: Path
    ) -> None:
        if pin is None or self.local_state(pin.resource.url) is LocalState.TRUSTED:
            return
        if (
            sha256(fetched.body) != pin.resource.sha256
            or len(fetched.body) != pin.resource.raw_bytes
        ):
            return
        path = stage / "refetched.raw"
        archive._write_new(path, fetched.body)
        if pin.fd is not None:
            os.close(pin.fd)
            pin.fd = None
        blob = self.store.root / archive._blob_path(archive._raw_hash(pin.resource))
        if blob.exists():
            archive._require_hash(
                self.store.root,
                blob,
                archive._raw_hash(pin.resource),
                pin.resource.raw_bytes,
            )
        else:
            archive._install_link(path, blob)
        pin.candidate = path
        pin.method = "reused"

    def acknowledge_gap(self, url: str, expected_hash: str, reason: str) -> None:
        """Record an explicit, hash-bound exception without erasing prior references."""
        previous = self._manifest.resources.get(url)
        if previous is None or archive._raw_hash(previous) != expected_hash:
            raise archive.ArchiveError(
                "history gap does not match the current resource"
            )
        if not reason.strip():
            raise archive.ArchiveError("history gap needs an operator reason")
        if self.local_state(url) is LocalState.TRUSTED or self._existing(previous):
            raise archive.ArchiveError(
                "available or archived versions cannot be history gaps"
            )
        if (url, previous.sha256) not in self._manifest.historical_raw_hashes():
            raise archive.ArchiveError(
                "history gap requires a persistent manifest reference"
            )
        blob = self.store.root / archive._blob_path(expected_hash)
        if blob.exists():
            raise archive.ArchiveError(
                "recover the preserved old blob before acknowledging a gap"
            )
        with self._stage() as stage:
            snapshot = self._snapshot(stage, "gap.sqlite")
            with self._lock.suspended():
                self._save_snapshot(snapshot)
                data = archive._canonical_model(
                    HistoryGap(
                        resource=archive._evidence(previous),
                        reason=reason,
                        manifest_sha256=snapshot[1],
                    )
                )
                path = self._gap_path(previous)
                archive._install_bytes(path, data)
                archive._copy_immutable(
                    path, self.backup_root / path.relative_to(self.store.root)
                )
                source = self.store.root / "manifests" / f"{snapshot[1][7:]}.sqlite"
                archive._copy_immutable(
                    source, self.backup_root / source.relative_to(self.store.root)
                )

    def _gap_path(self, resource: Resource) -> Path:
        source_id = archive._version_id(
            archive._source_key(resource), archive._raw_hash(resource)
        )
        return (
            self.store.root
            / "history-gaps"
            / f"{source_id.removeprefix('src:v1:')}.json"
        )

    def _gap(self, resource: Resource | None) -> HistoryGap | None:
        if resource is None:
            return None
        path = self._gap_path(resource)
        if not path.exists() or self._available_old(resource):
            return None
        archive._require_safe_file(self.store.root, path)
        gap = archive._load_model(HistoryGap, path)
        snapshot = (
            self.store.root
            / "manifests"
            / f"{archive._hex(gap.manifest_sha256)}.sqlite"
        )
        archive._require_hash(self.store.root, snapshot, gap.manifest_sha256)
        with Manifest.open_snapshot(snapshot) as frozen:
            old = frozen.resources.get(resource.url)
            if (
                old is None
                or archive._evidence(old) != gap.resource
                or old.sha256 != resource.sha256
            ):
                raise archive.ArchiveError("history gap evidence mismatch")
        return gap

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
            for abandoned in (self.store.root / "staging").iterdir():
                if abandoned != stage:
                    if abandoned.is_symlink() or not abandoned.is_dir():
                        raise archive.ArchiveError("unsafe abandoned staging path")
                    shutil.rmtree(abandoned)
            self._unfinished_observations()
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
                    with self._lock.suspended():
                        self._journal(current)
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
            self._check_recovery_latest(target, current, allowed)
            if self._gap(current) is not None:
                return
            self._recovery_encoding = self._select_encoding(current, intent)
            try:
                temporary = self._prepare_restore(current)
            finally:
                self._recovery_encoding = None
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
        self.touched.add((current.region.value, current.kind.value))
        self._resume_reads()

    @staticmethod
    def _check_recovery_latest(
        target: Path, current: Resource, allowed: set[str]
    ) -> None:
        if not target.exists():
            return
        stored = target.read_bytes()
        try:
            raw = decompress(stored) if current.path.suffix == ".zst" else stored
        except CorruptDataError as exc:
            raise archive.ArchiveError(
                "latest is outside the replacement history"
            ) from exc
        if sha256(raw) not in allowed | {current.sha256}:
            raise archive.ArchiveError("latest is outside the replacement history")

    @staticmethod
    def _select_encoding(current: Resource, intent: Replacement) -> str | None:
        if (current.sha256, current.stored_bytes) == (
            intent.proposed.sha256,
            intent.proposed.stored_bytes,
        ):
            return intent.proposed_stored_sha256
        if intent.previous is not None and current.sha256 == intent.previous.sha256:
            return intent.previous_stored_sha256
        return None

    def _prepare_restore(self, current: Resource) -> Path | None:
        raw_hash = archive._raw_hash(current)
        blob = self.store.root / archive._blob_path(raw_hash)
        archive._require_hash(self.store.root, blob, raw_hash, current.raw_bytes)
        if self.local_state(current.url) is LocalState.TRUSTED:
            return None
        raw = blob.read_bytes()
        encoding_hash = getattr(self, "_recovery_encoding", None)
        if encoding_hash is None:
            stored = compress(raw) if current.path.suffix == ".zst" else raw
        else:
            encoded = (
                self.store.root / "encodings" / f"{archive._hex(encoding_hash)}.bin"
            )
            archive._require_hash(self.store.root, encoded, encoding_hash)
            stored = encoded.read_bytes()
            decoded = decompress(stored) if current.path.suffix == ".zst" else stored
            if sha256(decoded) != current.sha256:
                raise archive.ArchiveError(
                    "recovery encoding differs from committed raw"
                )
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
        with self._stage() as stage:
            snapshot = self._snapshot(stage, "not-modified.sqlite")
            previous = self._manifest.resources.get(url)
            pin = self._prepare_old(previous, stage)
            try:
                updated = super().mark_not_modified(url, request_id=request_id)
                self.touched.add((updated.region.value, updated.kind.value))
                with self._lock.suspended():
                    assert previous is not None
                    source_id = archive._version_id(
                        archive._source_key(previous), archive._raw_hash(previous)
                    )
                    version = (
                        self.store.root
                        / "versions"
                        / f"{source_id.removeprefix('src:v1:')}.json"
                    )
                    if not version.exists():
                        # The pre-304 resource still matches the shared committed snapshot.
                        snapshot, _ = self._intent_snapshot(snapshot, previous, url)
                    self._preserve_old(pin, snapshot)
                    self._journal(updated)
                self._resume_reads()
            finally:
                if pin is not None and pin.fd is not None:
                    os.close(pin.fd)
        return updated

    def _available_old(self, resource: Resource) -> bool:
        if self.local_state(resource.url) is LocalState.TRUSTED:
            return True
        source_id = archive._version_id(
            archive._source_key(resource), archive._raw_hash(resource)
        )
        version = (
            self.store.root / "versions" / f"{source_id.removeprefix('src:v1:')}.json"
        )
        blob = self.store.root / archive._blob_path(archive._raw_hash(resource))
        return version.exists() or blob.exists()

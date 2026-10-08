"""Immutable raw-source batches built from a locked crawl manifest."""

import ctypes
import errno
import hashlib
import os
import shutil
import stat
import sys
import uuid
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, Literal, cast

from pydantic import BaseModel, ConfigDict, ValidationError

from sve_carddb.core.json import canonical, digest, parse
from sve_carddb.fetch.writer import LocalState
from sve_carddb.manifest import (
    READABLE_SCHEMA_VERSIONS,
    ExclusiveLock,
    Kind,
    Manifest,
    Region,
    Resource,
)
from sve_carddb.store import (
    CorruptDataError,
    UnsafePathError,
    decompress,
    resolve_within,
)

if TYPE_CHECKING:
    from collections.abc import Iterable, Sequence

_FICLONE = 0x40049409
_CHUNK = 1024 * 1024
_HEX = frozenset("0123456789abcdef")
_HEX_LENGTH = 64
_HASH_LENGTH = len("sha256:") + _HEX_LENGTH
_AT_FDCWD = -100
_RENAME_NOREPLACE = 1
_LINK_FALLBACK_ERRNOS = {errno.EXDEV, errno.EPERM, errno.EACCES, errno.EOPNOTSUPP}


class ArchiveError(RuntimeError):
    """A source batch could not be sealed or verified."""


class ArchiveRaceError(ArchiveError):
    """The live manifest or a pinned source changed during preparation."""


class IncompleteBatchError(ArchiveError):
    """Required current raw bytes are missing or unsafe."""

    def __init__(self, missing: list[Missing]) -> None:
        self.missing = missing
        super().__init__(f"{len(missing)} required raw sources are unavailable")


class _Model(BaseModel):
    model_config = ConfigDict(extra="forbid", strict=True)


class Scope(_Model):
    provider: str
    kind: str


class ResourceEvidence(_Model):
    url: str
    region: str
    kind: str
    path: str
    sha256: str
    raw_bytes: int
    stored_bytes: int
    content_type: str
    etag: str | None
    last_modified: str | None
    first_fetched_at: str
    last_checked_at: str
    last_changed_at: str
    archived_at: str | None


class Receipt(_Model):
    archive_format: Literal[1] = 1
    source_key: str
    raw_sha256: str
    observed_at: str
    manifest_sha256: str
    resource: ResourceEvidence


class Descriptor(_Model):
    archive_format: Literal[1] = 1
    id: str
    source_key: str
    provider: str
    kind: str
    url: str
    raw_sha256: str
    raw_bytes: int
    first_receipt_id: str


class Blob(_Model):
    store_id: str
    path: str
    sha256: str
    bytes: int


class Entry(_Model):
    source_version_id: str
    receipt_id: str
    descriptor_sha256: str
    blob: Blob


class Current(_Model):
    url: str
    source_version_id: str


class Missing(_Model):
    url: str
    expected_raw_sha256: str
    reason: Literal["missing_raw", "hash_mismatch", "unsafe_path", "missing_history"]


class ManifestPointer(_Model):
    path: Literal["manifest.sqlite"] = "manifest.sqlite"
    sha256: str
    bytes: int
    schema_version: int


class Inventory(_Model):
    input_format: Literal[1] = 1
    created_at: str
    manifest: ManifestPointer
    scope: list[Scope]
    current: list[Current]
    entries: list[Entry]
    missing: list[Missing]
    history_gaps: list[Missing]


class Seal(_Model):
    input_format: Literal[1] = 1
    inventory_sha256: str
    inventory_bytes: int


class BackupReceipt(_Model):
    manifest_sha256: str
    input_batch_ids: list[str]
    blob_hashes: list[str]
    verified_at: str


class RestoreCheckReceipt(_Model):
    store_id: str
    batch_id: str
    backup_receipt_sha256: str
    verified_at: str


class SourceGroup(_Model):
    provider: str
    kind: str
    versions: int
    referenced_raw_bytes: int


class CapacityReport(_Model):
    """Measured capacity, with unknown derived and historical growth kept explicit."""

    source_versions: int
    source_groups: list[SourceGroup]
    png_versions: int
    unique_raw_blobs: int
    referenced_raw_logical_bytes: int
    unique_raw_logical_bytes: int
    unique_raw_allocated_bytes: int
    unique_png_logical_bytes: int
    unique_png_allocated_bytes: int
    dedup_saved_bytes: int
    latest_logical_bytes: int
    latest_allocated_bytes: int
    latest_missing: int
    staging_upper_bytes: int
    backup_closure_bytes: int
    backup_free_bytes: int | None
    archive_free_bytes: int
    webp_logical_bytes: int | None
    webp_allocated_bytes: int | None
    daily_growth_bytes: None = None
    reserve_bytes: None = None


@dataclass(frozen=True, slots=True)
class ArchiveStore:
    data_root: Path
    manifest_path: Path
    lock_path: Path
    root: Path
    store_id: str
    read_roots: tuple[Path, ...] = ()

    def __post_init__(self) -> None:
        """Reject ambiguous names and archive/latest or allow-root overlap."""
        if not self.store_id or "/" in self.store_id or ".." in self.store_id:
            msg = "store_id must be a simple, stable name"
            raise ArchiveError(msg)
        archive = self.root.resolve()
        latest = self.data_root.resolve()
        if archive.is_relative_to(latest) or latest.is_relative_to(archive):
            msg = "archive store and latest data root must not overlap"
            raise ArchiveError(msg)
        if any(archive.is_relative_to(root.resolve()) for root in self.read_roots):
            msg = "archive store must be outside allowed read roots"
            raise ArchiveError(msg)


@dataclass(frozen=True, slots=True)
class BatchResult:
    batch_id: str
    inventory: Inventory
    path: Path
    hardlinks: int
    reflinks: int
    copied_bytes: int


@dataclass(slots=True)
class _Pinned:
    resource: Resource
    target: Path | None
    identity: tuple[str, int, int, int, int, int] | None
    candidate: Path
    method: Literal["hardlink", "reflink", "copy", "reused"]
    fd: int | None = None
    candidate_stat: tuple[int, int, int, int, int] | None = None


@dataclass(slots=True)
class _PreparedMetadata:
    current: list[Current]
    entries: dict[str, Entry]
    known: set[tuple[str, str]]


def seal_batch(
    store: ArchiveStore,
    *,
    scope: Sequence[Scope] | None = None,
    max_handles: int = 64,
    retries: int = 2,
) -> BatchResult:
    """Pin, prepare and seal current raw sources; never mutate the live manifest."""
    if max_handles <= 0 or retries < 0:
        msg = "max_handles must be positive and retries nonnegative"
        raise ValueError(msg)
    if not store.manifest_path.is_file():
        msg = f"manifest does not exist: {store.manifest_path}"
        raise ArchiveError(msg)
    _mkdir_safe(store.root)
    with ExclusiveLock(store.root / ".lock"):
        for attempt in range(retries + 1):
            stage = store.root / "staging" / uuid.uuid4().hex
            _mkdir_safe(stage)
            try:
                return _seal_attempt(store, stage, scope, max_handles)
            except ArchiveRaceError:
                if attempt == retries:
                    raise
            finally:
                shutil.rmtree(stage)
    msg = "archive preparation did not complete"
    raise ArchiveError(msg)


def _seal_attempt(  # ruff: ignore[complex-structure, too-many-branches, too-many-statements, too-many-locals] -- pin and seal phases share one retry boundary
    store: ArchiveStore, stage: Path, scope: Sequence[Scope] | None, max_handles: int
) -> BatchResult:
    with (
        ExclusiveLock(store.lock_path),
        Manifest.open_live(store.manifest_path) as live,
    ):
        first = live.backup(stage / "first.sqlite")
        resources = list(live.resources.all())
        history = live.historical_raw_hashes()
    selected_scope = _scope(resources, scope)
    if scope is None:
        selected_scope.update(_archived_scopes(store))
    selected = [
        item
        for item in resources
        if (item.region.value, item.kind.value) in selected_scope
    ]
    existing = _existing_versions(store, selected_scope)
    selected_urls = {item.url for item in selected}
    selected_urls.update(item.url for item, _ in existing.values())
    scoped_history = (
        history
        if scope is None
        else [(url, raw) for url, raw in history if url in selected_urls]
    )
    prepared: list[_Pinned] = []
    missing: list[Missing] = []
    for offset in range(0, len(selected), max_handles):
        chunk = selected[offset : offset + max_handles]
        with (
            ExclusiveLock(store.lock_path),
            Manifest.open_live(store.manifest_path) as live,
        ):
            if any(live.resources.get(item.url) != item for item in chunk):
                msg = "a resource changed while pinning source handles"
                raise ArchiveRaceError(msg)
            pins = []
            try:
                for index, item in enumerate(chunk):
                    try:
                        pins.append(_pin(store, stage, item, offset + index))
                    except (FileNotFoundError, UnsafePathError) as exc:
                        reason: Literal["missing_raw", "unsafe_path"] = (
                            "unsafe_path"
                            if isinstance(exc, UnsafePathError)
                            else "missing_raw"
                        )
                        missing.append(
                            Missing(
                                url=item.url,
                                expected_raw_sha256=_raw_hash(item),
                                reason=reason,
                            )
                        )
            except BaseException:
                for pin in pins:
                    if pin.fd is not None:
                        os.close(pin.fd)
                        pin.fd = None
                raise
        for pin in pins:
            try:
                _prepare(pin)
            except ArchiveRaceError:
                raise
            except (
                CorruptDataError,
                FileNotFoundError,
                UnsafePathError,
            ) as exc:
                # Mypy carries narrowing of the reused except binding across loops.
                prepared_reason: Literal[
                    "missing_raw", "hash_mismatch", "unsafe_path"
                ] = (
                    "unsafe_path"
                    if isinstance(cast("object", exc), UnsafePathError)
                    else "hash_mismatch"
                    if isinstance(cast("object", exc), CorruptDataError)
                    else "missing_raw"
                )
                missing.append(
                    Missing(
                        url=pin.resource.url,
                        expected_raw_sha256=_raw_hash(pin.resource),
                        reason=prepared_reason,
                    )
                )
            except ArchiveError:
                missing.append(
                    Missing(
                        url=pin.resource.url,
                        expected_raw_sha256=_raw_hash(pin.resource),
                        reason="hash_mismatch",
                    )
                )
            finally:
                if pin.fd is not None:
                    os.close(pin.fd)
                    pin.fd = None
        prepared.extend(pin for pin in pins if pin.candidate_stat is not None)
    if missing:
        raise IncompleteBatchError(missing)
    metadata = _prepare_metadata(
        store, prepared, existing, "sha256:" + first.sha256, stage / "first.sqlite"
    )
    with (
        ExclusiveLock(store.lock_path),
        Manifest.open_live(store.manifest_path) as live,
    ):
        final = live.backup(stage / "final.sqlite")
        if final.sha256 != first.sha256 or live.historical_raw_hashes() != history:
            msg = "manifest changed between source preparation and sealing"
            raise ArchiveRaceError(msg)
        for pin in prepared:
            if live.resources.get(
                pin.resource.url
            ) != pin.resource or not _still_pinned(store, pin):
                msg = f"source changed while preparing {pin.resource.url}"
                raise ArchiveRaceError(msg)
        return _publish(
            store,
            stage,
            prepared,
            metadata,
            selected_scope,
            scoped_history,
            "sha256:" + final.sha256,
        )


def _scope(
    resources: list[Resource], requested: Sequence[Scope] | None
) -> set[tuple[str, str]]:
    available = {(item.region.value, item.kind.value) for item in resources}
    values = (
        available
        if requested is None
        else {(item.provider, item.kind) for item in requested}
    )
    if not values or not values <= {
        (region.value, kind.value) for region in Region for kind in Kind
    }:
        msg = "scope must contain valid provider/kind pairs"
        raise ArchiveError(msg)
    return values


def _archived_scopes(store: ArchiveStore) -> set[tuple[str, str]]:
    return {
        (descriptor.provider, descriptor.kind)
        for path in (store.root / "versions").glob("*.json")
        for descriptor in [_load_model(Descriptor, path)]
    }


def _pin(  # ruff: ignore[complex-structure] -- pinning handles link, clone and FD ownership
    store: ArchiveStore, stage: Path, resource: Resource, index: int
) -> _Pinned:
    candidate = stage / "pins" / str(index)
    _mkdir_safe(candidate.parent)
    blob = store.root / _blob_path(_raw_hash(resource))
    fd: int | None = None
    try:  # ruff: ignore[too-many-statements-in-try-clause] -- FD ownership spans pinning fallbacks
        target = resolve_within(
            store.data_root, resource.path, also_allowed=store.read_roots
        )
        fd = os.open(target, os.O_RDONLY | os.O_NOFOLLOW)
        details = os.fstat(fd)
        if not stat.S_ISREG(details.st_mode):
            msg = f"source is not a regular file: {resource.path}"
            raise UnsafePathError(msg)
        actual = Path(f"/proc/self/fd/{fd}").readlink().resolve()
        if actual != target:
            msg = f"source path changed while opening {resource.path}"
            raise UnsafePathError(msg)
        if blob.exists():
            identity = _identity(target, details)
            return _Pinned(resource, target, identity, blob, "reused")
        try:
            os.link(target, candidate)
        except OSError as exc:
            if exc.errno not in _LINK_FALLBACK_ERRNOS:
                raise
            if _try_reflink(fd, candidate):
                identity = _identity(target, os.fstat(fd))
                return _Pinned(resource, target, identity, candidate, "reflink")
            pinned = _Pinned(
                resource, target, _identity(target, details), candidate, "copy", fd
            )
            fd = None
            return pinned
        linked = candidate.stat()
        current = os.fstat(fd)
        if (linked.st_dev, linked.st_ino) != (current.st_dev, current.st_ino):
            msg = f"source changed while pinning {resource.path}"
            raise ArchiveRaceError(msg)
        identity = _identity(target, current)
        return _Pinned(resource, target, identity, candidate, "hardlink")
    except FileNotFoundError:
        if blob.exists():
            return _Pinned(resource, None, None, blob, "reused")
        return _Pinned(resource, None, None, candidate, "copy")
    finally:
        if fd is not None:
            os.close(fd)


def _try_reflink(source_fd: int, target: Path) -> bool:
    if not sys.platform.startswith("linux"):
        return False
    import fcntl  # ruff: ignore[import-outside-top-level] -- fcntl does not exist on Windows.

    try:
        with target.open("xb") as output:
            fcntl.ioctl(output.fileno(), _FICLONE, source_fd)
            output.flush()
            os.fsync(output.fileno())
    except OSError:
        target.unlink(missing_ok=True)
        return False
    return True


def _prepare(pin: _Pinned) -> None:
    resource = pin.resource
    if pin.fd is not None:
        with pin.candidate.open("xb") as output:
            os.lseek(pin.fd, 0, os.SEEK_SET)
            while block := os.read(pin.fd, _CHUNK):
                output.write(block)
            output.flush()
            os.fsync(output.fileno())
    if not pin.candidate.exists():
        raise FileNotFoundError(pin.candidate)
    if pin.method != "reused" and resource.path.suffix == ".zst":
        raw = decompress(pin.candidate.read_bytes())
        raw_file = pin.candidate.with_suffix(".raw")
        _write_new(raw_file, raw)
        pin.candidate = raw_file
        actual = digest(raw)
        byte_count = len(raw)
    else:
        actual, byte_count = _hash_file(pin.candidate)
    if actual != _raw_hash(resource) or byte_count != resource.raw_bytes:
        msg = f"raw hash or byte count mismatch for {resource.url}"
        raise ArchiveError(msg)
    if pin.target is not None and pin.identity != _identity(
        pin.target, pin.target.stat()
    ):
        msg = f"source changed while reading {resource.url}"
        raise ArchiveRaceError(msg)
    pin.candidate_stat = _file_identity(pin.candidate.stat())


def _still_pinned(store: ArchiveStore, pin: _Pinned) -> bool:
    try:
        if pin.candidate_stat != _file_identity(pin.candidate.stat()):
            return False
    except FileNotFoundError:
        return False
    if pin.target is None:
        try:
            resolve_within(
                store.data_root, pin.resource.path, also_allowed=store.read_roots
            ).stat()
        except FileNotFoundError:
            return True
        return False
    try:
        target = resolve_within(
            store.data_root, pin.resource.path, also_allowed=store.read_roots
        )
        return target == pin.target and _identity(target, target.stat()) == pin.identity
    except FileNotFoundError, UnsafePathError:
        return False


def _identity(
    path: Path, details: os.stat_result
) -> tuple[str, int, int, int, int, int]:
    return (str(path), *_file_identity(details))


def _file_identity(details: os.stat_result) -> tuple[int, int, int, int, int]:
    return (
        details.st_dev,
        details.st_ino,
        details.st_size,
        details.st_mtime_ns,
        details.st_ctime_ns,
    )


def _raw_hash(resource: Resource) -> str:
    if len(resource.sha256) != _HEX_LENGTH or set(resource.sha256) - _HEX:
        msg = f"invalid manifest raw hash for {resource.url}"
        raise ArchiveError(msg)
    return "sha256:" + resource.sha256


def _blob_path(raw_hash: str) -> PurePosixPath:
    hex_digest = _hex(raw_hash)
    return PurePosixPath("raw", "sha256", hex_digest[:2], f"{hex_digest}.raw")


def _hex(value: str) -> str:
    if (
        not value.startswith("sha256:")
        or len(value) != _HASH_LENGTH
        or set(value[7:]) - _HEX
    ):
        msg = f"invalid archive hash: {value}"
        raise ArchiveError(msg)
    return value[7:]


def _hash_file(path: Path) -> tuple[str, int]:
    with path.open("rb") as source:
        hash_value = "sha256:" + hashlib.file_digest(source, "sha256").hexdigest()
    return hash_value, path.stat().st_size


def _write_new(path: Path, data: bytes) -> None:
    _mkdir_safe(path.parent)
    with path.open("xb") as output:
        output.write(data)
        output.flush()
        os.fsync(output.fileno())
    _fsync_dir(path.parent)


def _mkdir_safe(path: Path) -> None:
    missing: list[Path] = []
    current = path
    while current != current.parent:
        if current.is_symlink():
            msg = f"archive path contains a symlink: {current}"
            raise ArchiveError(msg)
        if not current.exists():
            missing.append(current)
        current = current.parent
    for directory in reversed(missing):
        directory.mkdir(exist_ok=True)
        _fsync_dir(directory.parent)


def _fsync_dir(path: Path) -> None:
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def _canonical_model(value: _Model) -> bytes:
    return canonical(value.model_dump(mode="json"))


def _load_model[T: _Model](model: type[T], path: Path) -> T:
    try:
        data = path.read_bytes()
        if canonical(parse(data)) != data:
            msg = f"noncanonical archive metadata: {path}"
            raise ArchiveError(msg)
        return model.model_validate_json(data)
    except (OSError, ValueError, ValidationError) as exc:
        msg = f"invalid archive metadata: {path}"
        raise ArchiveError(msg) from exc


def _install_bytes(path: Path, data: bytes) -> None:
    _mkdir_safe(path.parent)
    temporary = path.with_name(f".{path.name}.{uuid.uuid4().hex}.tmp")
    try:
        _write_new(temporary, data)
        try:
            os.link(temporary, path)
        except FileExistsError:
            if path.is_symlink() or path.read_bytes() != data:
                msg = f"immutable archive metadata differs: {path}"
                raise ArchiveError(msg) from None
        _fsync_dir(path.parent)
    finally:
        temporary.unlink(missing_ok=True)


def _install_link(
    source: Path, target: Path, *, trusted_existing: bool = False
) -> None:
    _mkdir_safe(target.parent)
    try:
        os.link(source, target)
    except FileExistsError:
        if not trusted_existing:
            msg = f"archive path appeared unexpectedly: {target}"
            raise ArchiveError(msg) from None
    _fsync_dir(target.parent)


def _evidence(resource: Resource) -> ResourceEvidence:
    return ResourceEvidence(
        url=resource.url,
        region=resource.region.value,
        kind=resource.kind.value,
        path=str(resource.path),
        sha256=resource.sha256,
        raw_bytes=resource.raw_bytes,
        stored_bytes=resource.stored_bytes,
        content_type=resource.content_type,
        etag=resource.etag,
        last_modified=resource.last_modified,
        first_fetched_at=resource.first_fetched_at.isoformat(),
        last_checked_at=resource.last_checked_at.isoformat(),
        last_changed_at=resource.last_changed_at.isoformat(),
        archived_at=None
        if resource.archived_at is None
        else resource.archived_at.isoformat(),
    )


def _source_key(resource: Resource) -> str:
    return digest(
        canonical(
            {
                "provider": resource.region.value,
                "kind": resource.kind.value,
                "url": resource.url,
            }
        )
    )


def _version_id(source_key: str, raw_hash: str) -> str:
    return "src:v1:" + _hex(
        digest(canonical({"source_key": source_key, "raw_sha256": raw_hash}))
    )


def _existing_versions(
    store: ArchiveStore, scope: set[tuple[str, str]]
) -> dict[str, tuple[Descriptor, str]]:
    result: dict[str, tuple[Descriptor, str]] = {}
    checked_blobs: set[str] = set()
    checked_manifests: set[str] = set()
    for path in sorted((store.root / "versions").glob("*.json")):
        descriptor = _load_model(Descriptor, path)
        descriptor_hash = digest(_canonical_model(descriptor))
        if path.stem != descriptor.id.removeprefix("src:v1:"):
            msg = f"version index does not match descriptor: {path}"
            raise ArchiveError(msg)
        if descriptor.id in result:
            msg = f"duplicate archived source version: {descriptor.id}"
            raise ArchiveError(msg)
        if (descriptor.provider, descriptor.kind) in scope:
            _verify_entry(
                store.root,
                store.store_id,
                Entry(
                    source_version_id=descriptor.id,
                    receipt_id=descriptor.first_receipt_id,
                    descriptor_sha256=descriptor_hash,
                    blob=Blob(
                        store_id=store.store_id,
                        path=str(_blob_path(descriptor.raw_sha256)),
                        sha256=descriptor.raw_sha256,
                        bytes=descriptor.raw_bytes,
                    ),
                ),
                checked_blobs,
                checked_manifests,
            )
            result[descriptor.id] = (descriptor, descriptor_hash)
    return result


def _prepare_metadata(  # ruff: ignore[too-many-locals] -- install immutable source metadata outside the manifest lock
    store: ArchiveStore,
    prepared: list[_Pinned],
    existing: dict[str, tuple[Descriptor, str]],
    manifest_sha: str,
    source_db: Path,
) -> _PreparedMetadata:
    archived_db = store.root / "manifests" / f"{manifest_sha[7:]}.sqlite"
    if archived_db.exists():
        if _hash_file(archived_db)[0] != manifest_sha:
            msg = f"archived manifest differs: {archived_db}"
            raise ArchiveError(msg)
    else:
        _install_link(source_db, archived_db)
    current: list[Current] = []
    entries: dict[str, Entry] = {}
    known = {(item.url, item.raw_sha256) for item, _ in existing.values()}
    for source_id, (descriptor, descriptor_hash) in existing.items():
        entries[source_id] = Entry(
            source_version_id=source_id,
            receipt_id=descriptor.first_receipt_id,
            descriptor_sha256=descriptor_hash,
            blob=Blob(
                store_id=store.store_id,
                path=str(_blob_path(descriptor.raw_sha256)),
                sha256=descriptor.raw_sha256,
                bytes=descriptor.raw_bytes,
            ),
        )
    installed: set[Path] = set()
    for pin in prepared:
        resource = pin.resource
        raw_hash = _raw_hash(resource)
        key = _source_key(resource)
        source_id = _version_id(key, raw_hash)
        blob_path = store.root / _blob_path(raw_hash)
        _install_link(
            pin.candidate,
            blob_path,
            trusted_existing=pin.method == "reused" or blob_path in installed,
        )
        installed.add(blob_path)
        receipt = Receipt(
            source_key=key,
            raw_sha256=raw_hash,
            observed_at=datetime.now(UTC).isoformat(),
            manifest_sha256=manifest_sha,
            resource=_evidence(resource),
        )
        receipt_bytes = _canonical_model(receipt)
        receipt_id = digest(receipt_bytes)
        _install_bytes(
            store.root / "receipts" / f"{receipt_id[7:]}.json", receipt_bytes
        )
        if source_id in existing:
            descriptor, descriptor_hash = existing[source_id]
            if (
                descriptor.source_key,
                descriptor.raw_sha256,
                descriptor.url,
                descriptor.raw_bytes,
            ) != (key, raw_hash, resource.url, resource.raw_bytes):
                msg = f"archived source identity collision: {source_id}"
                raise ArchiveError(msg)
        else:
            descriptor = Descriptor(
                id=source_id,
                source_key=key,
                provider=resource.region.value,
                kind=resource.kind.value,
                url=resource.url,
                raw_sha256=raw_hash,
                raw_bytes=resource.raw_bytes,
                first_receipt_id=receipt_id,
            )
            descriptor_bytes = _canonical_model(descriptor)
            descriptor_hash = digest(descriptor_bytes)
            _install_bytes(
                store.root / "descriptors" / f"{descriptor_hash[7:]}.json",
                descriptor_bytes,
            )
            _install_link(
                store.root / "descriptors" / f"{descriptor_hash[7:]}.json",
                store.root / "versions" / f"{source_id.removeprefix('src:v1:')}.json",
            )
            existing[source_id] = (descriptor, descriptor_hash)
        entries[source_id] = Entry(
            source_version_id=source_id,
            receipt_id=receipt_id,
            descriptor_sha256=descriptor_hash,
            blob=Blob(
                store_id=store.store_id,
                path=str(_blob_path(raw_hash)),
                sha256=raw_hash,
                bytes=resource.raw_bytes,
            ),
        )
        current.append(Current(url=resource.url, source_version_id=source_id))
        known.add((resource.url, raw_hash))
    for pin in prepared:
        _refresh_after_own_link(store, pin)
    return _PreparedMetadata(current=current, entries=entries, known=known)


def _refresh_after_own_link(store: ArchiveStore, pin: _Pinned) -> None:
    """Account for ctime changed by linking a verified candidate into the store."""
    candidate = _file_identity(pin.candidate.stat())
    blob = store.root / _blob_path(_raw_hash(pin.resource))
    candidate_linked = (
        pin.method != "reused"
        and blob.exists()
        and candidate[:2] == _file_identity(blob.stat())[:2]
    )
    if pin.candidate_stat is None or (
        candidate[:-1] != pin.candidate_stat[:-1]
        if candidate_linked
        else candidate != pin.candidate_stat
    ):
        msg = f"prepared source changed while installing {pin.resource.url}"
        raise ArchiveRaceError(msg)
    pin.candidate_stat = candidate
    if pin.target is None or pin.identity is None:
        return
    try:
        target = resolve_within(
            store.data_root, pin.resource.path, also_allowed=store.read_roots
        )
        identity = _identity(target, target.stat())
    except (FileNotFoundError, UnsafePathError) as exc:
        msg = f"latest source changed while installing {pin.resource.url}"
        raise ArchiveRaceError(msg) from exc
    same_hardlink = pin.method == "hardlink" and identity[1:3] == candidate[:2]
    if (
        identity[:-1] != pin.identity[:-1]
        if same_hardlink
        else identity != pin.identity
    ):
        msg = f"latest source changed while installing {pin.resource.url}"
        raise ArchiveRaceError(msg)
    pin.identity = identity


def _publish(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- sealed inventory needs prepared evidence
    store: ArchiveStore,
    stage: Path,
    prepared: list[_Pinned],
    metadata: _PreparedMetadata,
    scope: set[tuple[str, str]],
    history: list[tuple[str, str]],
    manifest_sha: str,
) -> BatchResult:
    batch = stage / "batch"
    _mkdir_safe(batch)
    final_db = stage / "final.sqlite"
    _install_link(
        store.root / "manifests" / f"{_hex(manifest_sha)}.sqlite",
        batch / "manifest.sqlite",
    )
    gaps = [
        Missing(url=url, expected_raw_sha256="sha256:" + raw, reason="missing_history")
        for url, raw in history
        if (url, "sha256:" + raw) not in metadata.known
        and len(raw) == _HEX_LENGTH
        and not set(raw) - _HEX
    ]
    with Manifest.open_snapshot(final_db) as frozen:
        schema_version = frozen.schema_version
    inventory = Inventory(
        created_at=datetime.now(UTC).isoformat(),
        manifest=ManifestPointer(
            sha256=manifest_sha,
            bytes=final_db.stat().st_size,
            schema_version=schema_version,
        ),
        scope=[Scope(provider=provider, kind=kind) for provider, kind in sorted(scope)],
        current=sorted(metadata.current, key=lambda item: item.url),
        entries=[metadata.entries[key] for key in sorted(metadata.entries)],
        missing=[],
        history_gaps=sorted(
            gaps, key=lambda item: (item.url, item.expected_raw_sha256)
        ),
    )
    inventory_bytes = _canonical_model(inventory)
    batch_id = digest(inventory_bytes)
    _write_new(batch / "inventory.json", inventory_bytes)
    seal = Seal(inventory_sha256=batch_id, inventory_bytes=len(inventory_bytes))
    _write_new(batch / "seal.json", _canonical_model(seal))
    _fsync_dir(batch)
    destination = store.root / "batches" / _hex(batch_id)
    _mkdir_safe(destination.parent)
    if destination.exists():
        if (destination / "inventory.json").read_bytes() != inventory_bytes:
            msg = f"existing batch ID has different inventory: {batch_id}"
            raise ArchiveError(msg)
    else:
        _rename_no_replace(batch, destination)
        _fsync_dir(destination.parent)
    return BatchResult(
        batch_id=batch_id,
        inventory=inventory,
        path=destination,
        hardlinks=sum(pin.method == "hardlink" for pin in prepared),
        reflinks=sum(pin.method == "reflink" for pin in prepared),
        copied_bytes=sum(
            pin.resource.stored_bytes for pin in prepared if pin.method == "copy"
        ),
    )


def _rename_no_replace(source: Path, destination: Path) -> None:
    rename = ctypes.CDLL(None, use_errno=True).renameat2
    rename.argtypes = (
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_int,
        ctypes.c_char_p,
        ctypes.c_uint,
    )
    rename.restype = ctypes.c_int
    if rename(
        _AT_FDCWD,
        os.fsencode(source),
        _AT_FDCWD,
        os.fsencode(destination),
        _RENAME_NOREPLACE,
    ):
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error), destination)


def verify_batch(root: Path, store_id: str, batch_id: str) -> Inventory:
    """Verify a sealed batch and every raw, receipt and manifest it references."""
    batch = root / "batches" / _hex(batch_id)
    _require_safe_file(root, batch / "seal.json")
    seal = _load_model(Seal, batch / "seal.json")
    inventory_path = batch / "inventory.json"
    _require_safe_file(root, inventory_path)
    data = inventory_path.read_bytes()
    if (
        seal.inventory_sha256 != batch_id
        or seal.inventory_bytes != len(data)
        or digest(data) != batch_id
    ):
        msg = "batch seal does not match inventory"
        raise ArchiveError(msg)
    inventory = _load_model(Inventory, inventory_path)
    if inventory.missing or inventory.scope != sorted(
        inventory.scope, key=lambda item: (item.provider, item.kind)
    ):
        msg = "sealed batch has missing inputs or unsorted scope"
        raise ArchiveError(msg)
    manifest_path = batch / inventory.manifest.path
    _require_hash(
        root, manifest_path, inventory.manifest.sha256, inventory.manifest.bytes
    )
    if inventory.manifest.schema_version not in READABLE_SCHEMA_VERSIONS:
        msg = "sealed manifest schema is not supported"
        raise ArchiveError(msg)
    with Manifest.open_snapshot(manifest_path) as snapshot:
        if snapshot.schema_version != inventory.manifest.schema_version:
            raise ArchiveError(
                "Inventory manifest version differs from its frozen database"
            )
        if snapshot.integrity_check() != "ok":
            msg = "sealed manifest failed integrity_check"
            raise ArchiveError(msg)
        scope = {(item.provider, item.kind) for item in inventory.scope}
        resources = [
            item
            for item in snapshot.resources.all()
            if (item.region.value, item.kind.value) in scope
        ]
        historical = set(snapshot.historical_raw_hashes())
    expected_current = [
        Current(
            url=item.url,
            source_version_id=_version_id(_source_key(item), _raw_hash(item)),
        )
        for item in resources
    ]
    if inventory.current != expected_current:
        msg = "inventory current set differs from the sealed manifest"
        raise ArchiveError(msg)
    entries = {item.source_version_id: item for item in inventory.entries}
    if len(entries) != len(inventory.entries) or list(entries) != sorted(entries):
        msg = "inventory entries are duplicated or unsorted"
        raise ArchiveError(msg)
    if any(item.source_version_id not in entries for item in inventory.current):
        msg = "current source has no archived entry"
        raise ArchiveError(msg)
    checked_blobs: set[str] = set()
    checked_manifests: set[str] = set()
    for entry in inventory.entries:
        _verify_entry(root, store_id, entry, checked_blobs, checked_manifests)
    _verify_history_gaps(root, inventory, historical)
    _require_hash(
        root,
        root / "manifests" / f"{_hex(inventory.manifest.sha256)}.sqlite",
        inventory.manifest.sha256,
        inventory.manifest.bytes,
    )
    return inventory


def _verify_history_gaps(
    root: Path, inventory: Inventory, historical: set[tuple[str, str]]
) -> None:
    known_versions = {
        (descriptor.url, _hex(descriptor.raw_sha256))
        for entry in inventory.entries
        for descriptor in [
            _load_model(
                Descriptor,
                root / "descriptors" / f"{_hex(entry.descriptor_sha256)}.json",
            )
        ]
    }
    if inventory.history_gaps != sorted(
        inventory.history_gaps, key=lambda item: (item.url, item.expected_raw_sha256)
    ):
        msg = "history gaps are not sorted"
        raise ArchiveError(msg)
    seen_gaps: set[tuple[str, str]] = set()
    for gap in inventory.history_gaps:
        identity = (gap.url, _hex(gap.expected_raw_sha256))
        if (
            gap.reason != "missing_history"
            or identity not in historical
            or identity in known_versions
            or identity in seen_gaps
        ):
            msg = f"history gap is not supported by the sealed manifest: {gap.url}"
            raise ArchiveError(msg)
        seen_gaps.add(identity)


def _verify_entry(
    root: Path,
    store_id: str,
    entry: Entry,
    checked_blobs: set[str],
    checked_manifests: set[str],
) -> None:
    if entry.blob.store_id != store_id or entry.blob.path != str(
        _blob_path(entry.blob.sha256)
    ):
        msg = f"invalid archive blob locator: {entry.source_version_id}"
        raise ArchiveError(msg)
    descriptor_path = root / "descriptors" / f"{_hex(entry.descriptor_sha256)}.json"
    _require_hash(root, descriptor_path, entry.descriptor_sha256)
    descriptor = _load_model(Descriptor, descriptor_path)
    if (
        descriptor.id != entry.source_version_id
        or descriptor.raw_sha256 != entry.blob.sha256
        or descriptor.raw_bytes != entry.blob.bytes
        or descriptor.id != _version_id(descriptor.source_key, descriptor.raw_sha256)
        or descriptor.source_key
        != digest(
            canonical(
                {
                    "provider": descriptor.provider,
                    "kind": descriptor.kind,
                    "url": descriptor.url,
                }
            )
        )
    ):
        msg = f"source descriptor identity mismatch: {entry.source_version_id}"
        raise ArchiveError(msg)
    version_path = root / "versions" / f"{descriptor.id.removeprefix('src:v1:')}.json"
    _require_hash(root, version_path, entry.descriptor_sha256)
    for receipt_id in {entry.receipt_id, descriptor.first_receipt_id}:
        receipt_path = root / "receipts" / f"{_hex(receipt_id)}.json"
        _require_hash(root, receipt_path, receipt_id)
        receipt = _load_model(Receipt, receipt_path)
        identity_matches = (
            receipt.source_key == descriptor.source_key
            and receipt.raw_sha256 == descriptor.raw_sha256
            and receipt.resource.url == descriptor.url
            and receipt.resource.region == descriptor.provider
        )
        raw_matches = (
            receipt.resource.kind == descriptor.kind
            and "sha256:" + receipt.resource.sha256 == descriptor.raw_sha256
            and receipt.resource.raw_bytes == descriptor.raw_bytes
        )
        if not identity_matches or not raw_matches:
            msg = f"receipt does not describe source version: {receipt_id}"
            raise ArchiveError(msg)
        manifest_hash = receipt.manifest_sha256
        if manifest_hash not in checked_manifests:
            _require_hash(
                root,
                root / "manifests" / f"{_hex(manifest_hash)}.sqlite",
                manifest_hash,
            )
            checked_manifests.add(manifest_hash)
    if entry.blob.sha256 not in checked_blobs:
        _require_hash(root, root / entry.blob.path, entry.blob.sha256, entry.blob.bytes)
        checked_blobs.add(entry.blob.sha256)


def _require_safe_file(root: Path, path: Path) -> None:
    base = root.resolve()
    try:
        parts = path.relative_to(root).parts
    except ValueError as exc:
        msg = f"archive path is outside store: {path}"
        raise ArchiveError(msg) from exc
    if not parts or any(part in {"", ".", ".."} for part in parts):
        msg = f"invalid archive path: {path}"
        raise ArchiveError(msg)
    current = base
    for part in parts:
        current /= part
        if current.is_symlink():
            msg = f"archive closure contains a symlink: {current}"
            raise ArchiveError(msg)
    if not current.is_file():
        msg = f"archive closure is missing: {current}"
        raise ArchiveError(msg)


def _require_hash(
    root: Path, path: Path, expected: str, size: int | None = None
) -> None:
    _require_safe_file(root, path)
    actual = _hash_file(path)
    if actual[0] != expected or (size is not None and actual[1] != size):
        msg = f"archive file hash or size mismatch: {path}"
        raise ArchiveError(msg)


def _closure_paths(
    root: Path, batch_id: str, inventory: Inventory
) -> list[PurePosixPath]:
    batch = PurePosixPath("batches", _hex(batch_id))
    paths = {batch / "inventory.json", batch / "seal.json", batch / "manifest.sqlite"}
    for entry in inventory.entries:
        paths.add(PurePosixPath(entry.blob.path))
        paths.add(PurePosixPath("descriptors", f"{_hex(entry.descriptor_sha256)}.json"))
        paths.add(
            PurePosixPath(
                "versions", f"{entry.source_version_id.removeprefix('src:v1:')}.json"
            )
        )
        descriptor = _load_model(
            Descriptor, root / "descriptors" / f"{_hex(entry.descriptor_sha256)}.json"
        )
        for receipt_id in {entry.receipt_id, descriptor.first_receipt_id}:
            paths.add(PurePosixPath("receipts", f"{_hex(receipt_id)}.json"))
            receipt = _load_model(
                Receipt, root / "receipts" / f"{_hex(receipt_id)}.json"
            )
            paths.add(
                PurePosixPath("manifests", f"{_hex(receipt.manifest_sha256)}.sqlite")
            )
    paths.add(PurePosixPath("manifests", f"{_hex(inventory.manifest.sha256)}.sqlite"))
    return sorted(paths)


def capacity_report(  # ruff: ignore[too-many-locals] -- reports independent measured quantities
    store: ArchiveStore,
    batch_id: str,
    *,
    max_handles: int = 64,
    webp_root: Path | None = None,
    backup_root: Path | None = None,
) -> CapacityReport:
    """Measure a sealed batch and latest cache without changing source state."""
    if max_handles <= 0:
        msg = "max_handles must be positive"
        raise ValueError(msg)
    inventory = verify_batch(store.root, store.store_id, batch_id)
    descriptors = [
        _load_model(
            Descriptor,
            store.root / "descriptors" / f"{_hex(entry.descriptor_sha256)}.json",
        )
        for entry in inventory.entries
    ]
    raw_paths = {
        entry.blob.sha256: store.root / entry.blob.path for entry in inventory.entries
    }
    png_hashes = {
        item.raw_sha256 for item in descriptors if item.kind == Kind.IMAGE.value
    }
    raw_logical, raw_allocated = _sizes(raw_paths.values())
    png_logical, png_allocated = _sizes(
        raw_paths[hash_value] for hash_value in png_hashes
    )
    with Manifest.open_snapshot(
        store.root / "batches" / _hex(batch_id) / "manifest.sqlite"
    ) as snapshot:
        scope = {(item.provider, item.kind) for item in inventory.scope}
        resources = [
            item
            for item in snapshot.resources.all()
            if (item.region.value, item.kind.value) in scope
        ]
    latest_paths: list[Path] = []
    for resource in resources:
        try:
            path = resolve_within(
                store.data_root, resource.path, also_allowed=store.read_roots
            )
            path.stat()
        except FileNotFoundError, UnsafePathError:
            continue
        latest_paths.append(path)
    latest_logical, latest_allocated = _sizes(latest_paths)
    webp_logical, webp_allocated = (
        _sizes(webp_root.rglob("*.webp"))
        if webp_root is not None and webp_root.exists()
        else (None, None)
    )
    referenced = sum(item.raw_bytes for item in descriptors)
    grouped: dict[tuple[str, str], tuple[int, int]] = {}
    for item in descriptors:
        key = (item.provider, item.kind)
        count, size = grouped.get(key, (0, 0))
        grouped[key] = (count + 1, size + item.raw_bytes)
    return CapacityReport(
        source_versions=len(descriptors),
        source_groups=[
            SourceGroup(
                provider=provider, kind=kind, versions=count, referenced_raw_bytes=size
            )
            for (provider, kind), (count, size) in sorted(grouped.items())
        ],
        png_versions=sum(item.kind == Kind.IMAGE.value for item in descriptors),
        unique_raw_blobs=len(raw_paths),
        referenced_raw_logical_bytes=referenced,
        unique_raw_logical_bytes=raw_logical,
        unique_raw_allocated_bytes=raw_allocated,
        unique_png_logical_bytes=png_logical,
        unique_png_allocated_bytes=png_allocated,
        dedup_saved_bytes=referenced - raw_logical,
        latest_logical_bytes=latest_logical,
        latest_allocated_bytes=latest_allocated,
        latest_missing=len(resources) - len(latest_paths),
        staging_upper_bytes=sum(
            sorted(
                (item.stored_bytes + item.raw_bytes for item in resources), reverse=True
            )[:max_handles]
        ),
        backup_closure_bytes=sum(
            (store.root / path).stat().st_size
            for path in _closure_paths(store.root, batch_id, inventory)
        ),
        backup_free_bytes=shutil.disk_usage(backup_root).free
        if backup_root is not None and backup_root.exists()
        else None,
        archive_free_bytes=shutil.disk_usage(store.root).free,
        webp_logical_bytes=webp_logical,
        webp_allocated_bytes=webp_allocated,
    )


def _sizes(paths: Iterable[Path]) -> tuple[int, int]:
    logical = allocated = 0
    for path in paths:
        info = path.stat()
        logical += info.st_size
        allocated += info.st_blocks * 512
    return logical, allocated


def backup_batch(
    store: ArchiveStore,
    backup_root: Path,
    batch_id: str,
    *,
    require_separate_device: bool = True,
) -> BackupReceipt:
    """Copy the physical archive closure before writing a backup completion receipt."""
    if backup_root.resolve().is_relative_to(
        store.data_root.resolve()
    ) or backup_root.resolve().is_relative_to(store.root.resolve()):
        msg = "backup root must be outside latest data and the archive store"
        raise ArchiveError(msg)
    inventory = verify_batch(store.root, store.store_id, batch_id)
    _mkdir_safe(backup_root)
    if (
        require_separate_device
        and store.root.stat().st_dev == backup_root.stat().st_dev
    ):
        msg = "archive backup must be on a separate device"
        raise ArchiveError(msg)
    _copy_closure(store.root, backup_root, batch_id, inventory)
    verify_batch(backup_root, store.store_id, batch_id)
    receipt_path = backup_root / "backups" / f"{_hex(batch_id)}.json"
    expected_blobs = sorted({item.blob.sha256 for item in inventory.entries})
    if receipt_path.exists():
        _require_safe_file(backup_root, receipt_path)
        existing_receipt = _load_model(BackupReceipt, receipt_path)
        if (
            existing_receipt.manifest_sha256 != inventory.manifest.sha256
            or existing_receipt.input_batch_ids != [batch_id]
            or existing_receipt.blob_hashes != expected_blobs
        ):
            msg = "existing backup receipt does not match the sealed batch"
            raise ArchiveError(msg)
        return existing_receipt
    receipt = BackupReceipt(
        manifest_sha256=inventory.manifest.sha256,
        input_batch_ids=[batch_id],
        blob_hashes=expected_blobs,
        verified_at=datetime.now(UTC).isoformat(),
    )
    _install_bytes(receipt_path, _canonical_model(receipt))
    return receipt


def _copy_closure(
    source_root: Path, destination: Path, batch_id: str, inventory: Inventory
) -> None:
    batch_manifest = PurePosixPath("batches", _hex(batch_id), "manifest.sqlite")
    for relative in _closure_paths(source_root, batch_id, inventory):
        if relative != batch_manifest:
            _copy_immutable(source_root / relative, destination / relative)
    manifest = destination / "manifests" / f"{_hex(inventory.manifest.sha256)}.sqlite"
    _require_hash(destination, manifest, inventory.manifest.sha256)
    target = destination / batch_manifest
    if target.exists() or target.is_symlink():
        _require_hash(destination, target, inventory.manifest.sha256)
    else:
        # Link only within the copied closure; the backup must own independent bytes.
        _install_link(manifest, target)


def restore_backup(
    backup_root: Path, destination: Path, store_id: str, batch_id: str
) -> Inventory:
    """Restore one backed-up batch into an empty root and verify its closure."""
    receipt_path = backup_root / "backups" / f"{_hex(batch_id)}.json"
    _require_safe_file(backup_root, receipt_path)
    receipt_hash = _hash_file(receipt_path)[0]
    receipt = _load_model(BackupReceipt, receipt_path)
    inventory = verify_batch(backup_root, store_id, batch_id)
    if (
        receipt.input_batch_ids != [batch_id]
        or receipt.manifest_sha256 != inventory.manifest.sha256
        or receipt.blob_hashes
        != sorted({item.blob.sha256 for item in inventory.entries})
    ):
        msg = "backup receipt does not match the sealed batch"
        raise ArchiveError(msg)
    if destination.exists() and any(destination.iterdir()):
        msg = f"restore destination must be empty: {destination}"
        raise ArchiveError(msg)
    _mkdir_safe(destination)
    _copy_closure(backup_root, destination, batch_id, inventory)
    _copy_immutable(receipt_path, destination / "backups" / f"{_hex(batch_id)}.json")
    restored = verify_batch(destination, store_id, batch_id)
    if _hash_file(receipt_path)[0] != receipt_hash:
        msg = "backup receipt changed during restore check"
        raise ArchiveRaceError(msg)
    return restored


def has_restore_check(backup_root: Path, store_id: str) -> bool:
    """Find a past successful restore check, not the backup's current health."""
    checks = backup_root / "restore-checks"
    if not checks.exists():
        return False
    if checks.is_symlink() or not checks.is_dir():
        msg = f"invalid restore-check directory: {checks}"
        raise ArchiveError(msg)
    found = False
    for path in checks.glob("*.json"):
        _require_safe_file(backup_root, path)
        check = _load_model(RestoreCheckReceipt, path)
        if check.store_id != store_id:
            continue
        if path.stem != _hex(check.batch_id):
            msg = f"restore-check receipt path does not match batch: {path}"
            raise ArchiveError(msg)
        receipt_path = backup_root / "backups" / path.name
        _require_hash(backup_root, receipt_path, check.backup_receipt_sha256)
        found = True
    return found


def record_restore_check(
    backup_root: Path, destination: Path, store_id: str, batch_id: str
) -> None:
    """Record a successful restore after restore_backup verifies its destination."""
    name = f"{_hex(batch_id)}.json"
    receipt_path = backup_root / "backups" / name
    restored_receipt_path = destination / "backups" / name
    _require_safe_file(backup_root, receipt_path)
    _require_safe_file(destination, restored_receipt_path)
    receipt_hash = _hash_file(receipt_path)[0]
    if _hash_file(restored_receipt_path)[0] != receipt_hash:
        msg = "backup receipt changed after restore check"
        raise ArchiveRaceError(msg)
    try:
        _record_restore_check(backup_root, store_id, batch_id, receipt_hash)
    except OSError as exc:
        msg = f"could not record restore check: {exc}"
        raise ArchiveError(msg) from exc


def _record_restore_check(
    backup_root: Path, store_id: str, batch_id: str, receipt_hash: str
) -> None:
    path = backup_root / "restore-checks" / f"{_hex(batch_id)}.json"
    if path.exists():
        _require_safe_file(backup_root, path)
        existing = _load_model(RestoreCheckReceipt, path)
        if (
            existing.store_id != store_id
            or existing.batch_id != batch_id
            or existing.backup_receipt_sha256 != receipt_hash
        ):
            msg = f"existing restore-check receipt does not match backup: {path}"
            raise ArchiveError(msg)
        return
    check = RestoreCheckReceipt(
        store_id=store_id,
        batch_id=batch_id,
        backup_receipt_sha256=receipt_hash,
        verified_at=datetime.now(UTC).isoformat(),
    )
    _install_bytes(path, _canonical_model(check))


def _copy_immutable(source: Path, target: Path) -> None:
    _mkdir_safe(target.parent)
    if source.is_symlink() or target.is_symlink():
        msg = "immutable backup files must not be symlinks"
        raise ArchiveError(msg)
    source_hash = _hash_file(source)
    if target.exists():
        if _hash_file(target) != source_hash:
            msg = f"backup target differs from immutable source: {target}"
            raise ArchiveError(msg)
        return
    temporary = target.with_name(f".{target.name}.{uuid.uuid4().hex}.tmp")
    try:
        with source.open("rb") as incoming, temporary.open("xb") as outgoing:
            shutil.copyfileobj(incoming, outgoing, _CHUNK)
            outgoing.flush()
            os.fsync(outgoing.fileno())
        if _hash_file(temporary) != source_hash:
            msg = f"backup copy differs from source: {source}"
            raise ArchiveError(msg)
        os.link(temporary, target)
        _fsync_dir(target.parent)
    finally:
        temporary.unlink(missing_ok=True)


class ArchiveReader:
    """Read verified current raw bytes from one sealed batch, never from latest."""

    def __init__(self, root: Path, store_id: str, batch_id: str) -> None:
        self._root = root
        inventory = verify_batch(root, store_id, batch_id)
        entries = {item.source_version_id: item for item in inventory.entries}
        self._current = {
            item.url: entries[item.source_version_id].blob for item in inventory.current
        }

    def local_state(self, url: str) -> LocalState:
        """Report whether the sealed batch includes a current source URL."""
        return LocalState.TRUSTED if url in self._current else LocalState.MISSING

    def read(self, url: str) -> bytes:
        """Return exact archived raw bytes after checking their content hash."""
        blob = self._current.get(url)
        if blob is None:
            msg = f"sealed batch has no current source for {url}"
            raise ArchiveError(msg)
        path = self._root / blob.path
        _require_hash(self._root, path, blob.sha256, blob.bytes)
        return path.read_bytes()

"""Verify sealed source closure and expose exact raw bytes with shared metadata."""

import re
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING, cast

from pydantic import ValidationError

from sve_carddb.core.json import digest
from sve_carddb.core.provenance import ArchivePin, RawKind, Source
from sve_carddb.ingest.archive.source_archive import (
    ArchiveError,
    Current,
    Descriptor,
    Receipt,
    verify_batch,
)
from sve_carddb.ingest.archive.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping


class FrozenSources:
    @classmethod
    def configured(cls, stores: Mapping[str, Path], batch_id: str) -> FrozenSources:
        """Resolve an exact batch in explicitly configured stores, then verify ownership."""
        root, name = configured_batch(stores, batch_id)
        return cls(root, name, batch_id)

    def __init__(
        self, root: Path, store_id: str, batch_id: str, *, verify_raw: bool = True
    ) -> None:
        self.inventory = verify_batch(root, store_id, batch_id, verify_raw=verify_raw)
        self.entries = {
            entry.source_version_id: entry for entry in self.inventory.entries
        }
        self.root = root
        self.store_id = store_id
        self.batch_id = batch_id
        self._metadata: dict[str, Source] = {}
        self._descriptors: dict[str, Descriptor] = {}
        self._verified: set[str] = set()
        self._projections: dict[tuple[str, str], object] = {}

    @property
    def version_ids(self) -> frozenset[str]:
        """Expose batch membership independently of private inventory details."""
        return frozenset(self.entries)

    @property
    def current_versions(self) -> tuple[Current, ...]:
        """Expose the batch's URL selections in their canonical order."""
        return tuple(self.inventory.current)

    def require_scope(self, root: Path, store_id: str, batch_id: str) -> None:
        """A shared reader must still belong to the caller's exact input pin."""
        if (self.root.resolve(), self.store_id, self.batch_id) != (
            root.resolve(),
            store_id,
            batch_id,
        ):
            raise ValueError("Frozen source batch differs from configured input")

    def read(
        self, version: str, *, parser_version: str
    ) -> tuple[Source, bytes, Descriptor]:
        """Verify each newly consumed raw blob once, without retaining its bytes."""
        source, descriptor = self.metadata(version, parser_version=parser_version)
        entry = self.entries[version]
        raw = resolve_within(self.root, PurePosixPath(entry.blob.path)).read_bytes()
        if entry.blob.sha256 not in self._verified:
            if digest(raw) != entry.blob.sha256:
                raise ArchiveError("Frozen source content hash mismatch")
            self._verified.add(entry.blob.sha256)
        return source, raw, descriptor

    def metadata(
        self, version: str, *, parser_version: str
    ) -> tuple[Source, Descriptor]:
        """Resolve verified provenance without opening a frozen bitmap."""
        if version in self._metadata:
            source = self._metadata[version]
            if source.parser_version == parser_version:
                return source, self.descriptor(version)
            return source.model_copy(
                update={"parser_version": parser_version}
            ), self.descriptor(version)
        entry = self.entries.get(version)
        if entry is None:
            raise ArchiveError("Source version is absent from pinned batch")
        try:
            descriptor = self.descriptor(version)
            receipt = Receipt.model_validate_json(
                self._bytes(
                    f"receipts/{descriptor.first_receipt_id.removeprefix('sha256:')}.json",
                    descriptor.first_receipt_id,
                )
            )
        except ValidationError:
            raise ArchiveError("Invalid frozen source metadata") from None
        if descriptor.id != version or descriptor.raw_sha256 != entry.blob.sha256:
            raise ArchiveError("Frozen source identity mismatch")
        resource = receipt.resource
        instant = datetime.fromisoformat(resource.last_changed_at)
        if instant.tzinfo is None:
            raise ArchiveError("Source receipt timestamp has no timezone")
        source = Source(
            id=descriptor.id,
            kind=_kind(resource.content_type),
            url=descriptor.url,
            raw_locator=f"{self.store_id}:{entry.blob.path}",
            sha256=descriptor.raw_sha256,
            fetched_at=instant.astimezone(UTC).isoformat().replace("+00:00", "Z"),
            etag=resource.etag,
            last_modified=resource.last_modified,
            parser_version=parser_version,
            archive=ArchivePin(
                store_id=self.store_id,
                batch_id=self.batch_id,
                descriptor_sha256=entry.descriptor_sha256,
                first_receipt_id=descriptor.first_receipt_id,
            ),
        )
        self._metadata[version] = source
        return source, descriptor

    def descriptor(self, version: str) -> Descriptor:
        """Read each hash-verified descriptor once for this pinned inventory."""
        if version in self._descriptors:
            return self._descriptors[version]
        entry = self.entries.get(version)
        if entry is None:
            raise ArchiveError("Source version is absent from pinned batch")
        descriptor = Descriptor.model_validate_json(
            self._bytes(
                "descriptors/"
                + entry.descriptor_sha256.removeprefix("sha256:")
                + ".json",
                entry.descriptor_sha256,
            )
        )
        if descriptor.id != version or descriptor.raw_sha256 != entry.blob.sha256:
            raise ArchiveError("Frozen source identity mismatch")
        self._descriptors[version] = descriptor
        return descriptor

    def projection[T](
        self,
        version: str,
        *,
        parser_version: str,
        extract: Callable[[Source, bytes, Descriptor], T],
    ) -> T:
        """Keep only a parser's projections within this exact root/store/batch."""
        key = version, parser_version
        if key not in self._projections:
            source, raw, descriptor = self.read(version, parser_version=parser_version)
            self._projections[key] = extract(source, raw, descriptor)
        return cast("T", self._projections[key])

    def _bytes(self, relative: str, checksum: str) -> bytes:
        data = resolve_within(self.root, PurePosixPath(relative)).read_bytes()
        if digest(data) != checksum:
            raise ArchiveError("Frozen source content hash mismatch")
        return data


class FrozenBatches:
    """Own verified metadata and compact source projections for one command."""

    def __init__(self) -> None:
        self.batches: dict[tuple[Path, str, str], FrozenSources] = {}

    def batch(self, root: Path, store_id: str, batch_id: str) -> FrozenSources:
        """Resolve the full store/root/batch key without borrowing another pin."""
        key = root.resolve(), store_id, batch_id
        if key not in self.batches:
            self.batches[key] = FrozenSources(root, store_id, batch_id)
        return self.batches[key]

    def configured(self, stores: Mapping[str, Path], batch_id: str) -> FrozenSources:
        """Require unambiguous store ownership even for an already opened batch."""
        root, store_id = configured_batch(stores, batch_id)
        return self.batch(root, store_id, batch_id)


def configured_batch(stores: Mapping[str, Path], batch_id: str) -> tuple[Path, str]:
    """Resolve an exact batch through the caller's explicit archive stores."""
    if re.fullmatch(r"sha256:[0-9a-f]{64}", batch_id) is None:
        raise ValueError("Invalid configured source batch ID")
    matches = [
        (root, name)
        for name, root in stores.items()
        if (root / "batches" / batch_id[7:]).exists()
    ]
    if len(matches) != 1:
        raise ValueError("Source batch requires exactly one configured archive store")
    return matches[0]


def _kind(content_type: str) -> RawKind:
    media = content_type.split(";", 1)[0].strip().lower()
    if media.startswith("image/"):
        return "image"
    kinds: dict[str, RawKind] = {
        "application/pdf": "official_pdf",
        "application/json": "official_api",
        "text/html": "official_page",
        "application/xhtml+xml": "official_page",
    }
    if media not in kinds:
        raise ValueError("Unsupported product evidence media type")
    return kinds[media]

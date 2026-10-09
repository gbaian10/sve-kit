"""Verify sealed source closure and expose exact raw bytes with shared metadata."""

import re
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from pydantic import ValidationError

from sve_carddb.core.json import digest
from sve_carddb.core.provenance import ArchivePin, RawKind, Source
from sve_carddb.ingest.archive.source_archive import (
    ArchiveError,
    Descriptor,
    Receipt,
    verify_batch,
)
from sve_carddb.ingest.archive.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Mapping


class FrozenSources:
    @classmethod
    def configured(cls, stores: Mapping[str, Path], batch_id: str) -> FrozenSources:
        """Resolve an exact batch in explicitly configured stores, then verify ownership."""
        root, name = configured_batch(stores, batch_id)
        return cls(root, name, batch_id)

    def __init__(self, root: Path, store_id: str, batch_id: str) -> None:
        self.inventory = verify_batch(root, store_id, batch_id)
        self.entries = {
            entry.source_version_id: entry for entry in self.inventory.entries
        }
        self.root = root
        self.store_id = store_id
        self.batch_id = batch_id

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
        """Recheck pinned metadata/raw hashes; later receipts never replace first metadata."""
        entry = self.entries.get(version)
        if entry is None:
            raise ArchiveError("Source version is absent from pinned batch")
        try:
            descriptor = Descriptor.model_validate_json(
                self._bytes(
                    f"descriptors/{entry.descriptor_sha256.removeprefix('sha256:')}.json",
                    entry.descriptor_sha256,
                )
            )
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
        raw = self._bytes(entry.blob.path, entry.blob.sha256)
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
        return source, raw, descriptor

    def descriptor(self, version: str) -> Descriptor:
        """Read a hash-verified descriptor for an explicitly pinned inventory member."""
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
        return descriptor

    def _bytes(self, relative: str, checksum: str) -> bytes:
        data = resolve_within(self.root, PurePosixPath(relative)).read_bytes()
        if digest(data) != checksum:
            raise ArchiveError("Frozen source content hash mismatch")
        return data


class FrozenBatches:
    """Own shared source readers for one command; raw reads always recheck hashes."""

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

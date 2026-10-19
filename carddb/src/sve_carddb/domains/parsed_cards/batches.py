"""Read portable batch selections and restore runtime source provenance."""

from typing import TYPE_CHECKING

from sve_carddb.core.provenance import ArchivePin, Source
from sve_carddb.domains.parsed_cards.models import (
    BATCH_PART_SIZE,
    BatchFile,
    BatchSelection,
)

if TYPE_CHECKING:
    from sve_carddb.domains.parsed_cards.models import ParsedObservation
    from sve_carddb.domains.parsed_cards.storage import Catalog


def raw_path(value: ParsedObservation) -> str:
    """Recover the archive's content-addressed locator, never a local file path."""
    checksum = value.evidence.descriptor.raw_sha256[7:]
    return f"raw/sha256/{checksum[:2]}/{checksum}.raw"


def selection(catalog: Catalog, batch_id: str) -> BatchSelection:
    """Return the reviewed source selection, not a reconstruction of private inventory."""
    for batch in catalog.batches:
        if batch.id == batch_id:
            return batch
    raise ValueError("Parsed data does not include the requested batch")


def source(
    catalog: Catalog,
    value: ParsedObservation,
    *,
    batch_id: str,
    store_id: str,
    parser_version: str,
) -> Source:
    """Restore runtime provenance using an explicitly supplied logical store name."""
    if value.evidence.descriptor.id not in selection(catalog, batch_id).versions:
        raise ValueError("Parsed source version is absent from the requested batch")
    evidence = value.evidence
    descriptor = evidence.descriptor
    return Source(
        id=descriptor.id,
        kind="official_page",
        url=descriptor.url,
        raw_locator=f"{store_id}:{raw_path(value)}",
        sha256=descriptor.raw_sha256,
        fetched_at=evidence.fetched_at,
        etag=evidence.etag,
        last_modified=evidence.last_modified,
        parser_version=parser_version,
        archive=ArchivePin(
            store_id=store_id,
            batch_id=batch_id,
            descriptor_sha256=evidence.descriptor_sha256,
            first_receipt_id=descriptor.first_receipt_id,
        ),
    )


def parts(batch: BatchSelection) -> tuple[BatchFile, ...]:
    """Bound each new immutable batch member without touching older batches."""
    versions = sorted(batch.versions.items())
    return tuple(
        BatchFile(
            id=batch.id,
            region=batch.region,
            part=offset // BATCH_PART_SIZE,
            versions=dict(versions[offset : offset + BATCH_PART_SIZE]),
        )
        for offset in range(0, len(versions), BATCH_PART_SIZE)
    )


def join(members: tuple[BatchFile, ...]) -> BatchSelection:
    """Reject duplicate, incomplete or reordered selection parts before use."""
    if not members:
        raise ValueError("Parsed batch requires selection parts")
    versions = {}
    for member in members:
        for version, current in member.versions.items():
            if version in versions:
                raise ValueError("Duplicate parsed batch source version")
            versions[version] = current
    result = BatchSelection(
        id=members[0].id,
        region=members[0].region,
        versions=dict(sorted(versions.items())),
    )
    if parts(result) != members:
        raise ValueError("Parsed batch parts are not canonical")
    return result

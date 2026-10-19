"""Recover original inventory identity without an archive path or raw HTML."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import ArchivePin, Source
from sve_carddb.ingest.archive.source_archive import (
    Blob,
    Current,
    Entry,
    Inventory,
    Scope,
)

if TYPE_CHECKING:
    from sve_carddb.domains.parsed_cards.models import ParsedObservation
    from sve_carddb.domains.parsed_cards.storage import Catalog


def raw_path(value: ParsedObservation) -> str:
    """Recover the archive's content-addressed locator, never a local file path."""
    checksum = value.evidence.descriptor.raw_sha256[7:]
    return f"raw/sha256/{checksum[:2]}/{checksum}.raw"


def inventory(catalog: Catalog, batch_id: str, store_id: str) -> Inventory:
    """Require the reconstructed original inventory to match its unchanged hash."""
    pin = next((p for p in catalog.index.batches if p.id == batch_id), None)
    if pin is None:
        raise ValueError("Parsed data does not include the requested batch")
    entries: list[Entry] = []
    current: list[Current] = []
    for version, value in sorted(catalog.observations().items()):
        memberships = [m for m in value.memberships if m.batch_id == batch_id]
        if not memberships:
            continue
        membership = memberships[0]
        descriptor = value.evidence.descriptor
        entries.append(
            Entry(
                source_version_id=version,
                receipt_id=membership.receipt_id,
                descriptor_sha256=value.evidence.descriptor_sha256,
                blob=Blob(
                    store_id=store_id,
                    path=raw_path(value),
                    sha256=descriptor.raw_sha256,
                    bytes=descriptor.raw_bytes,
                ),
            )
        )
        if membership.current:
            current.append(Current(url=descriptor.url, source_version_id=version))
    result = Inventory(
        created_at=pin.created_at,
        manifest=pin.manifest,
        scope=[Scope(provider=pin.region, kind="card")],
        current=sorted(current, key=lambda c: c.url),
        entries=entries,
        missing=[],
        history_gaps=[],
    )
    if digest(canonical(result.model_dump(mode="json"))) != batch_id:
        raise ValueError("Parsed inventory differs from the original batch identity")
    return result


def source(
    value: ParsedObservation, *, batch_id: str, store_id: str, parser_version: str
) -> Source:
    """Restore runtime provenance using an explicitly supplied logical store name."""
    if not any(m.batch_id == batch_id for m in value.memberships):
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

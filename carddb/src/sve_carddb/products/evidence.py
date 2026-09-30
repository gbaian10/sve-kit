"""Resolve authored evidence through sealed archive closure, without live access."""

import hashlib
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import PurePosixPath
from typing import TYPE_CHECKING

from pydantic import ValidationError

from sve_carddb.registry.preview.evidence import Source
from sve_carddb.source_archive import ArchiveError, Descriptor, Receipt, verify_batch
from sve_carddb.store import resolve_within

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.products.models import Evidence
    from sve_carddb.source_archive import Inventory


@dataclass(frozen=True)
class CheckedSource:
    kind: str
    source: Source


def resolve_evidence(
    references: tuple[Evidence, ...], stores: Mapping[str, Path]
) -> Mapping[Evidence, CheckedSource]:
    """Check each full batch before reading its referenced descriptor and first receipt."""
    batches: dict[tuple[str, str], Inventory] = {}
    result: dict[Evidence, CheckedSource] = {}
    for reference in references:
        if reference in result:
            continue
        root = stores.get(reference.store_id)
        if root is None:
            raise ValueError(
                "Product evidence requires an explicitly configured archive store"
            )
        key = reference.store_id, reference.batch_id
        if key not in batches:
            batches[key] = verify_batch(root, *key)
        entry = next(
            (
                item
                for item in batches[key].entries
                if item.source_version_id == reference.source_version_id
            ),
            None,
        )
        if entry is None:
            raise ValueError(
                "Product evidence source version is absent from pinned batch"
            )
        try:
            descriptor = Descriptor.model_validate_json(
                _bytes(
                    root,
                    "descriptors/"
                    + entry.descriptor_sha256.removeprefix("sha256:")
                    + ".json",
                    entry.descriptor_sha256,
                )
            )
            receipt = Receipt.model_validate_json(
                _bytes(
                    root,
                    "receipts/"
                    + descriptor.first_receipt_id.removeprefix("sha256:")
                    + ".json",
                    descriptor.first_receipt_id,
                )
            )
        except ValidationError:
            raise ArchiveError("Invalid product archive evidence metadata") from None
        _bytes(root, entry.blob.path, entry.blob.sha256)
        resource = receipt.resource
        result[reference] = CheckedSource(
            _kind(resource.content_type),
            Source(
                id=descriptor.id,
                url=descriptor.url,
                raw_locator=reference.store_id + ":" + entry.blob.path,
                sha256=descriptor.raw_sha256,
                fetched_at=_instant(resource.last_changed_at),
                etag=resource.etag,
                last_modified=resource.last_modified,
                parser_version="product-authored-v1",
            ),
        )
    return result


def _bytes(root: Path, relative: str, checksum: str) -> bytes:
    data = resolve_within(root, PurePosixPath(relative)).read_bytes()
    if "sha256:" + hashlib.sha256(data).hexdigest() != checksum:
        raise ArchiveError("Product evidence content hash mismatch")
    return data


def _kind(content_type: str) -> str:
    media = content_type.split(";", 1)[0].strip().lower()
    if media.startswith("image/"):
        return "image"
    if media == "application/pdf":
        return "official_pdf"
    if media == "application/json":
        return "official_api"
    if media in {"text/html", "application/xhtml+xml"}:
        return "official_page"
    raise ValueError("Unsupported product evidence media type")


def _instant(value: str) -> str:
    instant = datetime.fromisoformat(value)
    if instant.tzinfo is None:
        raise ArchiveError("Product source receipt timestamp has no timezone")
    return instant.astimezone(UTC).isoformat().replace("+00:00", "Z")

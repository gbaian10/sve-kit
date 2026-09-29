"""Read-only JP evidence from a verified immutable archive batch."""

import hashlib
from datetime import UTC, datetime
from pathlib import PurePosixPath
from typing import TYPE_CHECKING

from pydantic import ValidationError

from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.registry.preview.evidence import CardEvidence, FaceEvidence, Source
from sve_carddb.source_archive import ArchiveError, Descriptor, Receipt, verify_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.store import resolve_within

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.records import Region


def _bytes(root: Path, relative: str, expected: str) -> bytes:
    content = resolve_within(root, PurePosixPath(relative)).read_bytes()
    if "sha256:" + hashlib.sha256(content).hexdigest() != expected:
        raise ArchiveError("Frozen evidence content hash mismatch")
    return content


class FrozenJP:
    """Pin current entries in one sealed batch; all later reads recheck hashes."""

    def __init__(
        self, root: Path, store_id: str, batch_id: str, *, parser_version: str
    ) -> None:
        inventory = verify_batch(root, store_id, batch_id)
        entries = {entry.source_version_id: entry for entry in inventory.entries}
        self._entries = {
            current.url: entries[current.source_version_id]
            for current in inventory.current
        }
        self._root = root
        self._store_id = store_id
        self._parser_version = parser_version
        self.batch_id = batch_id

    @staticmethod
    def coverage(_coverage_hash: str) -> bool:
        """An HTML inventory hash is not the historic complete JSONL input hash."""
        return False

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Extract one exact archived JP source and retain its first receipt."""
        if region != "jp":
            return None
        url = card_url(card_no)
        entry = self._entries.get(url)
        if entry is None:
            return None
        try:
            descriptor = Descriptor.model_validate_json(
                _bytes(
                    self._root,
                    f"descriptors/{entry.descriptor_sha256.removeprefix('sha256:')}.json",
                    entry.descriptor_sha256,
                )
            )
            receipt = Receipt.model_validate_json(
                _bytes(
                    self._root,
                    f"receipts/{descriptor.first_receipt_id.removeprefix('sha256:')}.json",
                    descriptor.first_receipt_id,
                )
            )
        except ValidationError:
            raise ArchiveError("Invalid frozen evidence metadata") from None
        if (descriptor.provider, descriptor.kind, descriptor.url) != (
            "jp",
            "card",
            url,
        ):
            raise ArchiveError("Frozen evidence source identity mismatch")
        raw = _bytes(self._root, entry.blob.path, entry.blob.sha256)
        record = extract_card(raw, number=card_no)
        resource = receipt.resource
        source = Source(
            id=descriptor.id,
            url=descriptor.url,
            raw_locator=f"{self._store_id}:{entry.blob.path}",
            sha256=descriptor.raw_sha256,
            fetched_at=_instant(resource.first_fetched_at),
            etag=resource.etag,
            last_modified=resource.last_modified,
            parser_version=self._parser_version,
        )
        return CardEvidence.from_card(
            source,
            "jp",
            legacy_projection(record),
            tuple(FaceEvidence(face.rarity, face.illustrator) for face in record.faces),
        )


def _instant(value: str) -> str:
    date = datetime.fromisoformat(value)
    if date.tzinfo is None:
        raise ArchiveError("Source receipt timestamp has no timezone")
    return date.astimezone(UTC).isoformat().replace("+00:00", "Z")

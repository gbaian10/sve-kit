"""Production EN evidence recomputed exclusively from sealed raw sources."""

from typing import TYPE_CHECKING

from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.source_archive import ArchiveError
from sve_carddb.parse.pages.extract_en import extract_card
from sve_carddb.parse.pages.official_en import card_url
from sve_carddb.registry.parser_adapters.official_en import legacy_projection
from sve_carddb.registry.preview.evidence import CardEvidence, FaceEvidence

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.preview.archive import FrozenJP
    from sve_carddb.registry.records import Region


class FrozenEN:
    def __init__(
        self, root: Path, store_id: str, batch_id: str, *, parser_version: str
    ) -> None:
        self._sources = FrozenSources(root, store_id, batch_id)
        self._current = {
            item.url: item.source_version_id for item in self._sources.inventory.current
        }
        self._parser_version = parser_version
        self.batch_id = batch_id

    @staticmethod
    def coverage(_coverage_hash: str) -> bool:
        """Neither card HTML nor a legacy renderer comparison proves review coverage."""
        return False

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Check exact region/URL and compute both observation hashes from raw bytes."""
        if region != "en":
            return None
        url = card_url(card_no)
        version = self._current.get(url)
        if version is None:
            return None
        source, raw, descriptor = self._sources.read(
            version, parser_version=self._parser_version
        )
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            "en",
            "card",
            url,
            "official_page",
        ):
            raise ArchiveError("Frozen EN evidence source identity mismatch")
        record = extract_card(raw, number=card_no)
        return CardEvidence.from_card(
            source,
            "en",
            legacy_projection(record),
            tuple(
                FaceEvidence(face.info["Rarity"], face.illustrator)
                for face in record.faces
            ),
        )


class FrozenRegions:
    def __init__(self, *, jp: FrozenJP, en: FrozenEN) -> None:
        self.jp = jp
        self.en = en

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Dispatch by the explicit region; suffixes never infer identity."""
        return (self.jp if region == "jp" else self.en).card(region, card_no)

    @staticmethod
    def coverage(_coverage_hash: str) -> bool:
        """Composition of raw batches still does not prove historical review scope."""
        return False

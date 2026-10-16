"""Production EN evidence recomputed exclusively from sealed raw sources."""

from typing import TYPE_CHECKING

from sve_carddb.domains import physical_sources
from sve_carddb.domains.registry.preview.evidence import CardEvidence, FaceEvidence
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.source_archive import ArchiveError
from sve_carddb.parse.pages import extract_en
from sve_carddb.parse.pages.official_en import card_url

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.preview.archive import FrozenJP
    from sve_carddb.domains.registry.preview.review_inputs import RegistryReviewInputs
    from sve_carddb.domains.registry.records import MappingReviewData


class FrozenEN:
    review_inputs: RegistryReviewInputs | None = None

    def __init__(
        self,
        root: Path,
        store_id: str,
        batch_id: str,
        *,
        parser_version: str,
        sources: FrozenSources | None = None,
    ) -> None:
        self._sources = sources or FrozenSources(root, store_id, batch_id)
        self._sources.require_scope(root, store_id, batch_id)
        self._current = {
            item.url: item.source_version_id for item in self._sources.inventory.current
        }
        self._parser_version = parser_version
        self.batch_id = batch_id

    @staticmethod
    def coverage(_review: MappingReviewData) -> bool:
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
        source, descriptor = self._sources.metadata(
            version, parser_version=self._parser_version
        )
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            "en",
            "card",
            url,
            "official_page",
        ):
            raise ArchiveError("Frozen EN evidence source identity mismatch")
        page = physical_sources.page(
            self._sources, version, region="en", parser_version=self._parser_version
        )
        record = page.record.get()
        assert isinstance(record, extract_en.CardRecord)
        return CardEvidence(
            source,
            page.observation.get(),
            tuple(
                FaceEvidence(face.info["Rarity"], face.illustrator)
                for face in record.faces
            ),
        )


class FrozenRegions:
    def __init__(
        self,
        *,
        jp: FrozenJP,
        en: FrozenEN,
        review_inputs: RegistryReviewInputs | None = None,
    ) -> None:
        self.jp = jp
        self.en = en
        self.review_inputs = review_inputs

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Dispatch by the explicit region; suffixes never infer identity."""
        return (self.jp if region == "jp" else self.en).card(region, card_no)

    def coverage(self, review: MappingReviewData) -> bool:
        """Historical coverage comes only from independently verified registry inputs."""
        return self.review_inputs is not None and self.review_inputs.coverage(review)

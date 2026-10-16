"""Read-only JP evidence from a verified immutable archive batch."""

from typing import TYPE_CHECKING

from sve_carddb.domains import physical_sources
from sve_carddb.domains.registry.preview.evidence import CardEvidence, FaceEvidence
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.source_archive import ArchiveError
from sve_carddb.parse.pages import extract_jp
from sve_carddb.parse.pages.official_jp import card_url

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.preview.review_inputs import RegistryReviewInputs
    from sve_carddb.domains.registry.records import MappingReviewData


class FrozenJP:
    """Pin current entries in one sealed batch and share verified page projections."""

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
        """An HTML inventory hash is not the historic complete JSONL input hash."""
        return False

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Extract one exact archived JP source and retain its first receipt."""
        if region != "jp":
            return None
        url = card_url(card_no)
        version = self._current.get(url)
        if version is None:
            return None
        source, descriptor = self._sources.metadata(
            version, parser_version=self._parser_version
        )
        if (descriptor.provider, descriptor.kind, descriptor.url, source.kind) != (
            "jp",
            "card",
            url,
            "official_page",
        ):
            raise ArchiveError("Frozen evidence source identity mismatch")
        page = physical_sources.page(
            self._sources, version, region="jp", parser_version=self._parser_version
        )
        record = page.record.get()
        assert isinstance(record, extract_jp.CardRecord)
        return CardEvidence(
            source,
            page.observation.get(),
            tuple(FaceEvidence(face.rarity, face.illustrator) for face in record.faces),
        )

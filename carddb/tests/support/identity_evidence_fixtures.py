"""Synthetic pinned evidence providers belong to tests, not runtime catalog APIs."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.preview.evidence import CardEvidence
    from sve_carddb.domains.registry.preview.review_inputs import RegistryReviewInputs
    from sve_carddb.domains.registry.records import MappingReviewData


@dataclass(frozen=True)
class MemoryEvidence:
    """Inject already extracted sources, including synthetic EN sources in tests."""

    cards: Mapping[tuple[Region, str], CardEvidence]
    coverage_hashes: frozenset[str] = frozenset()
    review_inputs: RegistryReviewInputs | None = None

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Resolve a pinned extracted source by exact region and card number."""
        return self.cards.get((region, card_no))

    def coverage(self, review: MappingReviewData) -> bool:
        """Check a pinned complete review input, independently from individual cards."""
        return review.coverage_hash in self.coverage_hashes

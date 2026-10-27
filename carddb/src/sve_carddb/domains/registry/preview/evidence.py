"""Frozen evidence supplied by an archive adapter or a synthetic test provider."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.preview.review_inputs import RegistryReviewInputs
    from sve_carddb.domains.registry.records import MappingReviewData, Observation


@dataclass(frozen=True)
class FaceEvidence:
    rarity_raw: str
    credit_raw: str | None


@dataclass(frozen=True)
class CardEvidence:
    source: Source
    observation: Observation
    faces: tuple[FaceEvidence, ...]


class EvidenceProvider(Protocol):
    @property
    def review_inputs(self) -> RegistryReviewInputs | None:
        """Expose only verified historical scope for registry-wide containment checks."""
        ...

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Resolve exact identity within pinned inputs; never fall back to latest."""
        ...

    def coverage(self, review: MappingReviewData) -> bool:
        """Whether the full historical input matches the review region, scope, date and hash."""
        ...

"""Frozen evidence supplied by an archive adapter or a synthetic test provider."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol

from sve_carddb.core.json import canonical
from sve_carddb.domains.registry.records import Observation
from sve_carddb.domains.registry.review import observation

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.inputs import Card
    from sve_carddb.domains.registry.preview.review_inputs import RegistryReviewInputs
    from sve_carddb.domains.registry.records import MappingReviewData


@dataclass(frozen=True)
class FaceEvidence:
    rarity_raw: str
    credit_raw: str | None


@dataclass(frozen=True)
class CardEvidence:
    source: Source
    observation: Observation
    faces: tuple[FaceEvidence, ...]

    @classmethod
    def from_card(
        cls, source: Source, region: Region, card: Card, faces: tuple[FaceEvidence, ...]
    ) -> CardEvidence:
        """Compute the registry observation recipe from extracted data, not registry hashes."""
        if len(card.faces) != len(faces):
            raise ValueError("Source face metadata count mismatch")
        return cls(
            source,
            Observation.model_validate_json(canonical(observation(card, region))),
            faces,
        )


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

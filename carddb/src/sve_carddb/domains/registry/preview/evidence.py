"""Frozen evidence supplied by an archive adapter or a synthetic test provider."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol

from sve_carddb.domains.registry.inputs import Card, canonical
from sve_carddb.domains.registry.records import Observation
from sve_carddb.domains.registry.review import observation

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region


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
        """Compute the old observation recipe from extracted data, not registry hashes."""
        if len(card.faces) != len(faces):
            raise ValueError("Source face metadata count mismatch")
        return cls(
            source,
            Observation.model_validate_json(canonical(observation(card, region))),
            faces,
        )


class EvidenceProvider(Protocol):
    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Resolve exact identity within pinned inputs; never fall back to latest."""
        ...

    def coverage(self, coverage_hash: str) -> bool:
        """Whether the complete historical review input with this hash is pinned."""
        ...

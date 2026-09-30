"""Frozen evidence supplied by an archive adapter or a synthetic test provider."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Protocol

from sve_carddb.registry.inputs import Card, canonical
from sve_carddb.registry.records import Observation, Region
from sve_carddb.registry.review import observation

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build_inputs import Source


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


@dataclass(frozen=True)
class MemoryEvidence:
    """Inject already extracted sources, including synthetic EN sources in tests."""

    cards: Mapping[tuple[Region, str], CardEvidence]
    coverage_hashes: frozenset[str] = frozenset()

    def card(self, region: Region, card_no: str) -> CardEvidence | None:
        """Resolve a pinned extracted source by exact region and card number."""
        return self.cards.get((region, card_no))

    def coverage(self, coverage_hash: str) -> bool:
        """Check a pinned complete review input, independently from individual cards."""
        return coverage_hash in self.coverage_hashes

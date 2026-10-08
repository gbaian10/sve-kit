"""Verified same-card counterpart candidates use a fixed digital game order."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.core.provenance import Source
    from sve_carddb.domains.catalog.adoption_models import SourceRef


GAME_PRIORITY = ("sv1", "svwb")


@dataclass(frozen=True)
class NameCandidate:
    text: str
    origin: str
    authority: str
    decision_id: str | None
    source: Source | None
    refs: tuple[SourceRef, ...] = ()


def first_counterpart(candidates: tuple[NameCandidate, ...]) -> NameCandidate | None:
    """Current policy names and same-card links use sv1 first."""
    for game in GAME_PRIORITY:
        matches = tuple(c for c in candidates if c.origin == "official_" + game)
        if len({candidate.text for candidate in matches}) > 1:
            raise ValueError("Ambiguous adopted digital names")
        if matches:
            return min(matches, key=lambda c: (c.text, c.decision_id or ""))
    return None

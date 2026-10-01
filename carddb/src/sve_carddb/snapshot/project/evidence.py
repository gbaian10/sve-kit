"""Explicit adapter inputs for domain decisions owned by other build capabilities."""

from dataclasses import dataclass, field
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.snapshot.project.source import Record


@dataclass(frozen=True)
class DisplayBinding:
    """Reuse an exact source use on an explicitly verified display owner."""

    source_use_id: str
    destination: tuple[str, ...]
    target_lang: str
    basis: str
    translation_id: str | None = None


@dataclass(frozen=True)
class Decisions:
    """Carry verified build projections; absence never proves cross-region equivalence."""

    wording: Mapping[str, tuple[Record, ...]] = field(default_factory=dict)
    observation_states: Mapping[tuple[str, str, str], str] = field(default_factory=dict)
    display_bindings: tuple[DisplayBinding, ...] = ()
    related_regions: Mapping[str, tuple[str, ...]] = field(default_factory=dict)
    active_scopes: Mapping[str, tuple[str, ...]] = field(default_factory=dict)
    aligned_regions: frozenset[tuple[str, str]] = frozenset()
    general_printings: frozenset[str] = frozenset()

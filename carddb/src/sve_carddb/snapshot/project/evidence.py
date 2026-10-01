"""Explicit adapter inputs for domain decisions owned by other build capabilities."""

from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import object_value, parse

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from pydantic import BaseModel

    from sve_carddb.routes.defaults import GeneralEvidence
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
    observed_texts: Mapping[tuple[str, str], tuple[Record, ...]] = field(
        default_factory=dict
    )
    display_bindings: tuple[DisplayBinding, ...] = ()
    related_regions: Mapping[str, tuple[str, ...]] = field(default_factory=dict)
    active_scopes: Mapping[str, tuple[str, ...]] = field(default_factory=dict)
    aligned_regions: frozenset[tuple[str, str]] = frozenset()
    general_evidence: Mapping[str, GeneralEvidence] = field(default_factory=dict)

    def with_text_views(
        self,
        wording: Mapping[str, Sequence[BaseModel]],
        observed_texts: Mapping[tuple[str, str], Sequence[BaseModel]],
    ) -> Decisions:
        """Preserve native #145 public payloads, including unavailable sources."""
        return replace(
            self,
            wording={
                face: tuple(
                    object_value(parse(row.model_dump_json().encode())) for row in rows
                )
                for face, rows in wording.items()
            },
            observed_texts={
                key: tuple(
                    object_value(parse(row.model_dump_json().encode())) for row in rows
                )
                for key, rows in observed_texts.items()
            },
        )

"""Detached effective entities and routing history, without publication."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.registry.records import DATA_MODELS, EnglishPrintingData
from sve_carddb.domains.registry.storage import Entry
from sve_carddb.domains.registry.transitions.models import Before

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.core.models import RecordData
    from sve_carddb.domains.registry.transitions.models import Repair
    from sve_carddb.domains.registry.transitions.routing import RouteProjection


@dataclass(frozen=True)
class Entity:
    content: bytes
    reference: Before
    permanent_content: bytes | None

    def entry(self) -> Entry | None:
        """Return a detached record; inactive review/relation remains in history."""
        return (
            None if self.content == b"null" else Entry.model_validate_json(self.content)
        )

    def data(self) -> RecordData | None:
        """Narrow complete wire data without exposing mutable canonical state."""
        entry = self.entry()
        if entry is None:
            return None
        model = DATA_MODELS[entry.kind]
        if entry.kind == "printing" and entry.data["region"] == "en":
            model = EnglishPrintingData
        return model.model_validate_json(canonical(entry.data))


def entity(entry: Entry | None, key: str, transition: str | None) -> Entity:
    """Pin exact semantic content and the transition that produced it."""
    raw = None if entry is None else entry.model_dump(mode="json", round_trip=True)
    return Entity(
        canonical(raw),
        Before(
            transition_key=transition,
            record_key=key,
            record_hash=digest(
                canonical(None if entry is None else entry.model_dump(mode="json"))
            ),
        ),
        None if entry is None else canonical(raw),
    )


@dataclass(frozen=True)
class EffectiveRegistry:
    records: Mapping[str, Entity]
    repairs: tuple[Repair, ...]
    routes: RouteProjection

    def entries(self) -> tuple[Entry, ...]:
        """Keep tombstones and historical art in the build-side input closure."""
        return tuple(
            entry
            for item in self.records.values()
            if (entry := item.entry()) is not None
        )


def frozen_records(records: dict[str, Entity]) -> Mapping[str, Entity]:
    """Prevent the caller or a later transaction from mutating a completed result."""
    return MappingProxyType(dict(sorted(records.items())))

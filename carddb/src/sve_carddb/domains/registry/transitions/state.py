"""Detached effective entities and browsing/old-deck repair hints, without publication."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal

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
class RepairHint:
    status: Literal["unchanged", "repaired", "choice_required"]
    printing_id: str
    card_id: str
    choices: tuple[str, ...]


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

    def browse(self, kind: str) -> tuple[Entry, ...]:
        """Exclude retired identities, tombstones and unused art from browsing.

        Wording/errata readiness is independent and cannot remove known identities.
        This view carries no current/DSL/default/artist or publication capability.
        """
        entries = self.entries()
        cards = {
            str(e.data["id"])
            for e in entries
            if e.kind == "card" and e.data["identity_state"] != "retired"
        }
        faces = {
            str(e.data["id"])
            for e in entries
            if e.kind == "face" and e.data["card_id"] in cards
        }
        printings = {
            str(e.data["id"])
            for e in entries
            if e.kind == "printing"
            and e.data["card_id"] in cards
            and all(m["face_id"] in faces for m in _maps(e))
        }
        return tuple(
            e
            for e in entries
            if e.kind == kind and _visible(e, cards, faces, printings)
        )

    def resolve_int_id(self, int_id: int) -> RepairHint | None:
        """Resolve the original printing and require choices for a split in its history."""
        printing_id = next(
            (
                str(e.data["printing_id"])
                for e in self.entries()
                if e.kind == "card_int_id" and e.data["int_id"] == int_id
            ),
            None,
        )
        if printing_id is None:
            return None
        printing = self.records["printing:" + printing_id].entry()
        assert printing is not None
        choices: set[str] = set()
        changed = False
        for repair in self.repairs:
            if any(m.printing_id == printing_id for m in repair.printing_moves):
                changed = True
                if repair.kind == "split":
                    choices.update(repair.new_card_ids)
            if repair.old_card_id in choices:
                if repair.retire_old:
                    choices.remove(repair.old_card_id)
                choices.update(repair.new_card_ids)
        return RepairHint(
            "choice_required" if choices else "repaired" if changed else "unchanged",
            printing_id,
            str(printing.data["card_id"]),
            tuple(sorted(choices)),
        )


def frozen_records(records: dict[str, Entity]) -> Mapping[str, Entity]:
    """Prevent the caller or a later transaction from mutating a completed result."""
    return MappingProxyType(dict(sorted(records.items())))


def _maps(entry: Entry) -> list[dict[str, object]]:
    raw = entry.data["source_face_map"]
    assert isinstance(raw, list)
    return [dict(m) for m in raw if isinstance(m, dict)]


def _visible(
    entry: Entry, cards: set[str], faces: set[str], printings: set[str]
) -> bool:
    data = entry.data
    if entry.kind == "card":
        return data["id"] in cards
    if entry.kind == "face":
        return data["id"] in faces
    if entry.kind == "printing":
        return data["id"] in printings
    if entry.kind == "card_int_id":
        return data["printing_id"] in printings
    if entry.kind == "art":
        uses = data["uses"]
        return isinstance(uses, list) and any(
            isinstance(use, dict) and use["printing_id"] in printings for use in uses
        )
    return False

"""Current name application inputs and its atomic write boundary."""

from dataclasses import dataclass
from functools import cached_property
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.domains.digital.name_policies.loader import load
from sve_carddb.domains.translations.digital import name_proof
from sve_carddb.domains.translations.names.counterparts import NameCandidate

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.domains.catalog.adoption_models import SourceRef
    from sve_carddb.domains.digital.links.importer import Result as LinkResult
    from sve_carddb.domains.digital.name_policies.loader import Snapshot
    from sve_carddb.domains.digital.name_policies.projection import Plan
    from sve_carddb.domains.digital.name_policies.results import Result
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.domains.translations.names.resolve import Names
    from sve_carddb.domains.translations.names.sources import NameOwner
    from sve_carddb.domains.translations.sources import Sources


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str
    application_at: str = ""

    @cached_property
    def snapshot(self) -> Snapshot:
        """Reuse current inputs within this command."""
        return load(self.root, self.authored_revision)

    def load(self) -> Snapshot:
        """Reuse current inputs within this command."""
        return self.snapshot

    def configuration(self) -> dict[str, JsonValue]:
        """Declare the selected policy and the explicit first-application baseline."""
        snapshot = self.load()
        if len(snapshot.current_names) != 1:
            raise ValueError("Current name application needs one current name policy")
        return {
            "digital_name_application": {
                "recipe": "owner-name-current-v2",
                **snapshot.configuration(),
            }
        }


def prepare(
    db: Database,
    inputs: Inputs,
    texts: TextPlan,
    *,
    sources: Sources,
    replay: Names,
    links: LinkResult | None = None,
) -> Plan:
    """Check current rules before inspecting each owner's name candidates."""
    from sve_carddb.domains.digital.name_policies.projection import (  # ruff: ignore[import-outside-top-level] -- projection shares the entry point inputs without eager circular imports
        prepare as prepare_current,
    )

    return prepare_current(
        db, inputs, texts, sources=sources, replay=replay, links=links
    )


def populate(
    db: Database,
    inputs: Inputs,
    texts: TextPlan,
    *,
    sources: Sources,
    replay: Names,
    links: LinkResult | None = None,
) -> Result:
    """Recompute current owner-local selection at the write boundary."""
    from sve_carddb.domains.digital.name_policies.projection import (  # ruff: ignore[import-outside-top-level] -- projection shares the entry point inputs without eager circular imports
        populate as populate_current,
    )

    return populate_current(
        db, inputs, texts, sources=sources, replay=replay, links=links
    )


def _counterparts(
    db: Database,
    sources: Sources,
    owner: NameOwner,
    links: LinkResult | None,
    *,
    name_ref: SourceRef | None = None,
) -> tuple[NameCandidate, ...]:
    if links is None:
        return ()
    eligible = links.eligible_owner(db, sources, owner, name_ref=name_ref)
    faces = {r.values["id"]: r.values for r in db.rows("digital_face")}
    cards = {r.values["id"]: r.values for r in db.rows("digital_card")}
    units = {r.values["id"]: r.values for r in db.rows("text_unit")}
    names = {
        r.values["digital_face_id"]: str(units[r.values["name_unit_id"]]["text"])
        for r in db.rows("digital_text")
        if r.values["lang"] == "zh-Hant"
    }
    result = []
    for row in db.rows("digital_link"):
        link = row.values
        if link["id"] not in eligible or link["digital_face_id"] not in names:
            continue
        face = faces[link["digital_face_id"]]
        card = cards[link["digital_card_id"]]
        text = names[face["id"]]
        ref, source = name_proof(
            sources,
            str(card["game"]),
            str(card["official_id"]),
            str(face["phase"]),
            "zh-Hant",
            text,
        )
        result.append(
            NameCandidate(
                text,
                "official_" + str(card["game"]),
                "digital_official",
                str(link["decision_id"]),
                source,
                (ref,),
            )
        )
    return tuple(sorted(result, key=lambda c: (c.origin, c.text, c.decision_id or "")))

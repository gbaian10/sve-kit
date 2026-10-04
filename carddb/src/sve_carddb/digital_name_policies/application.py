"""Current name application inputs and its atomic write boundary."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.digital_links.loader import link_id
from sve_carddb.digital_name_policies.loader import load
from sve_carddb.maintainers import is_maintainer
from sve_carddb.translations.counterparts import NameCandidate
from sve_carddb.translations.digital import name_proof

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import InputRecord
    from sve_carddb.catalog.adoption_models import SourceRef
    from sve_carddb.digital_links.importer import Result as LinkResult
    from sve_carddb.digital_name_policies.current_application import Plan
    from sve_carddb.digital_name_policies.loader import Snapshot
    from sve_carddb.snapshot.project.evidence import DisplayBinding
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.translations.current_names import Names
    from sve_carddb.translations.name_sources import NameOwner
    from sve_carddb.translations.sources import Sources


@dataclass(frozen=True)
class Inputs:
    root: Path
    repository: Path
    authored_revision: str
    application_at: str = ""

    def load(self) -> Snapshot:
        """Read the entire immutable authored closure, including historical versions."""
        return load(self.root, self.repository, self.authored_revision)

    def configuration(self) -> dict[str, JsonValue]:
        """Declare the selected policy and the explicit first-application baseline."""
        snapshot = self.load()
        if len(snapshot.current_names) != 1:
            raise ValueError("Current name application needs one current name policy")
        return {
            "digital_name_application": {
                "recipe": "owner-name-current-v2",
                **snapshot.pins(),
            }
        }


@dataclass(frozen=True)
class Result:
    record: InputRecord
    bindings: tuple[DisplayBinding, ...]
    report: dict[str, JsonValue]


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
    from sve_carddb.digital_name_policies.current_application import (  # ruff: ignore[import-outside-top-level] -- the current plan shares this module's input/result models
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
    from sve_carddb.digital_name_policies.current_application import (  # ruff: ignore[import-outside-top-level] -- the current plan shares this module's input/result models
        populate as populate_current,
    )

    return populate_current(
        db, inputs, texts, sources=sources, replay=replay, links=links
    )


def _counterparts(  # ruff: ignore[too-many-locals] -- relation, actual sample membership and frozen name proof are separate gates
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
    records = {r.values["id"]: r.values for r in db.rows("decision")}
    fresh = {
        k: r
        for members in links.by_owner.values()
        for r in members
        for k in (r.record_key,)
    }
    decisions = dict(links.decisions)
    checked = {
        link_id(fresh[key])
        for key, decision in decisions.items()
        if key in fresh
        and key in links.checked_members
        and is_maintainer(records[decision]["reviewed_by"])
        and records[decision]["state"] in {"sampled", "confirmed"}
        and isinstance(records[decision]["sample_ids"], Json)
        and _sample_member(key, records[decision]["sample_ids"])
    }
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
        decision = records[link["decision_id"]]
        result.append(
            NameCandidate(
                text,
                "official_" + str(card["game"]),
                "digital_official",
                str(decision["id"]),
                str(decision["reviewed_at"]),
                source,
                (ref,),
                counterpart_checked=str(link["id"]) in checked,
            )
        )
    return tuple(sorted(result, key=lambda c: (c.origin, c.text, c.decision_id or "")))


def _sample_member(key: str, value: object) -> bool:
    return (
        isinstance(value, Json) and isinstance(value.value, list) and key in value.value
    )

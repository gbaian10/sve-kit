"""Merge producer evidence before deriving sparse mechanism facts and coverage."""

from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue, TypeAdapter, model_validator

from sve_carddb.build import Json
from sve_carddb.build.rows import insert_exact
from sve_carddb.core.json import canonical
from sve_carddb.core.models import Hash, RecordData, Text

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from sve_carddb.build import Database, Value

type Relation = Literal["has", "grants", "refers", "counts"]
RELATIONS: tuple[Relation, ...] = ("has", "grants", "refers", "counts")
type FieldKey = tuple[str, str | None, str, int | None, str]


class FieldCheck(RecordData, frozen=True):
    face_id: Text
    revision_id: Text
    field: Literal["effect", "section"]
    ordinal: int | None
    relation: Relation
    whole_field: bool

    @model_validator(mode="after")
    def field_ordinal(self) -> FieldCheck:
        """An ordinal identifies one exact section, never all sections."""
        if (self.field == "effect" and self.ordinal is not None) or (
            self.field == "section" and (self.ordinal is None or self.ordinal < 0)
        ):
            raise ValueError("Invalid mechanism check field ordinal")
        return self

    def key(self) -> FieldKey:
        """Bind a relation check to its selected face revision and field."""
        return self.face_id, self.revision_id, self.field, self.ordinal, self.relation


_CHECKS = TypeAdapter(tuple[FieldCheck, ...])


class Inspection(RecordData):
    card_id: Text
    keyword_id: Text
    scope: Literal["shared"]
    source_kind: Literal["dsl", "manual", "community"]
    source_text_hash: Hash
    producer_version: Text
    decision_id: Text | None
    dsl_id: Text | None
    checks: tuple[FieldCheck, ...]


def required_fields(db: Database) -> dict[str, set[FieldKey]]:
    """Missing current faces remain unmet requirements; empty main text is still a field."""
    current = {
        str(row.values["face_id"]): str(row.values["revision_id"])
        for row in db.rows("face_current")
        if row.values["region"] == "jp"
    }
    sections: dict[str, list[int]] = {}
    for row in db.rows("face_text_section"):
        sections.setdefault(str(row.values["revision_id"]), []).append(
            int(str(row.values["ordinal"]))
        )
    result: dict[str, set[FieldKey]] = {}
    for row in db.rows("face"):
        face = str(row.values["id"])
        revision = current.get(face)
        fields: list[tuple[str, int | None]] = [("effect", None)]
        if revision is not None:
            fields.extend(
                ("section", ordinal) for ordinal in sections.get(revision, ())
            )
        result.setdefault(str(row.values["card_id"]), set()).update(
            (face, revision, field, ordinal, relation)
            for field, ordinal in fields
            for relation in RELATIONS
        )
    return result


def _state(required: set[FieldKey], checks: Sequence[FieldCheck]) -> str:
    if not checks:
        return "unreviewed"
    covered = {check.key() for check in checks if check.whole_field}
    return "complete" if required and required <= covered else "partial"


def store_evidence(
    db: Database,
    positives: Sequence[Mapping[str, Value]],
    inspections: Sequence[Inspection],
) -> None:
    """Producers submit validated positives and checks, never coverage verdicts."""
    required = required_fields(db)
    for positive in positives:
        insert_exact(db, "card_mechanic", dict(positive), ("id",))
    for inspection in inspections:
        values: dict[str, Value] = dict(inspection.model_dump(exclude={"checks"}))
        values["state"] = _state(
            required.get(inspection.card_id, set()), inspection.checks
        )
        values["checked_ranges"] = Json(
            [check.model_dump(mode="json") for check in inspection.checks]
        )
        insert_exact(
            db,
            "mechanic_coverage",
            values,
            ("card_id", "keyword_id", "scope", "producer_version"),
        )


def _shortest(selected: set[str], universe: set[str]) -> tuple[str, list[JsonValue]]:
    complement = universe - selected
    return (
        ("include", list[JsonValue](sorted(selected)))
        if len(selected) <= len(complement)
        else ("exclude", list[JsonValue](sorted(complement)))
    )


def _project_positive(
    db: Database, hashes: Mapping[str, str], versions: frozenset[str]
) -> None:
    """Only exact fresh evidence can enter the public sparse sets."""
    decisions = {
        str(row.values["id"]): row.values["state"] for row in db.rows("decision")
    }
    overrides: dict[str, list[Mapping[str, Value]]] = {}
    for row in db.rows("mechanic_override"):
        overrides.setdefault(str(row.values["target_tag_id"]), []).append(row.values)
    positive: dict[tuple[str, str, str], tuple[set[str], set[str]]] = {}
    for row in db.rows("card_mechanic"):
        tag = row.values
        if (
            tag["scope"] != "shared"
            or tag["source_text_hash"] != hashes.get(str(tag["card_id"]))
            or tag["producer_version"] not in versions
        ):
            continue
        if any(
            override["source_text_hash"] == tag["source_text_hash"]
            and override["producer_version"] == tag["producer_version"]
            and decisions[str(override["decision_id"])] == "confirmed"
            for override in overrides.get(str(tag["id"]), [])
        ):
            continue
        key = (str(tag["card_id"]), str(tag["keyword_id"]), str(tag["scope"]))
        relations, actions = positive.setdefault(key, (set(), set()))
        relations.add(str(tag["relation"]))
        if tag["action"] is not None:
            actions.add(str(tag["action"]))
    for (cid, keyword, scope), (relations, actions) in sorted(positive.items()):
        db.insert(
            "mechanic_projection",
            {
                "card_id": cid,
                "keyword_id": keyword,
                "scope": scope,
                "relations": Json(list[JsonValue](sorted(relations))),
                "actions": Json(list[JsonValue](sorted(actions))),
            },
        )


def _coverage_sets(
    db: Database, hashes: Mapping[str, str], versions: frozenset[str]
) -> tuple[dict[tuple[str, str], set[str]], dict[tuple[str, str], set[str]]]:
    required = required_fields(db)
    checks_by_keyword: dict[tuple[str, str, str], list[FieldCheck]] = {}
    for row in db.rows("mechanic_coverage"):
        value = row.values
        if (
            value["scope"] != "shared"
            or value["source_text_hash"] != hashes.get(str(value["card_id"]))
            or value["producer_version"] not in versions
        ):
            continue
        raw = value["checked_ranges"]
        if not isinstance(raw, Json):
            raise TypeError("Mechanism checks require a JSON array")
        checks = _CHECKS.validate_json(canonical(raw.value))
        cid = str(value["card_id"])
        fields = required.get(cid, set())
        if any(check.key() not in fields for check in checks):
            raise ValueError("Mechanism check differs from current card fields")
        if value["state"] != _state(fields, checks):
            raise ValueError("Mechanism coverage state differs from checked ranges")
        if checks:
            key = (cid, str(value["keyword_id"]), str(value["scope"]))
            checks_by_keyword.setdefault(key, []).extend(checks)
    complete: dict[tuple[str, str], set[str]] = {}
    partial: dict[tuple[str, str], set[str]] = {}
    for (cid, keyword, scope), merged in checks_by_keyword.items():
        selected = (
            complete
            if _state(required.get(cid, set()), merged) == "complete"
            else partial
        )
        selected.setdefault((cid, scope), set()).add(keyword)
    return complete, partial


def project_current(
    db: Database, hashes: Mapping[str, str], versions: frozenset[str]
) -> None:
    """Merge fresh checks across producers; only all fields and relations prove absence."""
    _project_positive(db, hashes, versions)
    universe = {str(row.values["id"]) for row in db.rows("keyword")}
    complete, partial = _coverage_sets(db, hashes, versions)
    for cid, scope in sorted(complete.keys() | partial.keys()):
        completed = complete.get((cid, scope), set())
        complete_all = bool(universe) and completed == universe
        complete_mode, complete_ids = _shortest(completed, universe)
        partial_mode, partial_ids = _shortest(
            partial.get((cid, scope), set()), universe - completed
        )
        db.insert(
            "card_mechanic_coverage",
            {
                "card_id": cid,
                "scope": scope,
                "complete_all": complete_all,
                "complete_mode": "include" if complete_all else complete_mode,
                "complete_keyword_ids": Json([] if complete_all else complete_ids),
                "partial_mode": "include" if complete_all else partial_mode,
                "partial_keyword_ids": Json([] if complete_all else partial_ids),
            },
        )

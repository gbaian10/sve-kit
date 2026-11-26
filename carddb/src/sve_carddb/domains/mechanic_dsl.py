"""Derive keyword relations from admitted whole-card programs instead of card wording."""

from collections import Counter
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.domains.effects.keyword_relations import TRACKED, program_relations
from sve_carddb.domains.mechanic_coverage import (
    RELATIONS,
    FieldCheck,
    Inspection,
    required_fields,
    store_evidence,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.effects.keyword_relations import Relation
    from sve_carddb.domains.keyword_self import CurrentFields
    from sve_carddb.domains.mechanics import Inputs

ADAPTER = "dsl-keyword-relations-v1"


def _programs(db: Database) -> dict[str, tuple[str, dict[str, JsonValue]]]:
    programs: dict[str, tuple[str, dict[str, JsonValue]]] = {}
    for row in db.rows("dsl_document"):
        ast = row.values["ast"]
        if not isinstance(ast, Json):
            raise TypeError("Program must be JSON")
        programs[str(row.values["card_id"])] = (
            str(row.values["id"]),
            object_value(ast.value),
        )
    return programs


def apply(
    db: Database,
    inputs: Inputs,
    hashes: Mapping[str, str],
    fields: Mapping[tuple[str, str], CurrentFields],
) -> dict[str, JsonValue]:
    """Submit program-derived labels and whole-card checks for the tracked keywords."""
    tracked = {
        record.code: record.id
        for record in inputs.registry.records
        if record.code in TRACKED
    }
    version = (
        ADAPTER
        + ":"
        + digest(
            canonical(
                {"adapter": ADAPTER, "keywords": list[JsonValue](sorted(tracked))}
            )
        )
    )
    programs = _programs(db)
    required = required_fields(db)
    confirmed = {
        str(row.values["id"])
        for row in db.rows("card")
        if row.values["identity_state"] == "confirmed"
    }
    positives: list[dict[str, Value]] = []
    inspections: list[Inspection] = []
    relations: Counter[str] = Counter()
    blank = 0
    for (cid, region), current in sorted(fields.items()):
        if region != "jp" or cid not in confirmed:
            continue
        program = programs.get(cid)
        uses: set[tuple[str, Relation]] = set()
        if program is None:
            # A card without rules text has nothing to author; any wording needs a program.
            if current.missing or any(current.main) or current.sections:
                continue
            blank += 1
        else:
            uses = {
                (tracked[code], relation)
                for code, relation in program_relations(program[1])
                if code in tracked
            }
        dsl_id = None if program is None else program[0]
        common: dict[str, Value] = {
            "card_id": cid,
            "scope": "shared",
            "source_kind": "manual" if program is None else "dsl",
            "source_text_hash": hashes[cid],
            "producer_version": version,
            "dsl_id": dsl_id,
            "decision_id": None,
        }
        for keyword, relation in sorted(uses):
            relations[relation] += 1
            positives.append(
                common
                | {
                    "id": "mechanic:"
                    + digest(
                        canonical(
                            {
                                "card_id": cid,
                                "keyword_id": keyword,
                                "scope": "shared",
                                "relation": relation,
                                "source_text_hash": hashes[cid],
                                "producer_version": version,
                                "dsl_id": dsl_id,
                            }
                        )
                    )[7:],
                    "keyword_id": keyword,
                    "relation": relation,
                    "action": None,
                    "source_id": None,
                }
            )
        # A face without current wording stays an unmet requirement, never a checked field.
        slots = sorted(
            {
                (face, revision, field, ordinal)
                for face, revision, field, ordinal, _ in required.get(cid, ())
                if revision is not None
            },
            key=lambda slot: (slot[0], slot[2], -1 if slot[3] is None else slot[3]),
        )
        checks = tuple(
            FieldCheck(
                face_id=face,
                revision_id=revision,
                field="effect" if field == "effect" else "section",
                ordinal=ordinal,
                relation=relation,
                whole_field=True,
            )
            for face, revision, field, ordinal in slots
            for relation in RELATIONS
        )
        inspections.extend(
            Inspection(
                card_id=cid,
                keyword_id=keyword,
                scope="shared",
                source_kind="manual" if program is None else "dsl",
                source_text_hash=hashes[cid],
                producer_version=version,
                decision_id=None,
                dsl_id=dsl_id,
                checks=checks,
            )
            for keyword in sorted(tracked.values())
        )
    store_evidence(db, positives, inspections)
    return {
        "adapter": ADAPTER,
        "producer_version": version,
        "programs": len(programs),
        "blank_cards": blank,
        "positive_tags": len(positives),
        "relations": dict[str, JsonValue](sorted(relations.items())),
        "inspections": len(inspections),
    }

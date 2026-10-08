"""Compare all adopted rules fields with the raw endpoint pinned by a reskin."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.importer import stat
from sve_carddb.domains.text_observations.intern import text_values

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.domains.text_observations import Vocabulary
    from sve_carddb.domains.text_observations.models import FaceObservation

FIELDS = (
    "face_id",
    "region",
    "source_id",
    "name_unit_id",
    "effect_unit_id",
    "class_code",
    "type_code",
    "cost",
    "attack",
    "defense",
)


def _plain(values: Mapping[str, Value]) -> dict[str, JsonValue]:
    result: dict[str, JsonValue] = {}
    for name, value in values.items():
        if value is not None and not isinstance(value, (str, int)):
            raise TypeError("Unexpected rules field type")
        result[name] = value
    return result


def actual_rules(db: Database) -> dict[str, bytes]:
    """Include ordered sections and every current-bearing membership in signatures."""
    rows = {
        str(row.values["id"]): _plain({name: row.values[name] for name in FIELDS})
        for row in db.rows("face_revision")
    }
    for table, field, output in (
        ("face_trait", "trait_code", "traits"),
        ("face_title", "title_code", "titles"),
        ("face_special_kind", "special_kind_code", "special_kinds"),
    ):
        members: dict[str, list[str]] = {}
        for row in db.rows(table):
            members.setdefault(str(row.values["revision_id"]), []).append(
                str(row.values[field])
            )
        for identifier, values in rows.items():
            values[output] = list[JsonValue](sorted(set(members.get(identifier, ()))))
    sections: dict[str, list[tuple[int, str]]] = {}
    for row in db.rows("face_text_section"):
        ordinal = row.values["ordinal"]
        assert isinstance(ordinal, int)
        sections.setdefault(str(row.values["revision_id"]), []).append(
            (ordinal, str(row.values["text_unit_id"]))
        )
    for identifier, values in rows.items():
        values["sections"] = [
            [ordinal, text] for ordinal, text in sorted(sections.get(identifier, ()))
        ]
    return {identifier: canonical(values) for identifier, values in rows.items()}


def expected_rules(item: FaceObservation, vocabulary: Vocabulary) -> bytes | None:
    """Recompute exact source semantics independently of the stored revision ID."""
    content = item.content
    if content.effect is None:
        return None
    language = "ja" if item.region == "jp" else "en"
    kind = vocabulary.lookup(item.region, "type", content.type_raw)
    cost, attack, defense = map(stat, content.stats)
    values: dict[str, JsonValue] = {
        "face_id": item.face_id,
        "region": item.region,
        "source_id": item.card.source.id,
        "name_unit_id": str(
            text_values(LocalizedText(lang=language, text=content.name))["id"]
        ),
        "effect_unit_id": str(
            text_values(LocalizedText(lang=language, text=content.effect))["id"]
        ),
        "class_code": None
        if content.class_raw == "-"
        else vocabulary.lookup(item.region, "class", content.class_raw).code,
        "type_code": kind.code,
        "cost": cost,
        "attack": attack,
        "defense": defense,
        "traits": list[JsonValue](
            sorted(
                {
                    vocabulary.lookup(item.region, "trait", trait).code
                    for trait in content.traits
                }
            )
        ),
        "titles": []
        if content.title is None
        else [vocabulary.lookup(item.region, "title", content.title).code],
        "special_kinds": list[JsonValue](sorted(set(kind.special_kinds))),
        "sections": [
            [
                ordinal,
                str(text_values(LocalizedText(lang=language, text=section))["id"]),
            ]
            for ordinal, section in enumerate(content.sections)
        ],
    }
    return canonical(values)

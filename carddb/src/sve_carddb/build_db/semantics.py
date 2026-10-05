"""Check that stored semantic rule hashes match the exact interned text."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json
from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.build_db import Database


def verify_semantics(db: Database) -> None:
    """Validate the exact canonical rule hash independently of the importer."""
    units = {r.values["id"]: r.values["text"] for r in db.rows("text_unit")}
    for row in db.rows("face_semantics"):
        values = row.values
        sections = values["rule_sections"]
        if not isinstance(sections, Json) or not isinstance(sections.value, list):
            raise TypeError("Invalid semantic sections")
        text = units.get(values["rule_text_unit_id"])
        strings: list[JsonValue] = []
        for key in sections.value:
            if not isinstance(key, str) or not isinstance(units.get(key), str):
                raise TypeError("Semantic section text dependency is missing")
            value = units[key]
            if not isinstance(value, str):
                raise TypeError("Semantic section requires exact text")
            strings.append(value)
        if (
            not isinstance(text, str)
            or digest(canonical({"rule_text": text, "rule_sections": strings}))
            != values["rule_hash"]
        ):
            raise ValueError(
                "Immutable semantic rule hash does not match exact content"
            )

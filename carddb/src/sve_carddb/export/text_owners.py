"""Resolve public text pointers through exact owners, including printed and historical fields."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, integer, object_value, string

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.reader import Row, View


def owner_key(owner: Row) -> bytes:
    """Reject a loose DB owner or extra key before an equal string can hide the mistake."""
    kind = string(owner["kind"])
    keys = (
        {"kind", "vocabulary_kind", "code"}
        if kind == "vocabulary"
        else {"kind", "id", "face_id"}
        if kind == "printing_face"
        else {"kind", "id"}
    )
    if (
        kind
        not in {
            "face_revision",
            "printing_face",
            "qa_version",
            "cr_clause",
            "vocabulary",
            "product_family",
            "product",
            "keyword",
        }
        or set(owner) != keys
    ):
        raise ValueError("Invalid public text owner")
    if any(not string(owner[key]) for key in keys):
        raise ValueError("Public text owner has an empty identifier")
    return canonical(owner)


@dataclass(frozen=True)
class TextOwner:
    owner: Row
    row: Row
    card_id: str | None = None
    face_id: str | None = None
    region: str | None = None
    mapping_state: str | None = None

    def field(self, name: str, ordinal: JsonValue) -> str:
        """The field must exist on this precise owner; null original text is not empty text."""
        kind = string(self.owner["kind"])
        fields = {
            "face_revision": {
                "name": "name_unit_id",
                "effect": "effect_unit_id",
                "section": "sections",
            },
            "printing_face": {
                "name": "printed_name_unit_id",
                "effect": "printed_effect_unit_id",
                "flavor": "flavor_unit_id",
                "section": "sections",
            },
            "qa_version": {"question": "question_unit_id", "answer": "answer_unit_id"},
            "cr_clause": {"effect": "text_unit_id"},
            "vocabulary": {"label": "label_unit_id"},
            "product_family": {"label": "name_unit_id"},
            "product": {"label": "name_unit_id"},
            "keyword": {
                "label": "name_unit_id",
                "effect": "definition_unit_id",
                "action_label": "actions",
            },
        }
        if name not in fields[kind] or (name in {"section", "action_label"}) != (
            ordinal is not None
        ):
            raise ValueError("Public owner field or ordinal is invalid")
        value = self.row[fields[kind][name]]
        if name in {"section", "action_label"} and integer(ordinal) < 0:
            raise ValueError("Public text ordinal cannot be negative")
        if name == "section":
            found = [
                object_value(s)
                for s in array(value)
                if object_value(s)["ordinal"] == ordinal
            ]
            if len(found) != 1:
                raise ValueError("Public section ordinal does not exist")
            value = found[0]["text_unit_id"]
        elif name == "action_label":
            index = integer(ordinal)
            if not 0 <= index < len(array(value)):
                raise ValueError("Public action label ordinal does not exist")
            value = object_value(array(value)[index])["label_unit_id"]
        if value is None:
            raise ValueError("Public text pointer has no exact original text")
        return string(value)


class TextOwners:
    def __init__(self, view: View) -> None:
        self.by_owner: dict[bytes, TextOwner] = {}
        cards = {string(c["id"]): c for c in view["card"]}
        faces = {string(f["id"]): f for f in view["face"]}
        for table in (
            "qa_version",
            "cr_clause",
            "product_family",
            "product",
            "keyword",
        ):
            for row in view[table]:
                self._add(TextOwner({"kind": table, "id": row["id"]}, row))
        for row in view["vocabulary"]:
            self._add(
                TextOwner(
                    {
                        "kind": "vocabulary",
                        "vocabulary_kind": row["kind"],
                        "code": row["code"],
                    },
                    row,
                )
            )
        for row in view["face_revision"]:
            face = faces[string(row["face_id"])]
            card = cards[string(face["card_id"])]
            self._add(
                TextOwner(
                    {"kind": "face_revision", "id": row["id"]},
                    row,
                    string(card["id"]),
                    string(face["id"]),
                    string(row["region"]),
                    _mapping(card, string(row["region"])),
                )
            )
        for printing in view["printing"]:
            card = cards[string(printing["card_id"])]
            region = string(printing["region"])
            for value in array(printing["faces"]):
                row = object_value(value)
                self._add(
                    TextOwner(
                        {
                            "kind": "printing_face",
                            "id": printing["id"],
                            "face_id": row["face_id"],
                        },
                        row,
                        string(card["id"]),
                        string(row["face_id"]),
                        region,
                        _mapping(card, region),
                    )
                )

    def _add(self, value: TextOwner) -> None:
        key = owner_key(value.owner)
        if key in self.by_owner:
            raise ValueError("Duplicate public text owner")
        self.by_owner[key] = value

    def get(self, owner: Row) -> TextOwner:
        """Lookup uses every member of the owner's closed composite key."""
        key = owner_key(owner)
        if key not in self.by_owner:
            raise ValueError("Public text owner does not exist")
        return self.by_owner[key]

    def pointer(self, pointer: Row) -> str:
        """Resolve through the public owner instead of deduplicated text or current pointers."""
        if set(pointer) != {"owner", "field", "ordinal"}:
            raise ValueError("Invalid public text pointer")
        return self.get(object_value(pointer["owner"])).field(
            string(pointer["field"]), pointer["ordinal"]
        )


def _mapping(card: Row, region: str) -> str:
    values = [
        object_value(r)
        for r in array(card["regions"])
        if object_value(r)["region"] == region
    ]
    if len(values) != 1:
        raise ValueError("Text owner card region is missing or ambiguous")
    return string(values[0]["mapping_state"])

"""Derive only whole names from the concept on the exact public source owner."""

from copy import deepcopy
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.export.text_owners import TextOwner, TextOwners

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.reader import Row, View


def whole_name(unit: Row, concept: str) -> Row:
    """Virtual sets retain the annotation-v1 identity and scalar coordinates."""
    text = string(unit["text"])
    if not text:
        raise ValueError("public-annotation/range")
    occurrences: list[JsonValue] = [
        {
            "ordinal": 0,
            "reference": {"kind": "card_name", "term_id": concept},
            "ranges": [{"start": 0, "end": len(text)}],
            "bold": True,
        }
    ]
    return {
        "id": "ann:"
        + hash_payload(
            {
                "recipe": "annotation-v1",
                "text_unit_id": unit["id"],
                "occurrences": occurrences,
            }
        ),
        "text_unit_id": unit["id"],
        "occurrences": occurrences,
    }


def assign_name_concepts(view: View) -> None:
    """P takes an exact stored occurrence, never a rules name or an equal string."""
    owners = TextOwners(view)
    texts = {string(row["id"]): row for row in view["text_unit"]}
    sets = {string(row["id"]): row for row in view["annotation_set"]}
    for owner in owners.by_owner.values():
        if owner.owner["kind"] in {"face_revision", "printing_face"}:
            owner.row["name_concept_id"] = None
    for field in view["field_annotation"]:
        owner = owners.get(object_value(field["owner"]))
        if (
            owner.owner["kind"] not in {"face_revision", "printing_face"}
            or field["field"] != "name"
            or field["ordinal"] is not None
        ):
            continue
        annotation = sets[string(field["annotation_set_id"])]
        occurrences = array(annotation["occurrences"])
        if len(occurrences) != 1:
            continue
        reference = object_value(object_value(occurrences[0])["reference"])
        if reference["kind"] != "card_name":
            continue
        concept = string(reference["term_id"])
        if annotation == whole_name(texts[owner.field("name", None)], concept):
            owner.row["name_concept_id"] = concept
    _translation_kinds(view, owners, texts, sets)


def _translation_kinds(
    view: View, owners: TextOwners, texts: dict[str, Row], sets: dict[str, Row]
) -> None:
    uses: dict[str, list[Row]] = {}
    for owner in owners.by_owner.values():
        for raw in array(owner.row["translations"]):
            value = object_value(raw)
            uses.setdefault(string(value["translation_id"]), []).append(value)
    for translation in view["translation"]:
        identifier = translation["annotation_set_id"]
        translation["annotation_kind"] = "none" if identifier is None else "explicit"
        if identifier is None:
            continue
        selected = uses.get(string(translation["id"]), [])
        concepts = [_source_concept(owners, value) for value in selected]
        if (
            concepts
            and concepts[0] is not None
            and all(c == concepts[0] for c in concepts)
            and sets[string(identifier)]
            == whole_name(
                texts[string(translation["text_unit_id"])], string(concepts[0])
            )
        ):
            translation["annotation_kind"] = "whole_name"


def _source_concept(owners: TextOwners, value: Row) -> str | None:
    pointer = object_value(value["source"])
    try:
        owner = owners.get(object_value(pointer["owner"]))
    except ValueError:
        raise ValueError("public-annotation/owner") from None
    if (
        value["field"] != "name"
        or value["ordinal"] is not None
        or pointer["field"] != "name"
        or pointer["ordinal"] is not None
        or owner.owner["kind"] not in {"face_revision", "printing_face"}
    ):
        return None
    concept = owner.row.get("name_concept_id")
    return None if concept is None else string(concept)


def compact_names(view: View) -> View:
    """Keep any set still used by an explicit effect, section or other occurrence."""
    result = deepcopy(view)
    owners = TextOwners(result)
    texts = {string(row["id"]): row for row in result["text_unit"]}
    retained = []
    for field in result["field_annotation"]:
        owner = owners.get(object_value(field["owner"]))
        concept = owner.row.get("name_concept_id")
        if (
            field["field"] == "name"
            and field["ordinal"] is None
            and concept is not None
            and field["annotation_set_id"]
            == whole_name(texts[owner.field("name", None)], string(concept))["id"]
        ):
            continue
        retained.append(field)
    result["field_annotation"] = retained
    for translation in result["translation"]:
        if translation["annotation_kind"] == "whole_name":
            translation["annotation_set_id"] = None
    used = {row["annotation_set_id"] for row in retained} | {
        row["annotation_set_id"]
        for row in result["translation"]
        if row["annotation_set_id"] is not None
    }
    result["annotation_set"] = [
        row for row in result["annotation_set"] if row["id"] in used
    ]
    return result


class _Expansion:
    def __init__(self, view: View) -> None:
        self.view = view
        self.owners = TextOwners(view)
        self.texts = {string(row["id"]): row for row in view["text_unit"]}
        self.sets = {string(row["id"]): row for row in view["annotation_set"]}
        self.fields = {
            canonical([row[name] for name in ("owner", "field", "ordinal")]): row
            for row in view["field_annotation"]
        }
        self.translations = {string(row["id"]): row for row in view["translation"]}

    def add(self, unit: str, concept: str) -> Row:
        if unit not in self.texts:
            raise ValueError("public-annotation/reference")
        annotation = whole_name(self.texts[unit], concept)
        identifier = string(annotation["id"])
        if identifier in self.sets and self.sets[identifier] != annotation:
            raise ValueError("public-annotation/identity")
        if identifier not in self.sets:
            self.sets[identifier] = annotation
            self.view["annotation_set"].append(annotation)
        return annotation

    def original(self, owner: TextOwner) -> None:
        concept = owner.row.get("name_concept_id")
        if concept is None:
            return
        annotation = self.add(owner.field("name", None), string(concept))
        pointer: Row = {"owner": owner.owner, "field": "name", "ordinal": None}
        key = canonical([pointer[name] for name in ("owner", "field", "ordinal")])
        if (
            key in self.fields
            and self.fields[key]["annotation_set_id"] != annotation["id"]
        ):
            raise ValueError("public-annotation/text_identity")
        if key not in self.fields:
            row = pointer | {"annotation_set_id": annotation["id"]}
            self.fields[key] = row
            self.view["field_annotation"].append(row)

    def translated(self, owner: TextOwner, value: Row) -> None:
        translation = self.translations.get(string(value["translation_id"]))
        if translation is None or translation["annotation_kind"] != "whole_name":
            return
        concept = _source_concept(self.owners, value)
        if concept is None:
            raise ValueError("public-annotation/owner")
        receiver_concept = owner.row.get("name_concept_id")
        if receiver_concept is not None and receiver_concept != concept:
            raise ValueError("public-annotation/text_identity")
        annotation = self.add(string(translation["text_unit_id"]), concept)
        if translation["annotation_set_id"] not in {None, annotation["id"]}:
            raise ValueError("public-annotation/text_identity")
        translation["annotation_set_id"] = annotation["id"]


def expand_names(view: View) -> None:
    """Full readers require source closure before restoring any omitted set."""
    expansion = _Expansion(view)
    for owner in expansion.owners.by_owner.values():
        expansion.original(owner)
        for raw in array(owner.row["translations"]):
            expansion.translated(owner, object_value(raw))
    for translation in expansion.translations.values():
        kind = translation["annotation_kind"]
        if kind not in {"none", "explicit", "whole_name"} or (kind == "none") != (
            translation["annotation_set_id"] is None
        ):
            raise ValueError("public-annotation/reference")

"""Project exact stored annotation uses without searching rendered text for labels."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.domains.translations.four_layer_storage import read_annotation
from sve_carddb.export.project.source import Record, Source, json_list
from sve_carddb.export.project.translations import pointer
from sve_carddb.export.text_owners import TextOwners

if TYPE_CHECKING:
    from collections.abc import Mapping, Sequence

    from sve_carddb.export.project.evidence import Decisions

USE_FIELDS = "id,context_id,field,ordinal,face_revision_id,printing_id,face_id,qa_version_id,cr_clause_id,vocabulary_kind,vocabulary_code,keyword_id,product_family_id,product_id"


def annotations(
    source: Source, view: dict[str, list[Record]], decisions: Decisions
) -> None:
    """Each public use retains its own coordinates even when exact strings share a set."""
    owners = TextOwners(view)
    sets = {}
    for row in source.rows("annotation_set", "id"):
        definition = read_annotation(source.db, string(row["id"]))
        if definition.occurrences:
            sets[definition.id] = object_value(definition.model_dump(mode="json"))
    use_annotations = {
        string(row["use_id"]): string(row["annotation_set_id"])
        for row in source.rows("translation_use_annotation", "use_id,annotation_set_id")
    }
    fields: dict[bytes, Record] = {}
    for use in source.rows("translation_use", USE_FIELDS):
        identifier = use_annotations.get(string(use["id"]))
        if identifier is None or identifier not in sets:
            continue
        location = pointer(use)
        if canonical(location["owner"]) not in owners.by_owner:
            continue
        if owners.pointer(location) != sets[identifier]["text_unit_id"]:
            raise ValueError(
                "Original annotation differs from exact public owner field"
            )
        key = canonical(location)
        projected = location | {"annotation_set_id": identifier}
        if key in fields and fields[key] != projected:
            raise ValueError("Original annotation uses disagree for one owner field")
        fields[key] = projected
    view["field_annotation"] = list(fields.values())
    translated = _translated(source, view, sets, fields)

    _concepts(source, view, decisions)
    validate_completeness(view, tuple(fields.values()), translated)


def _concepts(
    source: Source, view: dict[str, list[Record]], decisions: Decisions
) -> None:
    terms = source.index("glossary_term", "id,category")
    keywords = {
        string(row["term_id"]): string(row["id"])
        for row in source.rows("keyword", "id,term_id")
        if any(
            public["id"] == row["id"] and public["definition_unit_id"] is not None
            for public in view["keyword"]
        )
    }
    referenced = set()
    for annotation in view["annotation_set"]:
        for raw in array(annotation["occurrences"]):
            reference = object_value(object_value(raw)["reference"])
            if reference["kind"] == "vocabulary":
                continue
            referenced.add(
                string(
                    reference["term_id"]
                    if reference["kind"] == "card_name"
                    else reference["key"]
                )
            )
    concepts: list[Record] = []
    for identifier in sorted(referenced):
        term = terms[identifier]
        explanations: list[Record] = []
        if identifier in keywords:
            explanations.append({"kind": "keyword", "id": keywords[identifier]})
        concepts.append(
            {
                "id": identifier,
                "category": term["category"],
                "explanations": json_list(explanations),
                "card_ids": json_list(decisions.card_name_targets.get(identifier, ()))
                if term["category"] == "card_name"
                else [],
            }
        )
    view["annotation_concept"] = concepts


def _translated(
    source: Source,
    view: dict[str, list[Record]],
    sets: dict[str, Record],
    fields: dict[bytes, Record],
) -> dict[str, str]:
    translated = {
        string(row["translation_id"]): string(row["annotation_set_id"])
        for row in source.rows(
            "translation_annotation", "translation_id,annotation_set_id"
        )
    }
    used = {string(row["annotation_set_id"]) for row in fields.values()}
    for translation in view["translation"]:
        identifier = translated.get(string(translation["id"]))
        translation["annotation_set_id"] = identifier if identifier in sets else None
        if identifier in sets:
            if translation["text_unit_id"] != sets[identifier]["text_unit_id"]:
                raise ValueError("Target annotation differs from exact translated text")
            used.add(identifier)
    view["annotation_set"] = [sets[key] for key in sorted(used)]

    return {
        string(row["id"]): translated[string(row["id"])]
        for row in view["translation"]
        if translated.get(string(row["id"])) in sets
    }


def validate_completeness(
    view: dict[str, list[Record]],
    originals: Sequence[Record],
    translations: Mapping[str, str],
) -> None:
    """R cannot reconstruct omitted C declarations; P must check them before handoff."""
    fields = {
        canonical({name: row[name] for name in ("owner", "field", "ordinal")}): row[
            "annotation_set_id"
        ]
        for row in view["field_annotation"]
    }
    targets = {
        string(row["id"]): row["annotation_set_id"] for row in view["translation"]
    }
    sets = {row["id"] for row in view["annotation_set"]}
    if any(
        fields.get(
            canonical({name: row[name] for name in ("owner", "field", "ordinal")})
        )
        != row["annotation_set_id"]
        or row["annotation_set_id"] not in sets
        for row in originals
    ) or any(
        targets.get(identifier) != annotation or annotation not in sets
        for identifier, annotation in translations.items()
    ):
        raise ValueError("public-annotation/annotation_incomplete")

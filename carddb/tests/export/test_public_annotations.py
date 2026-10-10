"""Shared mutations dispatch to the production schema, owner and annotation boundaries."""

from copy import deepcopy
from dataclasses import replace
from functools import partial
from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError as SchemaError

from sve_carddb.contracts.snapshot import descriptor, validate
from sve_carddb.core.json import array, integer, object_value, parse, string
from sve_carddb.domains.translations.names.bindings import DisplayBinding
from sve_carddb.export.project import DisplayCheck, display_text, effective_support
from sve_carddb.export.project.annotations import validate_completeness
from sve_carddb.export.reader import compatible, read_snapshot
from sve_carddb.export.reader_annotations import validate_annotations
from sve_carddb.export.text_owners import TextOwners
from sve_carddb.export.transport.compression import Blob
from sve_carddb.export.transport.measure import accounted_sizes

from ..support.public_annotation_fixtures import (
    CASES,
    component_input,
    decode,
    inputs,
    view,
)
from ..support.snapshot_contract_fixtures import fixture, payloads
from ..support.snapshot_project_fixtures import TEXT, decisions
from .test_snapshot_current_translations import current_schema, db, jp_source
from .test_snapshot_project import EN_TEXT, dual_project, dual_region

__all__ = ("current_schema", "db")

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build import Database

HANDLED = {
    "read_projection",
    "schema",
    "descriptor",
    "canonical_bytes",
    "admission",
    "owner_field",
    "unicode_ranges",
    "annotation_identity",
    "shared_annotation",
    "annotation_emphasis",
    "concept_references",
    "display",
    "envelope_versions",
    "size_accounting",
}
CASES_TO_READ = tuple(
    object_value(c)
    for c in array(CASES["cases"])
    if object_value(c)["operation"] in HANDLED
)


def _schema(case: dict[str, JsonValue], scenario: JsonValue) -> dict[str, JsonValue]:
    validate(string(object_value(case["input"])["definition"]), scenario)
    return {}


def _canonical(
    _case: dict[str, JsonValue], scenario: JsonValue
) -> dict[str, JsonValue]:
    parse(string(object_value(scenario)["raw_json"]).encode())
    return {}


def _descriptor(
    _case: dict[str, JsonValue], scenario: JsonValue
) -> dict[str, JsonValue]:
    value = object_value(scenario)
    if {name: value[name] for name in ("columns", "items")} != descriptor(
        string(value["definition"])
    ):
        raise ValueError("public-annotation/descriptor")
    return {}


def _admission(
    _case: dict[str, JsonValue], scenario: JsonValue
) -> dict[str, JsonValue]:
    value = object_value(scenario)
    entry = object_value(value["value"])
    if (
        not compatible(entry)
        or entry["format_version"] not in array(value["supported_formats"])
        or tuple(map(int, string(entry["min_reader_version"]).split(".")))
        > tuple(map(int, string(value["reader_version"]).split(".")))
        or not set(map(string, array(entry["required_capabilities"])))
        <= set(map(string, array(value["supported_capabilities"])))
    ):
        raise ValueError("public-annotation/admission")
    return {}


def _projection(
    case: dict[str, JsonValue], scenario: JsonValue
) -> dict[str, JsonValue]:
    value = object_value(scenario)
    if case["group"] == "PA-01":
        read_snapshot(
            object_value(fixture("manifest.json")) | object_value(value["admission"]),
            payloads(),
        )
        return {}
    projection = view(value)
    validate_annotations(projection, tuple(map(string, array(value["languages"]))))
    selection = decode(
        "FieldTranslation",
        object_value(array(value["field_translations"])[-1])["value"],
    )
    translation = next(
        row
        for row in projection["translation"]
        if row["id"] == selection["translation_id"]
    )
    return {
        "source_owner_kind": object_value(object_value(selection["source"])["owner"])[
            "kind"
        ],
        "source_text_unit_id": translation["source_unit_id"],
        "target_text_unit_id": translation["text_unit_id"],
    }


def _identity(_case: dict[str, JsonValue], scenario: JsonValue) -> dict[str, JsonValue]:
    value = object_value(scenario)
    identifiers: list[JsonValue] = []
    for row in array(value["sets"]):
        used = {
            object_value(array(occurrence)[1]).get(
                "term_id", object_value(array(occurrence)[1]).get("key")
            )
            for occurrence in array(array(row)[2])
        }
        single = value | {
            "sets": [row],
            "concepts": [
                concept
                for concept in array(value["concepts"])
                if array(concept)[0] in used
            ],
        }
        projection = view(component_input(single))
        validate_annotations(projection, ("en", "ja", "zh-Hant"))
        identifiers.append(projection["annotation_set"][0]["id"])
    return {
        "text_unit_count": 1,
        "annotation_set_ids": identifiers,
        "distinct": len(set(map(string, identifiers))) == len(identifiers),
    }


def _unicode(
    value: dict[str, JsonValue], projection: dict[str, list[dict[str, JsonValue]]]
) -> dict[str, JsonValue]:
    text = string(array(value["text_unit"])[2])
    boundaries: list[JsonValue] = [0]
    for scalar in text:
        boundaries.append(
            integer(boundaries[-1]) + len(scalar.encode("utf-16-le")) // 2
        )
    spans = [
        object_value(span)
        for row in projection["annotation_set"]
        for occurrence in array(row["occurrences"])
        for span in array(object_value(occurrence)["ranges"])
    ]
    return {
        "codepoint_length": len(text),
        "utf16_length": boundaries[-1],
        "utf16_boundaries": boundaries,
        "slices": [text[integer(r["start"]) : integer(r["end"])] for r in spans],
        "utf16_ranges": [
            [boundaries[integer(r["start"])], boundaries[integer(r["end"])]]
            for r in spans
        ],
    }


def _component(case: dict[str, JsonValue], scenario: JsonValue) -> dict[str, JsonValue]:
    value = object_value(scenario)
    if case["operation"] == "annotation_emphasis":
        value |= {"text_unit": ["t:ja:559aead08264d579", "ja", "A"]}
    projection = view(component_input(value))
    if case["operation"] == "owner_field":
        pointer = decode("PublicTextPointer", value["pointer"])
        pointer["owner"] = projection["field_annotation"][0]["owner"]
        try:
            return {"text_unit_id": TextOwners(projection).pointer(pointer)}
        except ValueError, KeyError:
            raise ValueError("public-annotation/owner") from None
    validate_annotations(projection, ("en", "ja", "zh-Hant"))
    if case["operation"] == "shared_annotation":
        return {
            "annotation_set_count": len(projection["annotation_set"]),
            "field_annotation_count": len(projection["field_annotation"]),
        }
    if case["operation"] == "unicode_ranges":
        return _unicode(value, projection)
    return {}


def _envelope(_case: dict[str, JsonValue], scenario: JsonValue) -> dict[str, JsonValue]:
    value = object_value(scenario)
    config = object_value(parse(payloads()["config"]))
    try:
        validate(
            "Config",
            config | {"format_version": object_value(value["members"])["config"]},
        )
    except SchemaError:
        raise ValueError("public-annotation/format") from None
    return {}


def _original_annotation(
    projection: dict[str, list[dict[str, JsonValue]]], pointer: dict[str, JsonValue]
) -> JsonValue:
    return next(
        (
            row["annotation_set_id"]
            for row in projection["field_annotation"]
            if all(row[key] == pointer[key] for key in ("owner", "field", "ordinal"))
        ),
        None,
    )


def _display(case: dict[str, JsonValue], scenario: JsonValue) -> dict[str, JsonValue]:
    value, parameters = object_value(scenario), object_value(case["input"])
    projection = view(value)
    validate_annotations(projection, tuple(map(string, array(value["languages"]))))
    pointer = decode("PublicTextPointer", parameters["receiver"])
    owners = TextOwners(projection)
    owner = owners.get(object_value(pointer["owner"]))
    display = display_text(
        projection, owner.row, string(pointer["field"]), string(parameters["ui_lang"])
    )
    selection = next(
        (
            object_value(row)
            for row in array(owner.row["translations"])
            if object_value(row)["translation_id"] == display.translation_id
        ),
        None,
    )
    translation = next(
        (
            row
            for row in projection["translation"]
            if row["id"] == display.translation_id
        ),
        None,
    )
    source = None if selection is None else object_value(selection["source"])
    annotations = {row["id"]: row for row in projection["annotation_set"]}
    support = object_value(value["support"])
    status = effective_support(
        {
            "shared": {"status": support["shared_status"], "reasons": []},
            "overrides": []
            if support["override_status"] is None
            else [
                {
                    "region": support["region"],
                    "support": {"status": support["override_status"], "reasons": []},
                }
            ],
            "region_blocks": [
                {"region": support["region"], "reasons": support["region_blocks"]}
            ]
            if support["region_blocks"]
            else [],
        },
        string(support["region"]),
    )
    annotation_id = None if translation is None else translation["annotation_set_id"]
    bold = (
        None
        if annotation_id is None
        else object_value(array(annotations[string(annotation_id)]["occurrences"])[0])[
            "bold"
        ]
    )
    return {
        "original_text_unit_id": owners.pointer(pointer),
        "original_annotation_set_id": _original_annotation(projection, pointer),
        "translation_text_unit_id": None
        if translation is None
        else translation["text_unit_id"],
        "translation_annotation_set_id": annotation_id,
        "comparison_text_unit_id": None if source is None else owners.pointer(source),
        "comparison_annotation_set_id": None
        if source is None
        else _original_annotation(projection, source),
        "basis": None if selection is None else selection["basis"],
        "effective_status": status["effective_status"],
        "automatic": status["automatic"],
        "grants_aligned": object_value(value["producer"])["aligned"],
        "grants_official_counterpart": selection is not None
        and selection["basis"] == "official_counterpart",
        "translated_field": None if selection is None else selection["field"],
        "translated_section_ordinals": [
            object_value(row)["ordinal"]
            for row in array(owner.row["translations"])
            if object_value(row)["field"] == "section"
        ],
        "missing_translation": display.missing_translation,
        "origin": None if translation is None else translation["origin"],
        "low_confidence": False
        if translation is None
        else translation["low_confidence"],
        "show_proofreading_notice": False
        if translation is None
        else translation["low_confidence"],
        "bold_value": bold,
        "render_bold": parameters["bold_enabled"] is True and bold is True,
        "annotation_data_unchanged": projection == view(value),
    }


def _accounting(
    _case: dict[str, JsonValue], scenario: JsonValue
) -> dict[str, JsonValue]:
    value = object_value(scenario)

    def blob(raw: JsonValue) -> Blob:
        sizes = object_value(raw)
        return Blob(
            bytes(integer(sizes["raw"])),
            bytes(integer(sizes["br"])),
            bytes(integer(sizes["gzip"])),
        )

    files = {
        string(object_value(row)["key"]): object_value(row)
        for row in array(value["files"])
    }
    payloads = {key: blob(row) for key, row in files.items()}
    text_keys = [
        key
        for key, row in files.items()
        if row["role"] in {"config", "bootstrap", "text"}
    ]
    return {
        "text_files": accounted_sizes(payloads, text_keys),
        "startup_by_region": {
            region: accounted_sizes(
                payloads, map(string, array(keys)), manifest=blob(value["manifest"])
            )
            for region, keys in object_value(value["startup"]).items()
        },
        "text_all_counted_as_alternative": "text_all" not in text_keys,
        "annotation_increment_already_in_files": "annotation_increment"
        not in text_keys,
        "capacity_acceptance": "not_measured",
    }


OPERATIONS = {
    "schema": _schema,
    "canonical_bytes": _canonical,
    "descriptor": _descriptor,
    "admission": _admission,
    "read_projection": _projection,
    "annotation_identity": _identity,
    "display": _display,
    "envelope_versions": _envelope,
    "size_accounting": _accounting,
}


def invoke(case: dict[str, JsonValue], scenario: JsonValue) -> dict[str, JsonValue]:
    return OPERATIONS.get(string(case["operation"]), _component)(case, scenario)


@pytest.mark.parametrize(
    "case",
    CASES_TO_READ,
    ids=lambda c: (
        string(c["operation"]) + ":" + string(c["group"]) + "/" + string(c["id"])
    ),
)
def test_shared_public_annotation_operation(case: dict[str, JsonValue]) -> None:
    expected = object_value(case["expected"])
    for scenario in inputs(case):
        if expected["result"] == "reject":
            reason = string(expected["reason"])
            operation = case["operation"]
            if (
                operation
                in {
                    "read_projection",
                    "unicode_ranges",
                    "owner_field",
                    "concept_references",
                    "display",
                    "envelope_versions",
                    "annotation_emphasis",
                }
                and case["group"] != "PA-01"
            ):
                with pytest.raises(ValueError, match="public-annotation/" + reason):
                    invoke(case, scenario)
            else:
                with pytest.raises((ValueError, SchemaError)):
                    invoke(case, scenario)
        else:
            actual = invoke(case, deepcopy(scenario))
            for field, value in expected.items():
                if field != "result":
                    assert actual[field] == value


PRODUCER_CASES = tuple(
    object_value(case)
    for case in array(CASES["cases"])
    if object_value(case)["operation"] == "produce"
)


def _producer_annotations(
    value: dict[str, JsonValue], declaration: dict[str, JsonValue]
) -> None:
    originals = []
    for raw in array(declaration["fields"]):
        location, annotation = array(raw)
        originals.append(
            decode("PublicTextPointer", location)
            | {"annotation_set_id": array(annotation)[0]}
        )
    translated = {
        string(array(raw)[0]): string(array(array(raw)[1])[0])
        for raw in array(declaration["translations"])
    }
    validate_completeness(view(value), originals, translated)


def _producer_selection(database: Database, value: dict[str, JsonValue]) -> None:
    dual_region(database)
    chosen = jp_source()
    flags = object_value(value["producer"])
    basis = decode(
        "FieldTranslation", object_value(array(value["field_translations"])[0])["value"]
    )["basis"]
    with database.transaction():
        if not flags["source_fresh"]:
            database.update(
                "translation",
                {"id": "translation"},
                {"source_hash": "sha256:" + "a" * 64},
            )
        if not flags["identity_confirmed"]:
            database.update("card", {"id": "card"}, {"identity_state": "provisional"})
        if not flags["source_adopted"]:
            database.delete("face_current", {"face_id": "face", "region": "jp"})
        if basis == "official_counterpart":
            database.update(
                "region_divergence",
                {"card_id": "card", "region": "en", "field_scope": "name"},
                {"resolved": not bool(flags["divergent"])},
            )
            database.delete(
                "translation_selection",
                {"context_id": "context", "target_lang": "zh-Hant"},
            )
            database.update(
                "translation",
                {"id": "translation"},
                {
                    "origin": "official",
                    "authority": "sve_official",
                    "target_lang": "en",
                    "text": "Synthetic official",
                },
            )
            chosen = replace(
                decisions(),
                display_checks=(
                    DisplayCheck(
                        "use",
                        ("face_revision", "revision-en"),
                        TEXT,
                        EN_TEXT if flags["counterpart_fresh"] else "t:stale",
                        True,
                    ),
                ),
                display_bindings=(
                    DisplayBinding(
                        "use",
                        ("face_revision", "revision"),
                        "en",
                        "official_counterpart",
                        "translation",
                    ),
                ),
            )
    if not flags["source_exists"]:
        chosen = replace(
            chosen,
            display_bindings=(
                replace(chosen.display_bindings[0], source_use_id="missing"),
            ),
        )
    try:
        result = dual_project(database, chosen)
    except ValueError, KeyError:
        raise ValueError("public-annotation/source_invalid") from None
    owner = "revision" if basis == "official_counterpart" else "revision-en"
    receiver = next(row for row in result.tables["face_revision"] if row["id"] == owner)
    if not any(
        object_value(row)["basis"] == basis for row in array(receiver["translations"])
    ):
        raise ValueError("public-annotation/source_invalid")


@pytest.mark.parametrize(
    "case",
    PRODUCER_CASES,
    ids=lambda case: string(case["group"]) + "/" + string(case["id"]),
)
def test_shared_public_producer_operation(
    case: dict[str, JsonValue], db: Database
) -> None:
    parameters = object_value(case["input"])
    for scenario in inputs(case):
        verify = (
            partial(
                _producer_annotations,
                object_value(scenario),
                object_value(parameters["producer_annotations"]),
            )
            if "producer_annotations" in parameters
            else partial(_producer_selection, db, object_value(scenario))
        )
        with pytest.raises(
            ValueError,
            match="public-annotation/"
            + string(object_value(case["expected"])["reason"]),
        ):
            verify()

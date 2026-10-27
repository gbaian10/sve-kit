"""Section repairs stay atomic with their pinned main-text correction."""

import json
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.products import load_products
from sve_carddb.domains.registry.corrections import SectionComparison, correction_status
from sve_carddb.domains.registry.records import SectionCorrection
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.source_corrections.importer import verify_corrections
from sve_carddb.domains.text_observations import plan_text_observations
from sve_carddb.domains.text_observations.importer import revision_id
from sve_carddb.domains.translations.corrected_sources import Corrections

from ...support.registry_snapshot_fixtures import edit_record
from ...support.source_correction_fixtures import make_correction_case
from ..registry.test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared synthetic fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.review import Inputs
    from sve_carddb.domains.registry.storage import Entry

    from ...support.text_observation_fixtures import Case


def section_case(
    tmp_path: Path,
    inputs: Inputs,
    *,
    mismatch: bool = False,
    corrected: tuple[str, ...] = ("Synthetic token.",),
) -> Case:
    inputs.jp["BP02-071"].faces[0].sections = ["Synthetic token.\nDuplicate rule."]
    fixture = make_correction_case(tmp_path, inputs)
    case = fixture.texts

    def edit(entry: Entry) -> None:
        entry.data["sections"] = {
            "expected_raw_value": [
                "Wrong section." if mismatch else "Synthetic token.\nDuplicate rule."
            ],
            "corrected_value": list(corrected),
        }

    edit_record(case.root, "source_correction", edit)
    case.identity = replace(case.identity, snapshot=load_registry(case.root))
    case.catalog = load_products(case.root, registry=case.identity.snapshot)
    case.plan = plan_text_observations(
        case.identity, case.provider, images=fixture.images
    )
    return case


@pytest.mark.parametrize("corrected", [("Synthetic token.",), ()])
def test_section_correction_preserves_raw_and_imports_only_the_verified_result(
    tmp_path: Path,
    inputs: Inputs,
    corrected: tuple[str, ...],
) -> None:
    case = section_case(tmp_path, inputs, corrected=corrected)
    assert case.plan.corrections is not None
    application = case.plan.corrections[0]
    assert application.status == "applied"
    raw = application.observation
    candidate = next(
        item for item in case.plan.candidates() if item.printing_id == raw.printing_id
    )
    assert raw.content.sections == ("Synthetic token.\nDuplicate rule.",)
    assert candidate.content.sections == corrected
    assert candidate.content.effect == "Rule. (Reminder.)"
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
        with db.transaction():
            case.compose(case.plan.identity).populate(
                db,
                build=case.context(),
                vocabulary=case.vocabulary,
                published=(),
            )
        units = {row.values["id"]: row.values["text"] for row in db.rows("text_unit")}
        sections = {
            row.values["revision_id"]: units[row.values["text_unit_id"]]
            for row in db.rows("face_text_section")
        }
        assert sections[revision_id(raw)] == "Synthetic token.\nDuplicate rule."
        if corrected:
            assert sections[revision_id(candidate)] == corrected[0]
        else:
            assert revision_id(candidate) not in sections
        verify_corrections(db, case.plan, case.vocabulary)
        if corrected:
            section = next(
                row.values
                for row in db.rows("face_text_section")
                if row.values["revision_id"] == revision_id(candidate)
            )
            original = next(
                row.values
                for row in db.rows("face_text_section")
                if row.values["revision_id"] == revision_id(raw)
            )
            with db.transaction():
                db.update(
                    "face_text_section",
                    {
                        "revision_id": section["revision_id"],
                        "ordinal": section["ordinal"],
                    },
                    {"text_unit_id": original["text_unit_id"]},
                )
                with pytest.raises(ValueError, match="sections differ"):
                    verify_corrections(db, case.plan, case.vocabulary)


def test_section_raw_mismatch_blocks_the_entire_correction(
    tmp_path: Path,
    inputs: Inputs,
) -> None:
    case = section_case(tmp_path, inputs, mismatch=True)
    assert case.plan.corrections is not None
    application = case.plan.corrections[0]
    assert application.status == "conflict"
    candidate = next(
        item
        for item in case.plan.candidates()
        if item.printing_id == application.data.printing_id
    )
    assert candidate == application.observation
    assert (
        "source_correction_pending"
        in next(group for group in case.plan.groups if group.region == "jp").reasons
    )


@pytest.mark.parametrize(
    ("raw_text", "raw_sections", "expected"),
    [
        ("Old rule.", ("Old section.",), "applied"),
        ("New rule.", (), "already_fixed"),
        ("New rule.", ("Old section.",), "conflict"),
        ("Old rule.", (), "conflict"),
        ("Old rule.", ("Different section.",), "conflict"),
        ("Old rule.", ("Old section.", "Extra section."), "conflict"),
    ],
)
def test_main_text_and_complete_section_list_share_one_status(
    raw_text: str, raw_sections: tuple[str, ...], expected: str
) -> None:
    assert (
        correction_status(
            raw_text,
            "source",
            expected="Old rule.",
            corrected="New rule.",
            expected_hash="source",
            sections=SectionComparison(raw_sections, ("Old section.",), ()),
        )
        == expected
    )


@pytest.mark.parametrize(
    "value", ["not an array", [None], [1], [{"text": "Unexpected object"}]]
)
def test_section_value_schema_rejects_non_text_arrays(value: object) -> None:
    with pytest.raises(ValidationError):
        SectionCorrection.model_validate_json(
            json.dumps({"expected_raw_value": value, "corrected_value": []})
        )


def test_corrected_translation_projection_clears_sections_and_checks_their_raw_value(
    tmp_path: Path, inputs: Inputs
) -> None:
    case = section_case(tmp_path, inputs, corrected=())
    assert case.plan.corrections is not None
    application = case.plan.corrections[0]
    source = application.observation.card.source
    assert source.archive is not None
    proof = Corrections(case.plan)
    original: dict[str, JsonValue] = {
        "faces": [
            {
                "text": application.data.expected_raw_value,
                "sections": ["Synthetic token.\nDuplicate rule."],
            }
        ]
    }
    projected = proof.project(source.archive.batch_id, source.id, original)
    assert projected == {
        "faces": [{"text": application.data.corrected_value, "sections": []}]
    }
    assert original == {
        "faces": [
            {
                "text": application.data.expected_raw_value,
                "sections": ["Synthetic token.\nDuplicate rule."],
            }
        ]
    }
    with pytest.raises(ValueError, match="original sections"):
        proof.project(
            source.archive.batch_id,
            source.id,
            {"faces": [{"text": application.data.expected_raw_value, "sections": []}]},
        )


@pytest.mark.parametrize("unchanged", ["effect", "sections"])
def test_correction_marks_only_the_changed_text_field(
    tmp_path: Path, inputs: Inputs, unchanged: str
) -> None:
    case = section_case(tmp_path, inputs)

    def edit(entry: Entry) -> None:
        if unchanged == "effect":
            entry.data["corrected_value"] = entry.data["expected_raw_value"]
        else:
            section = entry.data["sections"]
            assert isinstance(section, dict)
            section["corrected_value"] = section["expected_raw_value"]

    edit_record(case.root, "source_correction", edit)
    case.identity = replace(case.identity, snapshot=load_registry(case.root))
    case.catalog = load_products(case.root, registry=case.identity.snapshot)
    case.plan = plan_text_observations(case.identity, case.provider, images=case.images)
    assert case.plan.corrections is not None
    application = case.plan.corrections[0]
    assert application.status == "applied"
    assert (application.marker(None) is None) == (unchanged == "effect")

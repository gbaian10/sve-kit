"""Unresolved drafts remain editable data without becoming renderable translations."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.template_translations.current import (
    from_files,
    shard,
    validate_foreign,
    validate_templates,
)
from sve_carddb.template_translations.current_build import populate
from sve_carddb.template_translations.current_models import (
    Candidate,
    CandidateRecord,
    DefinitionRecord,
)
from sve_carddb.template_translations.current_render import render
from sve_carddb.template_translations.current_write import compose
from sve_carddb.translations.loader import load_glossary

from .adoption_fixtures import commit
from .build_db_fixtures import seed
from .test_template_current import Case, current_case

if TYPE_CHECKING:
    from sve_carddb.contracts.template_parameters import Role

__all__ = ("current_case",)


ABSENT = "sha256:" + "0" * 64


def pending(normalized_hash: str = ABSENT, role: Role = "body") -> CandidateRecord:
    return CandidateRecord(
        kind="template_translation_candidate",
        data=Candidate(
            source_kind="effect",
            candidate_id="old-draft",
            lang="zh-Hant",
            text="N 與『X』 {{未綁定",
            normalized_hash=normalized_hash,
            role=role,
            reasons=("draft_anonymous_slot_ambiguous",),
        ),
        origin="machine",
        low_confidence=False,
        note="",
    )


def wire(record: CandidateRecord) -> dict[str, JsonValue]:
    return {
        "translation_authored_format": 2,
        "kind": "translation_shard",
        "records": [record.model_dump(mode="json", round_trip=True)],
    }


def test_candidate_preserves_unparsed_final_draft_and_has_no_definition_fk() -> None:
    record = pending()
    assert shard(canonical(wire(record))).records == (record,)
    assert record.data.text == "N 與『X』 {{未綁定"
    assert (record.data.normalized_hash, record.data.role) == (ABSENT, "body")
    assert record.origin == "machine"
    assert not record.low_confidence
    assert record.record_key == (
        '["template_translation_candidate","effect","old-draft","zh-Hant"]'
    )


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("text", ""),
        ("source_kind", "body"),
        ("candidate_id", ""),
        ("normalized_hash", "inv:absent"),
        ("role", "flavor"),
        ("reasons", []),
        ("reasons", ["b", "a"]),
        ("reasons", ["a", "a"]),
        ("reasons", ["待審"]),
        ("reasons", ["reason\n"]),
        ("reasons", ["Uppercase"]),
        ("reasons", ["reason-with-hyphen"]),
        ("source_text", "Not a permitted source copy"),
        ("definition_id", "T0000000000"),
    ],
)
def test_candidate_closed_data_refuses_invalid_values(
    field: str,
    value: JsonValue,
) -> None:
    content = wire(pending())
    record = object_value(array(content["records"])[0])
    object_value(record["data"])[field] = value
    with pytest.raises(ValueError, match=r"^Invalid current template shard$"):
        shard(canonical(content))


def test_candidate_refuses_official_origin_and_surrogate_utf8() -> None:
    content = wire(pending())
    record = object_value(array(content["records"])[0])
    record["origin"] = "official"
    with pytest.raises(ValueError, match=r"^Invalid current template shard$"):
        shard(canonical(content))
    content = wire(pending())
    raw = canonical(content).replace(b"N ", b"\\ud800 ", 1)
    with pytest.raises(ValueError, match=r"^Invalid current template shard$"):
        shard(raw)


def test_candidate_duplicate_key_never_selects_by_file_order() -> None:
    content = wire(pending())
    content["records"] = array(content["records"]) * 2
    with pytest.raises(
        ValueError, match=r"^Current template selection keys must be unique and exact$"
    ):
        shard(canonical(content))


def test_candidate_dedicated_path_is_checked_by_both_readers(
    current_case: Case,
) -> None:
    record = pending()
    raw = canonical(wire(record))
    validate_foreign(
        "translations/templates/template_translation_candidate/001.yaml", raw
    )
    with pytest.raises(
        ValueError,
        match=r"^Current template candidates require their dedicated shard area$",
    ):
        validate_foreign("translations/templates/template_translation/001.yaml", raw)
    mixed = wire(record)
    mixed["records"] = [
        record.model_dump(mode="json", round_trip=True),
        current_case.inputs.records[0].model_dump(mode="json", round_trip=True),
    ]
    with pytest.raises(
        ValueError,
        match=r"^Current template candidates require their dedicated shard area$",
    ):
        validate_foreign(
            "translations/templates/template_translation_candidate/001.yaml",
            canonical(mixed),
        )


def test_candidate_never_renders_projects_or_bypasses_source_checks(
    current_case: Case,
) -> None:
    member = current_case.generated.entries[0]
    record = pending(member.candidate.template_normalized_hash, member.entry.role)
    definitions = tuple(
        r for r in current_case.inputs.records if isinstance(r, DefinitionRecord)
    )
    package = compose(current_case.inputs.files, (*definitions, record))
    inputs = from_files(package)
    assert not inputs.translations()
    verified = validate_templates(inputs, current_case.sources, current_case.batches)
    assert verified.missing_translations == (definitions[0].data.id,)
    assert record.record_key not in verified.low_confidence
    assert verified.target_records == {}
    assert render(
        verified, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", {}
    ).issues == ("missing_template_translation",)
    with pytest.raises(
        ValueError, match=r"^Template pin references a missing current variant$"
    ):
        render(
            verified,
            member.entry.source_ref,
            "ctx:test",
            member.field_text,
            "zh-Hant",
            {},
            variants=((definitions[0].data.id, record.data.candidate_id),),
        )
    with create_database(compile_build(("translation_templates",))) as db:
        seed(db)
        with db.transaction():
            populate(db, verified)
        assert len(db.select("sentence_template", ("id",))) == 1
        assert db.select("template_translation", ("template_id",)) == ()
        assert db.select("translation_binding", ("binding_id",)) == ()
    directory = current_case.repository / "authored"
    for existing_path in (directory / "translations").rglob("*.yaml"):
        existing_path.unlink()
    for path, exact, _ in package.content:
        destination = directory / path
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_bytes(exact)
    commit(current_case.repository)
    assert load_glossary(directory).current_records() == ()
    absent = from_files(compose(current_case.inputs.files, (*definitions, pending())))
    with pytest.raises(
        ValueError,
        match=r"^Current template candidate pattern has no current source position$",
    ):
        validate_templates(absent, current_case.sources, current_case.batches)

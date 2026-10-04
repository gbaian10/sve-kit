"""Unresolved drafts remain editable data without becoming renderable translations."""

from dataclasses import replace

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.snapshot.values import array, canonical, object_value
from sve_carddb.template_translations.current import (
    from_files,
    shard,
    validate_foreign,
    validate_templates,
)
from sve_carddb.template_translations.current_build import populate
from sve_carddb.template_translations.current_models import (
    CandidateRecord,
    DefinitionRecord,
)
from sve_carddb.template_translations.current_render import render
from sve_carddb.template_translations.current_write import compose
from sve_carddb.template_translations.migration_definitions import Definitions
from sve_carddb.template_translations.migration_report import processing_list
from sve_carddb.template_translations.migration_targets import (
    Pending,
    Targets,
    candidates,
)
from sve_carddb.translations.loader import load_glossary

from .adoption_fixtures import commit
from .build_db_fixtures import seed
from .test_template_current import Case, current_case

__all__ = ("current_case",)


def pending(entries: tuple[str, ...] = ()) -> Pending:
    return Pending(
        "effect",
        "old-draft",
        "N 與『X』 {{未綁定",
        False,
        ("draft_anonymous_slot_ambiguous",),
        entries,
    )


def wire(record: CandidateRecord) -> dict[str, JsonValue]:
    return {
        "translation_authored_format": 2,
        "kind": "translation_shard",
        "records": [record.model_dump(mode="json")],
    }


def test_candidate_preserves_unparsed_final_draft_and_has_no_definition_fk() -> None:
    original = pending()
    record = candidates((original,))[0]
    assert shard(canonical(wire(record))).records == (record,)
    assert record.data.text == original.text
    assert record.data.inventory_ids == ()
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
        ("inventory_ids", ["b", "a"]),
        ("inventory_ids", ["a", "a"]),
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
    content = wire(candidates((pending(),))[0])
    record = object_value(array(content["records"])[0])
    object_value(record["data"])[field] = value
    with pytest.raises(ValueError, match=r"^Invalid current template shard$"):
        shard(canonical(content))


def test_candidate_refuses_official_origin_and_surrogate_utf8() -> None:
    content = wire(candidates((pending(),))[0])
    record = object_value(array(content["records"])[0])
    record["origin"] = "official"
    with pytest.raises(ValueError, match=r"^Invalid current template shard$"):
        shard(canonical(content))
    content = wire(candidates((pending(),))[0])
    raw = canonical(content).replace(b"N ", b"\\ud800 ", 1)
    with pytest.raises(ValueError, match=r"^Invalid current template shard$"):
        shard(raw)


def test_candidate_duplicate_key_never_selects_by_file_order() -> None:
    original = pending()
    assert len(candidates((original, original))) == 1
    with pytest.raises(
        ValueError, match=r"^Migration candidate keys contain conflicting final drafts$"
    ):
        candidates((original, replace(original, text="另一個原稿")))
    content = wire(candidates((original,))[0])
    content["records"] = array(content["records"]) * 2
    with pytest.raises(
        ValueError, match=r"^Current template selection keys must be unique and exact$"
    ):
        shard(canonical(content))


def test_candidate_dedicated_path_is_checked_by_both_readers(
    current_case: Case,
) -> None:
    record = candidates((pending(),))[0]
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
    mixed["records"] = sorted(
        [
            record.model_dump(mode="json"),
            current_case.inputs.records[0].model_dump(mode="json"),
        ],
        key=lambda row: str(object_value(row)["record_key"]),
    )
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
    record = candidates((pending((member.entry.id,)),))[0]
    definitions = tuple(
        r for r in current_case.inputs.records if isinstance(r, DefinitionRecord)
    )
    inventory = current_case.inputs.inventories[0]
    package = compose(
        current_case.inputs.files,
        (*definitions, record),
        inventory.source_batches,
        inventory.entries,
    )
    inputs = from_files(package)
    assert not inputs.translations()
    verified = validate_templates(inputs, current_case.sources)
    assert verified.missing_translations == (definitions[0].data.id,)
    assert record.record_key not in verified.low_confidence
    assert verified.target_records == {}
    assert render(
        verified, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", ()
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
            (),
            variants=((definitions[0].data.id, record.data.candidate_id),),
        )
    with create_database(compile_current_build(("translation_templates",))) as db:
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
    (directory / "translations/index.yaml").write_bytes(package.index)
    commit(current_case.repository)
    assert load_glossary(directory).current_records() == ()
    bad = inventory.entries[0].model_copy(
        update={"normalized_hash": "sha256:" + "0" * 64}
    )
    with pytest.raises(
        ValueError,
        match=r"^Current template inventory differs from its regenerated source$",
    ):
        validate_templates(
            replace(
                inputs, inventories=(inventory.model_copy(update={"entries": (bad,)}),)
            ),
            current_case.sources,
        )
    with pytest.raises(
        ValueError,
        match=r"^Current template candidate references an absent inventory entry$",
    ):
        from_files(
            compose(
                current_case.inputs.files,
                candidates((pending(("inv:absent",)),)),
                inventory.source_batches,
                inventory.entries,
            )
        )


def test_missing_candidate_source_counts_as_unknown_not_zero() -> None:
    original = pending()
    rows = processing_list(
        Definitions((), (), ()),
        (),
        (Targets((), (original,), ()),),
        (),
        (),
    )
    assert len(rows) == 1
    assert rows[0]["data_id"] == original.identifier
    assert rows[0]["affected_cards"] is None

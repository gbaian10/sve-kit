"""Whole-field rendering and projection of editable current translations."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.core.json import canonical, digest
from sve_carddb.template_translations.current import validate_templates
from sve_carddb.template_translations.current_build import apply, labels
from sve_carddb.template_translations.current_models import (
    DefinitionRecord,
    Variant,
    VariantRecord,
)
from sve_carddb.template_translations.current_render import render
from sve_carddb.translations.direct import write
from sve_carddb.translations.loader import load_glossary

from .build_db_fixtures import seed
from .test_template_current import Case, current_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.template_translations.current import Validated

__all__ = ("current_case",)


@pytest.fixture
def verified(current_case: Case) -> Validated:
    return validate_templates(
        current_case.inputs, current_case.sources, current_case.batches
    )


def test_low_confidence_renders_numeric_and_updates_without_changing_source(
    verified: Validated,
) -> None:
    member = verified.members[0]
    result = render(
        verified, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", {}
    )
    assert result.rendered is not None
    assert result.rendered.text == "Synthetic 2"
    assert result.rendered.low_confidence
    assert result.rendered.origin == "machine"
    old_id = result.rendered.identity()
    changed = tuple(
        r.model_copy(update={"note": "另一說明"}) for r in verified.inputs.records
    )
    other = render(
        replace(verified, inputs=replace(verified.inputs, records=changed)),
        member.entry.source_ref,
        "ctx:test",
        member.field_text,
        "zh-Hant",
        {},
    )
    assert other.rendered is not None
    assert other.rendered.identity() == old_id
    target = verified.inputs.translations()[0]
    changed_target = target.model_copy(
        update={
            "data": target.data.model_copy(update={"text": "New " + target.data.text})
        }
    )
    changed = tuple(
        changed_target if r == target else r for r in verified.inputs.records
    )
    other = render(
        replace(verified, inputs=replace(verified.inputs, records=changed)),
        member.entry.source_ref,
        "ctx:test",
        member.field_text,
        "zh-Hant",
        {},
    )
    assert other.rendered is not None
    assert other.rendered.identity() != old_id
    assert other.rendered.source_hash == result.rendered.source_hash


def test_missing_or_unresolved_piece_falls_back_whole_field(
    verified: Validated,
) -> None:
    member = verified.members[0]
    definitions = tuple(
        r for r in verified.inputs.records if isinstance(r, DefinitionRecord)
    )
    missing = replace(verified, inputs=replace(verified.inputs, records=definitions))
    assert render(
        missing, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", {}
    ).issues == ("missing_template_translation",)
    assert render(
        replace(verified, matches=()),
        member.entry.source_ref,
        "ctx:test",
        member.field_text,
        "zh-Hant",
        {},
    ).issues == ("unmatched_template_source",)
    with pytest.raises(
        ValueError, match=r"^Template field source hash differs from its context$"
    ):
        render(
            verified, member.entry.source_ref, "ctx:test", "different", "zh-Hant", {}
        )


def test_variant_is_never_automatically_selected_and_pin_must_exist(
    verified: Validated,
) -> None:
    member = verified.members[0]
    target = verified.inputs.translations()[0]
    variant = VariantRecord(
        kind="template_translation_variant",
        data=Variant(
            template_id=target.data.template_id,
            lang="zh-Hant",
            variant_key="test",
            text="Alternative " + target.data.text,
        ),
        origin="project",
        low_confidence=False,
        note="",
    )
    updated = replace(
        verified,
        inputs=replace(verified.inputs, records=(*verified.inputs.records, variant)),
    )
    result = render(
        updated, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", {}
    )
    assert result.rendered is not None
    assert result.rendered.text == "Synthetic 2"
    result = render(
        updated,
        member.entry.source_ref,
        "ctx:test",
        member.field_text,
        "zh-Hant",
        {},
        variants=((target.data.template_id, "test"),),
    )
    assert result.rendered is not None
    assert result.rendered.text == "Alternative Synthetic 2"
    with pytest.raises(
        ValueError, match=r"^Template pin references a missing current variant$"
    ):
        render(
            updated,
            member.entry.source_ref,
            "ctx:test",
            member.field_text,
            "zh-Hant",
            {},
            variants=((target.data.template_id, "absent"),),
        )


def test_shared_glossary_reader_accepts_current_templates(current_case: Case) -> None:
    snapshot = load_glossary(current_case.repository / "authored")
    assert [path for path, _, _ in snapshot.closure] == [
        "translations/templates/current/001.yaml"
    ]
    assert not snapshot.current_records()


def seeded(db: Database, text: str) -> None:
    seed(db)
    with db.transaction():
        db.update(
            "text_unit",
            {"id": "text"},
            {"text": text, "content_hash": digest(text.encode())},
        )


def test_apply_renders_by_source_hash_and_counts_every_field(
    verified: Validated,
) -> None:
    member = verified.members[0]
    schema = compile_build(("translation_templates",))
    with create_database(schema) as db:
        seeded(db, member.field_text)
        with db.transaction():
            report = apply(db, verified, "zh-Hant")
        # The seeded unit is also both owners' section 0, which no template covers.
        assert report.payload() == {
            "fields": 3,
            "translated": 1,
            "original": 2,
            "low_confidence": 1,
            "fallback_reasons": {"unmatched_template_source": 2},
            "pending_parameter_causes": {},
        }
        use = db.rows("translation_use")[0].values
        assert (use["face_revision_id"], use["field"], use["ordinal"]) == (
            "revision",
            "effect",
            None,
        )
        row = db.rows("translation")[0].values
        assert (row["text"], row["origin"], row["low_confidence"]) == (
            "Synthetic 2",
            "machine",
            True,
        )
        assert "decision_id" not in db.columns("sentence_template")
        binding = db.rows("text_template_binding")[0].values
        assert binding["params"] == Json({"slot_0": 2})


def test_apply_keeps_unconfirmed_and_already_translated_sources_original(
    verified: Validated,
) -> None:
    member = verified.members[0]
    schema = compile_build(("translation_templates",))
    with create_database(schema) as db:
        seeded(db, member.field_text)
        with db.transaction():
            db.update("card", {"id": "card"}, {"identity_state": "provisional"})
            assert apply(db, verified, "zh-Hant").reasons == {"unconfirmed_identity": 3}
        assert db.rows("translation") == ()
    with create_database(schema) as db:
        seeded(db, member.field_text)
        with db.transaction():
            write(
                db,
                {"face_revision_id": "revision"},
                field="name",
                lang="zh-Hant",
                source_unit_id="text",
                text="名稱",
                origin="machine",
                low_confidence=False,
            )
            report = apply(db, verified, "zh-Hant")
        assert report.translated == 0
        assert report.reasons["shared_source_translated"] == 1
        assert [r.values["text"] for r in db.rows("translation")] == ["名稱"]


def test_apply_keeps_changed_source_revision_original(verified: Validated) -> None:
    schema = compile_build(("translation_templates",))
    with create_database(schema) as db:
        seeded(db, verified.members[0].field_text + "変更")
        with db.transaction():
            report = apply(db, verified, "zh-Hant")
        assert report.translated == 0
        assert report.reasons == {"unmatched_template_source": 3}
        assert db.rows("translation_use") == ()
        assert db.rows("translation_selection") == ()


def test_labels_read_selected_vocabulary_and_name_translations(
    verified: Validated,
) -> None:
    schema = compile_build(("translation_templates",))
    with create_database(schema) as db:
        seeded(db, verified.members[0].field_text)
        with db.transaction():
            db.insert(
                "text_unit",
                {
                    "id": "name",
                    "lang": "ja",
                    "text": "合成カード",
                    "content_hash": digest("合成カード".encode()),
                },
            )
            db.update("face_revision", {"id": "revision"}, {"name_unit_id": "name"})
            write(
                db,
                {"face_revision_id": "revision"},
                field="name",
                lang="zh-Hant",
                source_unit_id="name",
                text="合成卡",
                origin="machine",
                low_confidence=True,
            )
            vocabulary = db.rows("vocabulary")[0].values
            write(
                db,
                {
                    "vocabulary_kind": str(vocabulary["kind"]),
                    "vocabulary_code": str(vocabulary["code"]),
                },
                field="label",
                lang="zh-Hant",
                source_unit_id=str(vocabulary["label_unit_id"]),
                text="詞彙",
                origin="project",
                low_confidence=False,
            )
        found = labels(db, "zh-Hant")
    name = found["card_name", "合成カード", "zh-Hant"]
    assert (name.text, name.origin, name.low_confidence) == ("合成卡", "machine", True)
    key = str(vocabulary["kind"]) + ":" + str(vocabulary["code"])
    assert found["vocabulary", key, "zh-Hant"].text == "詞彙"


def test_render_keeps_layout_and_appends_anchored_reminder_once(  # ruff: ignore[too-many-locals] -- one sealed source field exercises the actual partition and renderer together
    current_case: Case, tmp_path: Path
) -> None:
    from sve_carddb.catalog.adoption_models import Batch  # ruff: ignore[import-outside-top-level] -- minimal source fixture
    from sve_carddb.manifest import Kind  # ruff: ignore[import-outside-top-level] -- minimal source fixture
    from sve_carddb.source_archive import seal_batch  # ruff: ignore[import-outside-top-level] -- minimal source fixture
    from sve_carddb.sources.official_jp import card_url  # ruff: ignore[import-outside-top-level] -- synthetic source URL
    from sve_carddb.template_parameters.references import References  # ruff: ignore[import-outside-top-level] -- no synthetic terms
    from sve_carddb.template_translations.current import read_templates  # ruff: ignore[import-outside-top-level] -- current tree boundary
    from sve_carddb.template_translations.current_models import (  # ruff: ignore[import-outside-top-level] -- finite current data
        Translation,
        TranslationRecord,
    )
    from sve_carddb.template_translations.current_sources import Sources  # ruff: ignore[import-outside-top-level] -- one current source scan

    from .adoption_fixtures import commit  # ruff: ignore[import-outside-top-level] -- isolated author fixture
    from .test_effect_presence import page  # ruff: ignore[import-outside-top-level] -- synthetic page
    from .test_source_archive import _put, _resource, _store  # ruff: ignore[import-outside-top-level] -- synthetic sealed source
    from .test_template_current import _current_definition, _write  # ruff: ignore[import-outside-top-level] -- shared synthetic definitions and indexed fixture

    store = _store(tmp_path / "more-sources")
    text = "甲2枚（乙）<br>甲3枚"
    raw = page("jp", '<div class="detail">' + text + "</div>")
    _put(store, _resource(card_url("SYN-02"), "raw/more.html", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    batch = Batch(batch_id=sealed.batch_id)
    sources = Sources(
        {store.store_id: store.root}, References(), current_case.sources.rules
    )
    generated = sources.generate((batch,))
    definitions = tuple(
        {r.data.id: r for r in map(_current_definition, generated.entries)}.values()
    )
    targets: list[TranslationRecord] = []
    for definition in definitions:
        role = definition.data.role
        text = "BODY " if role == "body" else "REMINDER" if role == "reminder" else ""
        text += "".join(
            "{{" + slot.name + "}}" for slot in definition.data.parameter_schema.slots
        )
        targets.append(
            TranslationRecord(
                kind="template_translation",
                data=Translation(
                    template_id=definition.data.id, lang="zh-Hant", text=text
                ),
                origin="machine",
                low_confidence=False,
                note="",
            )
        )
    records: list[DefinitionRecord | TranslationRecord] = sorted(
        [*definitions, *targets], key=lambda r: r.record_key
    )
    _write(
        current_case.repository,
        {
            "translations/templates/current/001.yaml": {
                "translation_authored_format": 2,
                "kind": "translation_shard",
                "records": [
                    r.model_dump(mode="json", round_trip=True) for r in records
                ],
            },
        },
    )
    revision = commit(current_case.repository)
    inputs = read_templates(current_case.repository, revision)
    validated = validate_templates(inputs, sources, (batch,))
    member = generated.entries[0]
    result = render(
        validated, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", {}
    )
    assert result.rendered is not None
    assert result.rendered.text == "BODY 2REMINDER\nBODY 3"
    assert len(result.rendered.bindings) == 4


def test_reference_interpolation_tracks_exact_positions_and_requires_target_label(
    verified: Validated,
) -> None:
    from sve_carddb.contracts.template_parameters import Schema  # ruff: ignore[import-outside-top-level] -- synthetic typed reference schema
    from sve_carddb.template_translations.current_render import (  # ruff: ignore[import-outside-top-level] -- finite interpolation unit boundary
        Binding,
        Label,
        _fragment,
    )
    from sve_carddb.template_translations.text import parse as parse_text  # ruff: ignore[import-outside-top-level] -- finite placeholder language

    definition = next(
        r for r in verified.inputs.records if isinstance(r, DefinitionRecord)
    )
    slot = definition.data.parameter_schema.slots[0].model_copy(
        update={"type": "reference", "reference_kind": "term", "min": None, "max": None}
    )
    definition = definition.model_copy(
        update={
            "data": definition.data.model_copy(
                update={"parameter_schema": Schema(slots=(slot,))}
            )
        }
    )
    member = verified.members[0]
    binding = Binding(
        "bind:test",
        0,
        definition,
        member,
        canonical({slot.name: {"kind": "term", "id": "term:synthetic"}}),
    )
    parts = parse_text("前{{slot_0}}後{{slot_0}}", definition.data.parameter_schema)
    assert _fragment(binding, parts, "zh-Hant", {}) is None
    label = Label("term", "term:synthetic", "zh-Hant", "參照", "machine", True, True)
    result = _fragment(
        binding, parts, "zh-Hant", {("term", "term:synthetic", "zh-Hant"): label}
    )
    assert result is not None
    text, positions = result
    assert text == "前參照後參照"
    assert [(p.start, p.end) for p in positions] == [(1, 3), (4, 6)]
    assert all(p.label.emphasis is True and p.label.low_confidence for p in positions)


def test_current_package_splits_yaml_and_preserves_shared_closure(
    current_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    from sve_carddb.template_translations.current import from_files  # ruff: ignore[import-outside-top-level] -- reconstructed package readback
    from sve_carddb.template_translations.current_write import compose  # ruff: ignore[import-outside-top-level] -- current packaging boundary

    monkeypatch.setattr(
        "sve_carddb.template_translations.current_write.TARGET_BYTES", 1
    )
    package = compose(current_case.inputs.files, current_case.inputs.records)
    assert from_files(package).records == current_case.inputs.records
    assert all(len(raw) < 1048576 for _, raw, _ in package.content)
    assert all(b"decisions:" not in raw for _, raw, _ in package.content)


def test_invalid_placeholder_keeps_only_that_field_original(
    verified: Validated,
) -> None:
    member = verified.members[0]
    target = verified.inputs.translations()[0]
    broken = target.model_copy(
        update={"data": target.data.model_copy(update={"text": "壞{{unknown}}"})}
    )
    records = tuple(broken if r == target else r for r in verified.inputs.records)
    result = render(
        replace(verified, inputs=replace(verified.inputs, records=records)),
        member.entry.source_ref,
        "ctx:test",
        member.field_text,
        "zh-Hant",
        {},
    )
    assert result.issues == ("invalid_template_translation",)


@pytest.mark.parametrize("doubtful", [False, True])
def test_low_confidence_recognition_rule_marks_the_rendered_field(
    current_case: Case, *, doubtful: bool
) -> None:
    from sve_carddb.template_parameters.references import References  # ruff: ignore[import-outside-top-level] -- no synthetic terms
    from sve_carddb.template_translations.current_sources import Sources  # ruff: ignore[import-outside-top-level] -- one current source scan

    rules = current_case.sources.rules
    rules = rules.model_copy(
        update={
            "rules": tuple(
                r.model_copy(update={"low_confidence": doubtful}) for r in rules.rules
            )
        }
    )
    target = current_case.inputs.translations()[0]
    certain = target.model_copy(update={"low_confidence": False})
    inputs = replace(
        current_case.inputs,
        records=tuple(
            certain if r == target else r for r in current_case.inputs.records
        ),
    )
    sources = Sources(current_case.sources.stores, References(), rules)
    found = validate_templates(inputs, sources, current_case.batches)
    member = found.members[0]
    assert member.low_confidence is doubtful
    result = render(
        found, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", {}
    )
    assert result.rendered is not None
    assert result.rendered.low_confidence is doubtful

"""Whole-field rendering and projection of editable current translations."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.snapshot.values import canonical, digest, parse
from sve_carddb.template_translations.current import validate_templates
from sve_carddb.template_translations.current_build import populate, populate_field
from sve_carddb.template_translations.current_models import (
    DefinitionRecord,
    Variant,
    VariantRecord,
)
from sve_carddb.template_translations.current_render import render
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.name_build import NameOwner

from .build_db_fixtures import seed
from .test_template_current import Case, current_case

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.template_translations.current import Validated

__all__ = ("current_case",)


@pytest.fixture
def verified(current_case: Case) -> Validated:
    return validate_templates(current_case.inputs, current_case.sources)


def test_low_confidence_renders_numeric_and_updates_without_changing_source(
    verified: Validated,
) -> None:
    member = verified.members[0]
    result = render(
        verified, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", ()
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
        (),
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
        (),
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
        missing, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", ()
    ).issues == ("missing_template_translation",)
    assert render(
        replace(verified, matches=()),
        member.entry.source_ref,
        "ctx:test",
        member.field_text,
        "zh-Hant",
        (),
    ).issues == ("unmatched_template_source",)
    with pytest.raises(
        ValueError, match=r"^Template field source hash differs from its context$"
    ):
        render(
            verified, member.entry.source_ref, "ctx:test", "different", "zh-Hant", ()
        )


def test_variant_is_never_automatically_selected_and_pin_must_exist(
    verified: Validated,
) -> None:
    member = verified.members[0]
    target = verified.inputs.translations()[0]
    variant = VariantRecord(
        record_key=canonical(
            ["template_translation_variant", target.data.template_id, "zh-Hant", "test"]
        ).decode(),
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
        updated, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", ()
    )
    assert result.rendered is not None
    assert result.rendered.text == "Synthetic 2"
    result = render(
        updated,
        member.entry.source_ref,
        "ctx:test",
        member.field_text,
        "zh-Hant",
        (),
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
            (),
            variants=((target.data.template_id, "absent"),),
        )


def test_shared_glossary_reader_accepts_current_inventory(current_case: Case) -> None:
    snapshot = load_glossary(current_case.repository / "authored")
    assert len(snapshot.closure) == 2
    assert not snapshot.current_records()


def test_current_database_has_no_fake_decision_and_checks_real_owner(
    verified: Validated,
) -> None:
    member = verified.members[0]
    schema = compile_current_build(("translation_templates",))
    with create_database(schema) as db:
        seed(db)
        with db.transaction():
            db.update(
                "text_unit",
                {"id": "text"},
                {
                    "text": member.field_text,
                    "content_hash": digest(member.field_text.encode()),
                },
            )
            populate(db, verified)
            result = populate_field(
                db,
                verified,
                member.entry.source_ref,
                NameOwner("face_revision", "revision"),
                "effect",
                "zh-Hant",
            )
            assert result.rendered is not None
            assert len(db.rows("text_template_binding")) == 1
            assert db.rows("translation")[0].values["low_confidence"] is True
            assert "decision_id" not in db.columns("sentence_template")
            assert parse(result.rendered.bindings[0].params) == {"slot_0": 2}
        with db.transaction():
            unknown = populate_field(
                db,
                verified,
                member.entry.source_ref,
                NameOwner("printing_face", "printing", "face"),
                "effect",
                "zh-Hant",
            )
            assert unknown.issues == ("unknown_owner_source",)


def test_render_keeps_layout_and_appends_anchored_reminder_once(  # ruff: ignore[too-many-locals] -- one sealed source field exercises the actual partition and renderer together
    current_case: Case, tmp_path: Path
) -> None:
    from sve_carddb.catalog.adoption_models import Batch  # ruff: ignore[import-outside-top-level] -- minimal source fixture
    from sve_carddb.catalog.adoption_sources import PinnedRepository  # ruff: ignore[import-outside-top-level] -- source-free current reader
    from sve_carddb.manifest import Kind  # ruff: ignore[import-outside-top-level] -- minimal source fixture
    from sve_carddb.source_archive import seal_batch  # ruff: ignore[import-outside-top-level] -- minimal source fixture
    from sve_carddb.sources.official_jp import card_url  # ruff: ignore[import-outside-top-level] -- synthetic source URL
    from sve_carddb.template_parameters.references import References  # ruff: ignore[import-outside-top-level] -- no synthetic terms
    from sve_carddb.template_translations.current import read_templates  # ruff: ignore[import-outside-top-level] -- current tree boundary
    from sve_carddb.template_translations.current_models import (  # ruff: ignore[import-outside-top-level] -- finite current data
        Inventory,
        Translation,
        TranslationRecord,
    )
    from sve_carddb.template_translations.current_sources import Sources  # ruff: ignore[import-outside-top-level] -- one current source scan
    from sve_carddb.template_translations.migration_definitions import derive  # ruff: ignore[import-outside-top-level] -- real semantic definitions

    from .adoption_fixtures import commit  # ruff: ignore[import-outside-top-level] -- isolated author fixture
    from .test_effect_presence import page  # ruff: ignore[import-outside-top-level] -- synthetic page
    from .test_source_archive import _put, _resource, _store  # ruff: ignore[import-outside-top-level] -- synthetic sealed source
    from .test_template_current import _write  # ruff: ignore[import-outside-top-level] -- shared indexed fixture

    store = _store(tmp_path / "more-sources")
    text = "甲2枚（乙）<br>甲3枚"
    raw = page("jp", '<div class="detail">' + text + "</div>")
    _put(store, _resource(card_url("SYN-02"), "raw/more.html", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    batch = Batch(store_id=store.store_id, batch_id=sealed.batch_id)
    sources = Sources(
        {store.store_id: store.root}, References(), current_case.sources.rules
    )
    generated = sources.generate((batch,))
    definitions = derive((), generated.entries).records
    targets: list[TranslationRecord] = []
    for definition in definitions:
        role = definition.data.source_span.role
        text = "BODY " if role == "body" else "REMINDER" if role == "reminder" else ""
        text += "".join(
            "{{" + slot.name + "}}" for slot in definition.data.parameter_schema.slots
        )
        targets.append(
            TranslationRecord(
                record_key=canonical(
                    ["template_translation", definition.data.id, "zh-Hant"]
                ).decode(),
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
    inventory = Inventory(
        template_source_format=3,
        kind="template_source_inventory",
        source_batches=(batch,),
        entries=tuple(m.entry for m in generated.entries),
    )
    _write(
        current_case.repository,
        {
            "translations/templates/current/001.yaml": {
                "translation_authored_format": 2,
                "kind": "translation_shard",
                "records": [r.model_dump(mode="json") for r in records],
            },
            "translations/template-sources/001.yaml": inventory.model_dump(mode="json"),
        },
    )
    revision = commit(current_case.repository)
    inputs = read_templates(PinnedRepository(current_case.repository), revision)
    validated = validate_templates(inputs, sources)
    member = generated.entries[0]
    result = render(
        validated, member.entry.source_ref, "ctx:test", member.field_text, "zh-Hant", ()
    )
    assert result.rendered is not None
    assert result.rendered.text == "BODY 2REMINDER\nBODY 3"
    assert len(result.rendered.bindings) == 4


def test_reference_interpolation_tracks_exact_positions_and_requires_target_label(
    verified: Validated,
) -> None:
    from sve_carddb.template_parameters.models import Schema  # ruff: ignore[import-outside-top-level] -- synthetic typed reference schema
    from sve_carddb.template_translations.current_render import (  # ruff: ignore[import-outside-top-level] -- finite interpolation unit boundary
        Binding,
        Label,
        _fragment,
    )

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
    target = verified.inputs.translations()[0]
    target = target.model_copy(
        update={
            "data": target.data.model_copy(update={"text": "前{{slot_0}}後{{slot_0}}"})
        }
    )
    assert _fragment(binding, target, "zh-Hant", ()) is None
    label = Label("term", "term:synthetic", "zh-Hant", "參照", "machine", True, True)
    result = _fragment(binding, target, "zh-Hant", (label,))
    assert result is not None
    text, positions = result
    assert text == "前參照後參照"
    assert [(p.start, p.end) for p in positions] == [(1, 3), (4, 6)]
    assert all(p.label.emphasis is True and p.label.low_confidence for p in positions)
    with pytest.raises(
        ValueError, match=r"^Current reference label selection must be unique$"
    ):
        _fragment(binding, target, "zh-Hant", (label, label))


def test_current_package_splits_yaml_and_preserves_shared_closure(
    current_case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    from sve_carddb.template_translations.current import from_files  # ruff: ignore[import-outside-top-level] -- reconstructed package readback
    from sve_carddb.template_translations.current_write import compose  # ruff: ignore[import-outside-top-level] -- current packaging boundary
    from sve_carddb.template_translations.files import json_bytes  # ruff: ignore[import-outside-top-level] -- shared canonical YAML hash
    from sve_carddb.translations.models import Index  # ruff: ignore[import-outside-top-level] -- shared index wire

    monkeypatch.setattr(
        "sve_carddb.template_translations.current_write.TARGET_BYTES", 1
    )
    inventory = current_case.inputs.inventories[0]
    package = compose(
        current_case.inputs.files,
        current_case.inputs.records,
        inventory.source_batches,
        inventory.entries,
    )
    assert from_files(package).records == current_case.inputs.records
    index = Index.model_validate_json(json_bytes(package.index))
    assert index.translation_authored_format == 2
    assert all(
        digest(content) == {**index.includes, **index.inventories}[path]
        for path, _, content in package.content
    )
    assert all(len(raw) < 1048576 for _, raw, _ in package.content)
    assert all(b"decisions:" not in raw for _, raw, _ in package.content)


def test_processing_list_keeps_unknown_fields_candidates_and_quality(
    verified: Validated,
) -> None:
    from sve_carddb.template_translations.migration_definitions import Definitions  # ruff: ignore[import-outside-top-level] -- synthetic migration input
    from sve_carddb.template_translations.migration_report import processing_list  # ruff: ignore[import-outside-top-level] -- one processing report
    from sve_carddb.template_translations.migration_targets import Pending, Targets  # ruff: ignore[import-outside-top-level] -- inactive drafts never become target records

    records = tuple(
        r for r in verified.inputs.records if isinstance(r, DefinitionRecord)
    )
    member = verified.members[0]
    pending = Pending(
        "effect", "Tsynthetic", "N", True, ("anonymous_ambiguous",), (member.entry.id,)
    )
    targets = Targets(verified.inputs.translations(), (pending,), ())
    report = canonical(
        [
            {
                "effect_coverage": {
                    "failures": [
                        {
                            "source_version_id": member.entry.source_ref.source_version_id,
                            "locator": "/faces/1/text",
                            "reason": "unknown_effect_presence",
                        }
                    ]
                },
                "flavor_fields": [
                    {
                        "source_version_id": member.entry.source_ref.source_version_id,
                        "locator": "/faces/1/flavor",
                        "state": "unknown",
                    }
                ],
            }
        ]
    )
    rows = processing_list(
        Definitions(records, (), ()),
        verified.members,
        (targets,),
        (),
        verified.matches,
        source_report=report,
    )
    reasons = {str(row["reason"]) for row in rows}
    assert reasons == {
        "anonymous_ambiguous",
        "new_template",
        "low_confidence",
        "unknown_effect_presence",
        "unknown_flavor_source",
    }
    assert all(row["affected_cards"] == 1 for row in rows)
    assert b'"text"' not in canonical(list(rows))
    assert not any(row["reason"] == "missing_translation" for row in rows)

"""Editable current templates keep source/parameter validation without history gates."""

from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.catalog.adoption_models import Batch
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.manifest import Kind
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.template_parameter_rules.current import load_file
from sve_carddb.template_parameter_rules.current import parse as parse_rules
from sve_carddb.template_parameters.models import Schema
from sve_carddb.template_parameters.references import References
from sve_carddb.template_translations.current import (
    migrate_shard,
    read_templates,
    shard,
    validate_templates,
)
from sve_carddb.template_translations.current_models import (
    DefinitionRecord,
    Inventory,
    Shard,
    Translation,
    TranslationRecord,
)
from sve_carddb.template_translations.current_sources import Generated, Sources

from .adoption_fixtures import commit, git
from .template_intake_fixtures import definition
from .template_intake_fixtures import shard as legacy_shard
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

    from sve_carddb.template_translations.current import Inputs


@dataclass(frozen=True)
class Case:
    repository: Path
    revision: str
    inputs: Inputs
    sources: Sources
    generated: Generated


@pytest.fixture
def current_case(tmp_path: Path) -> Case:
    root = tmp_path / "repository"
    root.mkdir()
    git(root, "init", "-b", "main")
    store = _store(tmp_path / "sources")
    raw = page("jp", '<div class="detail">甲2枚</div>')
    _put(store, _resource(card_url("SYN-01"), "raw/card.html", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    batch = Batch(store_id=store.store_id, batch_id=sealed.batch_id)
    rules = parse_rules(
        canonical(
            {
                "parameter_rule_format": 2,
                "kind": "template_parameter_rules",
                "rules": [
                    {
                        "rule_id": "suffix_unit_cards",
                        "enabled": True,
                        "origin": "project",
                        "low_confidence": False,
                        "note": "",
                    }
                ],
            }
        )
    )
    sources = Sources({store.store_id: store.root}, References(), rules)
    generated = sources.generate((batch,))
    records = _records(generated)
    source_inventory = Inventory(
        template_source_format=3,
        kind="template_source_inventory",
        source_batches=(batch,),
        entries=tuple(m.entry for m in generated.entries),
    )
    values: dict[str, JsonValue] = {
        "translations/templates/current/001.yaml": records.model_dump(mode="json"),
        "translations/template-sources/001.yaml": source_inventory.model_dump(
            mode="json"
        ),
    }
    _write(root, values)
    revision = commit(root)
    inputs = read_templates(PinnedRepository(root), revision)
    return Case(root, revision, inputs, sources, generated)


def _records(generated: Generated) -> Shard:
    member = next(m for m in generated.entries if m.entry.role == "body")
    definitions = migrate_shard(canonical(legacy_shard([definition(member, new=True)])))
    target = definitions.records[0]
    assert isinstance(target, DefinitionRecord)
    schema = target.data.parameter_schema
    text = "Synthetic " + " ".join("{{" + s.name + "}}" for s in schema.slots)
    translation = TranslationRecord(
        record_key=canonical(
            ["template_translation", target.data.id, "zh-Hant"]
        ).decode(),
        kind="template_translation",
        data=Translation(template_id=target.data.id, lang="zh-Hant", text=text),
        origin="machine",
        low_confidence=True,
        note="待校對。",
    )
    return Shard(
        translation_authored_format=2,
        kind="translation_shard",
        records=tuple(
            sorted((*definitions.records, translation), key=lambda r: r.record_key)
        ),
    )


def _write(root: Path, values: dict[str, JsonValue]) -> None:
    index: dict[str, JsonValue] = {
        "translation_authored_format": 1,
        "kind": "translation_index",
        "includes": {},
        "inventories": {},
    }
    includes: dict[str, JsonValue] = {
        name: digest(canonical(value))
        for name, value in values.items()
        if "template-sources/" not in name
    }
    inventories: dict[str, JsonValue] = {
        name: digest(canonical(value))
        for name, value in values.items()
        if "template-sources/" in name
    }
    index.update(includes=includes, inventories=inventories)
    for name, value in {"translations/index.yaml": index, **values}.items():
        path = root / "authored" / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(canonical(value))


def test_read_is_source_free_and_low_confidence_stays_active(
    current_case: Case, mocker: MockerFixture
) -> None:
    mocker.patch(
        "sve_carddb.template_translations.files.immutable",
        side_effect=AssertionError("No history gate"),
    )
    mocker.patch.object(
        current_case.sources, "generate", side_effect=AssertionError("No raw reads")
    )
    inputs = read_templates(
        PinnedRepository(current_case.repository), current_case.revision
    )
    assert len(inputs.translations()) == 1
    assert inputs.translations()[0].low_confidence
    assert inputs.translations()[0].origin == "machine"
    assert not hasattr(inputs, "source_report")


def test_new_commit_can_edit_text_and_note_without_receipt(current_case: Case) -> None:
    path = current_case.repository / "authored/translations/templates/current/001.yaml"
    records = list(
        shard(
            canonical(
                {
                    "translation_authored_format": 2,
                    "kind": "translation_shard",
                    "records": [
                        r.model_dump(mode="json") for r in current_case.inputs.records
                    ],
                }
            )
        ).records
    )
    target = next(
        i for i, record in enumerate(records) if isinstance(record, TranslationRecord)
    )
    old = records[target]
    assert isinstance(old, TranslationRecord)
    records[target] = old.model_copy(
        update={
            "note": "修正譯字。",
            "data": old.data.model_copy(update={"text": "Changed " + old.data.text}),
        }
    )
    value = Shard(
        translation_authored_format=2, kind="translation_shard", records=tuple(records)
    )
    path.write_bytes(canonical(value.model_dump(mode="json")))
    index_path = current_case.repository / "authored/translations/index.yaml"
    index = object_value(parse(index_path.read_bytes()))
    object_value(index["includes"])["translations/templates/current/001.yaml"] = digest(
        path.read_bytes()
    )
    index_path.write_bytes(canonical(index))
    revision = commit(current_case.repository)
    inputs = read_templates(PinnedRepository(current_case.repository), revision)
    assert inputs.translations()[0].data.text.startswith("Changed ")
    verified = validate_templates(inputs, current_case.sources)
    assert verified.frequencies
    assert current_case.sources.generated_batches == 1


def test_source_inventory_mismatch_is_refused_at_build(current_case: Case) -> None:
    first, *rest = current_case.inputs.inventories[0].entries
    bad = first.model_copy(update={"normalized_hash": digest(b"wrong")})
    inputs = replace(
        current_case.inputs,
        inventories=(
            current_case.inputs.inventories[0].model_copy(
                update={"entries": (bad, *rest)}
            ),
        ),
    )
    with pytest.raises(
        ValueError,
        match=r"^Current template inventory differs from its regenerated source$",
    ):
        validate_templates(inputs, current_case.sources)


def test_source_coverage_must_be_exact(current_case: Case) -> None:
    inputs = replace(
        current_case.inputs,
        inventories=(
            current_case.inputs.inventories[0].model_copy(update={"entries": ()}),
        ),
    )
    with pytest.raises(
        ValueError,
        match=r"^Current template inventory must cover every source batch entry$",
    ):
        validate_templates(inputs, current_case.sources)


def test_numeric_bound_is_checked_against_source_role(current_case: Case) -> None:
    records = list(current_case.inputs.records)
    source = records[0]
    assert isinstance(source, DefinitionRecord)
    schema = source.data.parameter_schema
    slot = schema.slots[0].model_copy(update={"max": 1})
    changed = source.model_copy(
        update={
            "data": source.data.model_copy(
                update={"parameter_schema": Schema(slots=(slot,))}
            )
        }
    )
    records[0] = changed
    with pytest.raises(
        ValueError, match=r"^Template numeric bounds differ from the recognized role$"
    ):
        validate_templates(
            replace(current_case.inputs, records=tuple(records)), current_case.sources
        )


def test_closed_formats_refuse_receipts_and_unknown_switches() -> None:
    with pytest.raises(ValueError, match=r"^Invalid current template shard$"):
        shard(
            canonical(
                {
                    "translation_authored_format": 2,
                    "kind": "translation_shard",
                    "records": [],
                    "decisions": [],
                }
            )
        )
    with pytest.raises(ValueError, match=r"^Invalid current parameter rules$"):
        parse_rules(
            canonical(
                {
                    "parameter_rule_format": 2,
                    "kind": "template_parameter_rules",
                    "rules": [
                        {
                            "rule_id": "unknown",
                            "enabled": True,
                            "origin": "project",
                            "low_confidence": False,
                            "note": "",
                        }
                    ],
                }
            )
        )
    rules = parse_rules(
        canonical(
            {
                "parameter_rule_format": 2,
                "kind": "template_parameter_rules",
                "rules": [],
            }
        )
    )
    assert rules.enabled() == ()


def test_local_rule_input_rejects_symlinks(tmp_path: Path) -> None:
    path = tmp_path / "current.yaml"
    path.symlink_to(tmp_path / "other.yaml")
    with pytest.raises(
        ValueError, match=r"^Missing or symlink current parameter rules$"
    ):
        load_file(path)


def test_disabled_rules_leave_source_positions_pending(current_case: Case) -> None:
    rules = current_case.sources.rules
    disabled = rules.model_copy(
        update={
            "rules": tuple(r.model_copy(update={"enabled": False}) for r in rules.rules)
        }
    )
    sources = Sources(current_case.sources.stores, References(), disabled)
    batch = current_case.inputs.inventories[0].source_batches
    result = sources.generate(batch)
    member = next(m for m in result.entries if m.entry.role == "body")
    assert member.pending == ("numeric_rule_pending_approval",)


def test_current_rule_git_mode_is_checked(current_case: Case) -> None:
    from sve_carddb.template_parameter_rules.current import PATH, load  # ruff: ignore[import-outside-top-level] -- isolate the current rule reader

    path = current_case.repository / PATH
    path.parent.mkdir(parents=True)
    path.symlink_to("../translations/index.yaml")
    revision = commit(current_case.repository)
    with pytest.raises(
        ValueError, match=r"^Missing or unsafe current parameter rules$"
    ):
        load(PinnedRepository(current_case.repository), revision)


def test_definition_migration_preserves_existing_payload_slots_and_id(
    current_case: Case,
) -> None:
    from sve_carddb.template_translations.migration_definitions import derive  # ruff: ignore[import-outside-top-level] -- migration uses the same validated source fixture

    definitions = tuple(
        record
        for record in current_case.inputs.records
        if isinstance(record, DefinitionRecord)
    )
    result = derive(definitions, current_case.generated.entries)
    assert result.records == definitions
    assert result.representatives == tuple(
        (r.data.id, r.data.inventory_id) for r in definitions
    )
    assert not result.issues


def test_definition_migration_never_promotes_pending_source(current_case: Case) -> None:
    from sve_carddb.template_translations.migration_definitions import derive  # ruff: ignore[import-outside-top-level] -- pending source is distinct from missing translation

    members = tuple(
        replace(member, pending=("missing_card_name_concept",))
        for member in current_case.generated.entries
    )
    result = derive((), members)
    assert not result.records
    assert result.issues == tuple(
        (member.entry.id, ("missing_card_name_concept",)) for member in members
    )


def test_new_definition_uses_payload_id_and_verifies_its_source(
    current_case: Case,
) -> None:
    from sve_carddb.template_translations.migration_definitions import derive  # ruff: ignore[import-outside-top-level] -- derive actual semantic definitions

    result = derive((), current_case.generated.entries)
    assert len(result.records) == 1
    definition = result.records[0]
    assert definition.data.id == "T" + definition.data.content_hash[7:23]
    assert definition.data.supersedes_id is not None
    verified = validate_templates(
        replace(current_case.inputs, records=result.records), current_case.sources
    )
    assert verified.frequencies == ((definition.data.id, 1),)
    assert verified.missing_translations == (definition.data.id,)


def test_current_vocabulary_requires_catalog_authority_and_retains_composites() -> None:
    from sve_carddb.catalog.models import Term  # ruff: ignore[import-outside-top-level] -- explicit synthetic catalog authority
    from sve_carddb.products.models import LocalizedText  # ruff: ignore[import-outside-top-level] -- synthetic labels
    from sve_carddb.template_translations.current_references import (  # ruff: ignore[import-outside-top-level] -- current reference adapter
        References as CurrentReferences,
    )
    from sve_carddb.text_observations.vocabulary import Binding, Vocabulary  # ruff: ignore[import-outside-top-level] -- synthetic permanent codes

    binding = Binding(region="jp", kind="class", raw="SyntheticClass", code="test")
    with pytest.raises(
        ValueError, match=r"^Current references require a derived active catalog$"
    ):
        CurrentReferences(vocabulary=Vocabulary(bindings=(binding,)))
    term = Term(
        kind="class",
        code="test",
        label=LocalizedText(lang="en", text="Test"),
        active=True,
    )
    refs = CurrentReferences(vocabulary=Vocabulary(bindings=(binding,), terms=(term,)))
    result = refs.proposed_vocabulary("class", "SyntheticClass")
    assert result.issues == ()
    assert result.target == {
        "kind": "vocabulary",
        "vocabulary_kind": "class",
        "vocabulary_code": "test",
        "special_kinds": [],
    }
    assert refs.proposed_vocabulary("class", "Unknown").target is None


def test_effect_conversion_preserves_final_text_and_retains_pending_source(
    current_case: Case,
) -> None:
    from sve_carddb.template_translations.migration_drafts import EffectDraft  # ruff: ignore[import-outside-top-level] -- private draft conversion
    from sve_carddb.template_translations.migration_targets import effects  # ruff: ignore[import-outside-top-level] -- conversion boundary
    from sve_carddb.template_translations.preparation import Draft  # ruff: ignore[import-outside-top-level] -- synthetic draft

    definitions = tuple(
        r for r in current_case.inputs.records if isinstance(r, DefinitionRecord)
    )
    member = current_case.generated.entries[0]
    assert member.candidate.legacy_id is not None
    value = EffectDraft(
        Draft(
            template=member.candidate.legacy_id,
            normalized=member.normalized,
            zh="自撰 N 次",
            confidence="high",
            note="",
        ),
        False,
    )
    pending = replace(
        member,
        entry=member.entry.model_copy(update={"id": "inv:unresolved"}),
        pending=("missing_card_name_concept",),
    )
    result = effects(
        definitions, (member, pending), (value,), current_case.inputs.translations(), {}
    )
    assert result.records == current_case.inputs.translations()
    assert result.dispositions == (
        ("effect", value.draft.template, "active_and_pending"),
    )
    assert result.pending[0].text == value.draft.zh
    assert result.pending[0].reasons == ("missing_card_name_concept",)


def test_effect_conversion_aligns_known_unique_marker_and_never_activates_missing_source(
    current_case: Case,
) -> None:
    from sve_carddb.template_translations.migration_drafts import EffectDraft  # ruff: ignore[import-outside-top-level] -- source/draft accounting
    from sve_carddb.template_translations.migration_targets import effects  # ruff: ignore[import-outside-top-level] -- preserve original target wording
    from sve_carddb.template_translations.preparation import Draft  # ruff: ignore[import-outside-top-level] -- synthetic original

    definitions = tuple(
        r for r in current_case.inputs.records if isinstance(r, DefinitionRecord)
    )
    member = current_case.generated.entries[0]
    assert member.candidate.legacy_id is not None
    draft = Draft(
        template=member.candidate.legacy_id,
        normalized=member.normalized,
        zh="自撰 N 次",
        confidence="low",
        note="",
    )
    result = effects(definitions, (member,), (EffectDraft(draft, True),), (), {})
    assert result.records[0].data.text == "自撰 {{slot_0}} 次"
    assert result.records[0].low_confidence
    assert result.records[0].origin == "machine"
    missing = effects((), (), (EffectDraft(draft, True),), (), {})
    assert not missing.records
    assert missing.pending[0].text == draft.zh
    assert missing.pending[0].reasons == ("draft_source_missing",)


def test_repeated_slots_merge_only_with_equal_roles_values_and_unit_rules_across_family(
    current_case: Case,
) -> None:
    from sve_carddb.template_parameters.analysis import prepared  # ruff: ignore[import-outside-top-level] -- independent synthetic source spans
    from sve_carddb.template_sources.normalizer import partition  # ruff: ignore[import-outside-top-level] -- source partition
    from sve_carddb.template_translations.migration_definitions import (  # ruff: ignore[import-outside-top-level] -- conservative schema relation
        schema as derive_schema,
    )
    from sve_carddb.template_translations.sources import Reconstructed  # ruff: ignore[import-outside-top-level] -- typed source boundary

    from .test_template_parameters import candidate  # ruff: ignore[import-outside-top-level] -- exact synthetic hint generation

    def member(text: str) -> Reconstructed:
        part = partition(text)[0]
        normalized = prepared(text, part)[0].normalized
        value = candidate(text)
        hints = tuple(h.model_copy(update={"issues": ()}) for h in value.slots)
        return Reconstructed(
            current_case.generated.entries[0].entry,
            value,
            normalized,
            text,
            hints,
            tuple("numeric" for h in hints),
            (),
        )

    first, same, different = (
        member(text) for text in ("甲2枚、乙2枚", "甲3枚、乙3枚", "甲3枚、乙4枚")
    )
    merged = derive_schema(first, (first, same))
    assert len(merged.slots) == 1
    assert len(merged.slots[0].occurrences) == 2
    separated = derive_schema(first, (first, different))
    assert len(separated.slots) == 2
    different_role = replace(same, roles=("numeric", "health_value"))
    assert len(derive_schema(first, (first, different_role)).slots) == 2

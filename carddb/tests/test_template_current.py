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
from sve_carddb.template_parameters.analysis import SAFE_INTEGER, VERSION_PARAMETERS
from sve_carddb.template_parameters.models import Schema, Slot
from sve_carddb.template_parameters.references import References
from sve_carddb.template_translations.current import (
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
from sve_carddb.template_translations.definitions import payload
from sve_carddb.template_translations.members import ORDINALS
from sve_carddb.template_translations.models import Definition

from .adoption_fixtures import commit, git
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

    from sve_carddb.template_translations.current import Inputs
    from sve_carddb.template_translations.members import Reconstructed


@dataclass(frozen=True)
class Case:
    repository: Path
    revision: str
    inputs: Inputs
    sources: Sources
    generated: Generated


@pytest.fixture
def current_case(tmp_path: Path) -> Case:
    return make_case(tmp_path)


def make_case(tmp_path: Path) -> Case:
    root = tmp_path / "repository"
    root.mkdir()
    git(root, "init", "-b", "main")
    store = _store(tmp_path / "sources")
    raw = page("jp", '<div class="detail">甲2枚</div>')
    _put(store, _resource(card_url("SYN-01"), "raw/card.html", raw, Kind.CARD), raw)
    sealed = seal_batch(store)
    batch = Batch(batch_id=sealed.batch_id)
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


def _current_definition(member: Reconstructed) -> DefinitionRecord:
    schema = Schema(
        slots=tuple(
            Slot(
                name=h.name,
                type=h.type or "uint",
                occurrences=(h.occurrence,),
                reference_kind=h.reference_kind,
                min=(1 if role in ORDINALS else 0) if h.type == "uint" else None,
                max=SAFE_INTEGER if h.type == "uint" else None,
            )
            for h, role in zip(member.hints, member.roles, strict=True)
        )
    )
    member.verify_schema(schema)
    record = DefinitionRecord(
        record_key="temporary",
        kind="sentence_template",
        data=Definition(
            id="T" + "0" * 16,
            inventory_id=member.entry.id,
            source_span=member.candidate.source_span,
            source_lang="ja",
            normalizer_version=VERSION_PARAMETERS,
            semantic_variant="default",
            parameter_schema=schema,
            content_hash="sha256:" + "0" * 64,
            supersedes_id=None,
        ),
        origin="project",
        low_confidence=False,
        note="",
    )
    checksum = digest(payload(member, record))
    identifier = "T" + checksum[7:23]
    return record.model_copy(
        update={
            "record_key": canonical(["sentence_template", identifier]).decode(),
            "data": record.data.model_copy(
                update={"id": identifier, "content_hash": checksum}
            ),
        }
    )


def _records(generated: Generated) -> Shard:
    member = next(m for m in generated.entries if m.entry.role == "body")
    target = _current_definition(member)
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
        records=tuple(sorted((target, translation), key=lambda r: r.record_key)),
    )


def _write(root: Path, values: dict[str, JsonValue]) -> None:
    index: dict[str, JsonValue] = {
        "translation_authored_format": 2,
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


def test_new_definition_uses_payload_id_and_verifies_its_source(
    current_case: Case,
) -> None:
    record = _current_definition(current_case.generated.entries[0])
    assert record.data.id == "T" + record.data.content_hash[7:23]
    verified = validate_templates(
        replace(current_case.inputs, records=(record,)), current_case.sources
    )
    assert verified.frequencies == ((record.data.id, 1),)
    assert verified.missing_translations == (record.data.id,)


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


def test_current_build_does_not_require_legacy_catalog_or_environment_replay(
    current_case: Case,
) -> None:
    fresh = Sources(
        current_case.sources.stores, References(), current_case.sources.rules
    )
    verified = validate_templates(current_case.inputs, fresh)
    assert not verified.missing_translations
    assert sum(count for _, count in verified.frequencies) == 1
    assert fresh.generated_batches == 1

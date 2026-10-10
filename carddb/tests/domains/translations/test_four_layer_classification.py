"""Source grammar, concept closure and owner scope precede authored target matching."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import CardNameReference, GlossaryReference
from sve_carddb.domains.translations.four_layer_classification import (
    DOMAINS,
    Classifier,
    SourceReferences,
    Term,
)
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.parameters.rules import LEGACY_IDS, Rule, Rules

from .test_four_layer_normalizer import source as fixture_source

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import SourceDescriptor


def source(raw: str, field: str = "effect") -> SourceDescriptor:
    descriptor = fixture_source(raw, field)
    return descriptor.model_copy(
        update={
            "source_ref": descriptor.source_ref.model_copy(
                update={
                    "batch_id": "sha256:" + "a" * 64,
                    "source_version_id": "src:v1:" + "b" * 64,
                }
            )
        }
    )


def classifier(terms: tuple[Term, ...] = (), extra: tuple[str, ...] = ()) -> Classifier:
    refs = SourceReferences()
    for term in terms:
        refs.terms.setdefault(term.source_ja, []).append((term.id, term.category))
        if term.category == "card_name":
            refs.card_names.setdefault(term.source_ja, []).append(term.id)
    rules = Rules(
        format=2,
        kind="template_parameter_rules",
        rules=tuple(
            Rule(rule_id=key, enabled=True, origin="project", low_confidence=False)
            for key in sorted({*LEGACY_IDS, *extra})
        ),
    )
    return Classifier(terms, refs, rules)


def test_card_name_is_protected_before_numeric_and_phase_recognition() -> None:
    term = Term("term:name.synthetic", "card_name", "仮（２）😀")
    raw = "『仮（２）😀』を２枚選ぶ。\r\n"
    field = normalize_source(raw, source(raw))
    frames = []
    bindings = []
    for part in field.parts:
        recognized = classifier((term,)).recognize(raw, field.source, part)
        assert recognized.issues == ()
        definition, binding = recognized.bind(field.source, part)
        frames.append(definition)
        bindings.append(binding)
    field.verify(raw, tuple(frames), tuple(bindings), DOMAINS)
    assert bindings[0].values == {
        "leaf_0": CardNameReference(kind="card_name", term_id=term.id),
        "leaf_1": 2,
    }
    assert frames[0].semantic_variant.state == "pending"
    assert frames[0].semantic_variant.scope is not None
    assert bindings[0].occurrences[1].source_unit == "枚"
    assert all(f.projection.projection_kind == "none" for f in frames[1:])


def test_missing_name_concept_cannot_create_an_active_binding() -> None:
    raw = "『仮』を２枚選ぶ。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = classifier().recognize(raw, field.source, part)
    assert "missing_card_name_concept" in found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, part)


def test_phase_and_zone_values_come_from_source_roles_with_exact_positions() -> None:
    terms = (
        Term("term:phase.end", "rule_term", "エンドフェイズ"),
        Term("term:zone.hand", "rule_term", "手札"),
        Term("term:zone.graveyard", "rule_term", "墓場"),
    )
    raw = "エンドフェイズ開始時、手札から墓場に置く。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = classifier(terms).recognize(raw, field.source, part)
    definition, binding = found.bind(field.source, part)
    field.verify(raw, (definition,), (binding,), DOMAINS)
    assert [s.type for s in definition.leaf_schema.slots] == [
        "Phase",
        "ZoneSet",
        "ZoneSet",
    ]
    assert [s.role for s in definition.leaf_schema.slots] == [
        "phase_trigger",
        "source_zone",
        "destination_zone",
    ]
    assert binding.values == {
        "leaf_0": "end",
        "leaf_1": ("hand",),
        "leaf_2": ("graveyard",),
    }


def test_registered_threshold_alias_has_concept_and_numeric_leaves() -> None:
    term = Term("term:ability.necrocharge", "ability", "ネクロチャージ")
    raw = "【NC_２】仮。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = classifier((term,), ("keyword_alias_nc",)).recognize(
        raw, field.source, part
    )
    assert found.issues == ()
    definition, binding = found.bind(field.source, part)
    field.verify(raw, (definition,), (binding,), DOMAINS)
    assert binding.values == {
        "leaf_0": GlossaryReference(kind="glossary", key=term.id),
        "leaf_1": 2,
    }


def test_alias_id_without_registered_full_ability_spelling_is_not_authority() -> None:
    raw = "【NC_２】仮。"
    field = normalize_source(raw, source(raw))
    found = classifier(
        (Term("term:ability.necrocharge", "ability", "別の仮語"),),
        ("keyword_alias_nc",),
    ).recognize(raw, field.source, field.parts[0])
    assert found.issues
    assert not any(isinstance(v, GlossaryReference) for v in found.values.values())


def test_named_field_and_stale_source_use_exact_glossary_closure() -> None:
    term = Term("term:name.synthetic", "card_name", " 仮（２）😀 ")
    field = normalize_source(term.source_ja, source(term.source_ja, "name"))
    found = classifier((term,)).recognize(term.source_ja, field.source, field.parts[0])
    definition, binding = found.bind(field.source, field.parts[0])
    field.verify(term.source_ja, (definition,), (binding,), DOMAINS)
    with pytest.raises(ValueError, match="stale exact bytes"):
        classifier((term,)).recognize("他の仮名", field.source, field.parts[0])
    with pytest.raises(ValueError, match="exact glossary closure"):
        Classifier((term,), SourceReferences(), classifier().rules)


def test_unclassified_control_text_stays_in_exact_pending_frame() -> None:
    raw = "仮を選ぶ（しなくてもよい）。その後、ランダムに置く。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    definition, binding = (
        classifier().recognize(raw, field.source, part).bind(field.source, part)
    )
    field.verify(raw, (definition,), (binding,), DOMAINS)
    assert definition.projection.projection_kind == "pending"
    assert definition.semantic_variant.scope is not None
    assert definition.semantic_variant.scope.source_hash == field.source.source_hash
    assert "しなくてもよい" in part.canonical_source
    assert "ランダム" in part.canonical_source


def test_name_field_cannot_borrow_an_ability_concept_with_the_same_spelling() -> None:
    raw = "仮名"
    field = normalize_source(raw, source(raw, "name"))
    found = classifier((Term("term:ability.synthetic", "ability", raw),)).recognize(
        raw, field.source, field.parts[0]
    )
    assert found.issues == ("missing_or_ambiguous_named_concept",)

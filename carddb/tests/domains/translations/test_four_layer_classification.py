"""Source grammar, concept closure and owner scope precede authored target matching."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Constant,
    GlossaryReference,
    QuantitySpec,
)
from sve_carddb.domains.translations.four_layer_classification import (
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
    raw = "自分の手札の『仮（２）😀』を２枚選ぶ。\r\n"
    field = normalize_source(raw, source(raw))
    engine = classifier((term,))
    frames = []
    bindings = []
    for part in field.parts:
        recognized = engine.recognize(raw, field.source, part)
        assert recognized.issues == ()
        definition, binding = recognized.bind(field.source, part)
        frames.append(definition)
        bindings.append(binding)
    field.verify(raw, tuple(frames), tuple(bindings), engine.domains)
    assert bindings[0].values == {
        "leaf_0": CardNameReference(kind="card_name", term_id=term.id),
        "leaf_1": QuantitySpec(mode="exact", expr=Constant(kind="constant", value=2)),
    }
    assert frames[0].semantic_variant.state == "pending"
    assert frames[0].semantic_variant.scope is not None
    assert bindings[0].occurrences[1].source_unit == "枚"
    assert all(f.projection.projection_kind == "none" for f in frames[1:])


def test_leader_person_unit_survives_classification_and_binding_replay() -> None:
    raw = "相手のリーダー１人を選ぶ。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier(extra=("leader_person_quantity",))
    frame, binding = engine.recognize(raw, field.source, part).bind(field.source, part)
    field.verify(raw, (frame,), (binding,), engine.domains)
    assert binding.values == {
        "leaf_0": QuantitySpec(mode="exact", expr=Constant(kind="constant", value=1))
    }
    assert binding.occurrences[0].source_unit == "人"


def test_missing_name_concept_cannot_create_an_active_binding() -> None:
    raw = "『仮』を２枚選ぶ。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = classifier().recognize(raw, field.source, part)
    assert "missing_card_name_concept" in found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, part)


def test_fixed_phase_and_zone_words_do_not_add_n0_leaves() -> None:
    terms = (
        Term("term:phase.end", "rule_term", "エンドフェイズ"),
        Term("term:zone.hand", "rule_term", "手札"),
        Term("term:zone.cemetery", "rule_term", "墓場"),
    )
    raw = "エンドフェイズ開始時、手札から墓場に置く。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier(terms)
    definition, binding = engine.recognize(raw, field.source, part).bind(
        field.source, part
    )
    field.verify(raw, (definition,), (binding,), engine.domains)
    assert definition.leaf_schema.slots == ()
    assert binding.values == {}
    assert part.canonical_source == raw
    assert definition.semantic_variant.state == "pending"


def test_registered_threshold_alias_keeps_fixed_spelling_and_numeric_leaf() -> None:
    term = Term("term:ability.necrocharge", "ability", "ネクロチャージ")
    raw = "【NC_２】仮。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier((term,), ("keyword_alias_nc",))
    found = engine.recognize(raw, field.source, part)
    assert found.issues == ()
    definition, binding = found.bind(field.source, part)
    field.verify(raw, (definition,), (binding,), engine.domains)
    assert binding.values == {"leaf_0": 2}
    assert definition.leaf_schema.slots[0].role == "threshold"
    assert part.canonical_source == "【NC_N】仮。"


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
    engine = classifier((term,))
    found = engine.recognize(term.source_ja, field.source, field.parts[0])
    definition, binding = found.bind(field.source, field.parts[0])
    field.verify(term.source_ja, (definition,), (binding,), engine.domains)
    with pytest.raises(ValueError, match="stale exact bytes"):
        classifier((term,)).recognize("他の仮名", field.source, field.parts[0])
    with pytest.raises(ValueError, match="exact glossary closure"):
        Classifier((term,), SourceReferences(), classifier().rules)


def test_unclassified_control_text_stays_in_exact_pending_frame() -> None:
    raw = "仮を選ぶ（しなくてもよい）。その後、ランダムに置く。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier()
    definition, binding = engine.recognize(raw, field.source, part).bind(
        field.source, part
    )
    field.verify(raw, (definition,), (binding,), engine.domains)
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


@pytest.mark.parametrize(
    ("raw", "type_name", "role", "value", "unit"),
    [
        (
            "自分の手札のカードを２枚まで選ぶ。",
            "QuantitySpec",
            "selection_count",
            QuantitySpec(mode="up_to", expr=Constant(kind="constant", value=2)),
            "枚",
        ),
        (
            "相手の場のフォロワーが２体以上いるなら、仮。",
            "QuantitySpec",
            "existence_count",
            QuantitySpec(mode="at_least", expr=Constant(kind="constant", value=2)),
            "体",
        ),
        ("仮を２枚引く。", "Nat", "count", 2, "枚"),
        ("元のコスト２以下の仮。", "Nat", "threshold", 2, None),
    ],
)
def test_n0_quantity_constructs_preserve_roles_modes_and_exact_source_units(
    raw: str,
    type_name: str,
    role: str,
    value: int | QuantitySpec,
    unit: str | None,
) -> None:
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier()
    recognized = engine.recognize(raw, field.source, part)
    assert recognized.issues == ()
    definition, binding = recognized.bind(field.source, part)
    field.verify(raw, (definition,), (binding,), engine.domains)
    slot = definition.leaf_schema.slots[0]
    assert (slot.type, slot.role) == (type_name, role)
    assert binding.values == {slot.name: value}
    assert binding.occurrences[0].source_unit == unit


def test_generic_suffix_cannot_authorize_a_n0_numeric_leaf() -> None:
    raw = "仮２枚、仮。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    found = classifier().recognize(raw, field.source, part)
    assert found.issues == ("n0_numeric_construction_unresolved",)
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, part)


def test_braced_stat_and_action_leaves_keep_adopted_reference_categories() -> None:
    terms = (
        Term("term:stat.attack", "rule_term", "攻撃力"),
        Term("term:action.engage", "ability", "アクト"),
    )
    raw = "{攻撃力}と{アクト}を仮。"
    field = normalize_source(raw, source(raw))
    part = field.parts[0]
    engine = classifier(
        terms, ("braced_stat_reference", "braced_action_engage_reference")
    )
    found = engine.recognize(raw, field.source, part)
    assert found.issues == ()
    definition, binding = found.bind(field.source, part)
    field.verify(raw, (definition,), (binding,), engine.domains)
    assert [slot.role for slot in definition.leaf_schema.slots] == [
        "rule_term",
        "ability",
    ]
    assert binding.values == {
        "leaf_0": GlossaryReference(kind="glossary", key=terms[0].id),
        "leaf_1": GlossaryReference(kind="glossary", key=terms[1].id),
    }


def test_unrelated_catalog_member_preserves_frame_and_binding_identity() -> None:
    original = Term("term:name.first", "card_name", "仮名")
    raw = original.source_ja
    field = normalize_source(raw, source(raw, "name"))
    part = field.parts[0]
    first = classifier((original,))
    second = classifier((original, Term("term:name.other", "card_name", "他の仮名")))
    before = first.recognize(raw, field.source, part).bind(field.source, part)
    after = second.recognize(raw, field.source, part).bind(field.source, part)
    assert before == after
    field.verify(raw, (after[0],), (after[1],), second.domains)


@pytest.mark.parametrize(
    ("raw", "reason"),
    [
        ("相手の場のフォロワー２枚を選ぶ。", "source_unit_mismatch"),
        ("相手の墓場のフォロワー２体を選ぶ。", "source_unit_mismatch"),
        ("相手の場とEXエリアのフォロワー２体を選ぶ。", "source_unit_mismatch"),
        ("不明な集合２枚を選ぶ。", "n0_numeric_construction_unresolved"),
    ],
)
def test_counted_unit_requires_registered_object_and_counted_zone(
    raw: str, reason: str
) -> None:
    field = normalize_source(raw, source(raw))
    found = classifier().recognize(raw, field.source, field.parts[0])
    assert reason in found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


@pytest.mark.parametrize(
    "raw",
    [
        "相手の場のフォロワー２体を選ぶ。自分の手札に戻す。",
        "相手の墓場のフォロワー２枚を選ぶ。場に出す。",
        "相手の場とEXエリアのフォロワー２枚を選ぶ。自分の手札に加える。",
    ],
)
def test_counted_zone_precedes_destination_and_union_keeps_its_own_unit(
    raw: str,
) -> None:
    field = normalize_source(raw, source(raw))
    engine = classifier()
    found = engine.recognize(raw, field.source, field.parts[0])
    assert not found.issues
    frame, binding = found.bind(field.source, field.parts[0])
    field.verify(raw, (frame,), (binding,), engine.domains)
    assert binding.occurrences[0].source_unit in {"体", "枚"}


@pytest.mark.parametrize(
    "raw",
    [
        "自分のEXエリアのフォロワー２体を選ぶ。",
        "自分のEXエリアのトークン・フォロワー２枚を選ぶ。",
    ],
)
def test_ex_kind_or_unit_cannot_supply_missing_token_evidence(raw: str) -> None:
    field = normalize_source(raw, source(raw))
    found = classifier().recognize(raw, field.source, field.parts[0])
    assert found.issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        found.bind(field.source, field.parts[0])


def test_explicit_ex_token_follower_keeps_entity_unit() -> None:
    raw = "自分のEXエリアのトークン・フォロワー２体を選ぶ。"
    field = normalize_source(raw, source(raw))
    engine = classifier()
    found = engine.recognize(raw, field.source, field.parts[0])
    assert not found.issues
    frame, binding = found.bind(field.source, field.parts[0])
    field.verify(raw, (frame,), (binding,), engine.domains)
    assert binding.occurrences[0].source_unit == "体"


def test_generic_ex_source_includes_tokens_without_claiming_their_absence() -> None:
    raw = "自分のEXエリアのフォロワー２枚を選ぶ。"
    field = normalize_source(raw, source(raw))
    engine = classifier()
    frame, binding = engine.recognize(raw, field.source, field.parts[0]).bind(
        field.source, field.parts[0]
    )
    engine.verify(raw, field, field.parts[0], frame, binding)
    assert binding.occurrences[0].source_unit == "枚"


def test_battlefield_named_card_does_not_infer_kind_from_its_unit() -> None:
    name = Term("term:name.synthetic", "card_name", "仮名")
    raw = "自分の場の『仮名』２枚を選ぶ。"
    field = normalize_source(raw, source(raw))
    found = classifier((name,)).recognize(raw, field.source, field.parts[0])
    assert found.issues == ("n0_numeric_construction_unresolved",)

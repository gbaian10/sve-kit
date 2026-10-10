"""N1 preparation shares complete flags while keeping unresolved uses exact."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    GlossaryReference,
    LeafRef,
    LiteralNode,
    Span,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_authored import from_files
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from .test_four_layer_classification import classifier as n0_classifier
from .test_four_layer_classification import source


def classifier(terms: tuple[Term, ...] = (), extra: tuple[str, ...] = ()) -> Classifier:
    return n0_classifier(
        terms, (*extra, "braced_ability_reference", "bracket_keyword_reference")
    )


QUICK = Term("term:ability.quick", "ability", "クイック")
KEYWORD = Term("term:keyword.synthetic", "keyword", "仮旗")


def context(raw: str, owner: str = "revision:synthetic") -> CardContext:
    descriptor = source(raw).model_copy(
        update={"owner": FaceRevisionOwner(kind="face_revision", revision_id=owner)}
    )
    return CardContext(descriptor, "card:" + owner, "face:normal", "normal", "spell")


def test_quick_shares_one_target_but_preserves_each_owner_and_position() -> None:
    engine = classifier((QUICK,), ("braced_ability_reference",))
    target = SelectedTarget(
        Target(
            format=1,
            nodes=(LeafRef(kind="LeafRef", slot="leaf_0"),),
        )
    )
    reference = GlossaryReference(kind="glossary", key=QUICK.id)
    label = Label(reference, "zh-Hant", "Quick", "machine", True, True)
    renderer = Renderer(
        {(canonical(reference.model_dump(mode="json")), "zh-Hant"): label},
        {},
        {},
        domains=engine.domains,
    )
    frames = []
    bindings = []
    for owner in ("revision:first", "revision:second"):
        raw = "{クイック}"
        facts = context(raw, owner)
        derived = derive_source(raw, facts.source, engine, context=facts)
        assert derived.pending_causes == ((),)
        frame, binding = derived.bind(0)
        frame.verify(derived.field.parts[0].canonical_source)
        binding.verify(frame, engine.domains)
        assert frame.projection.projection_kind == "card_field"
        assert frame.projection.scopes == ()
        assert frame.projection.imports == ()
        assert frame.projection.exports == ()
        assert frame.semantic_variant.key == "quick_card_field.v1"
        assert binding.values == {"leaf_0": reference}
        assert binding.occurrences[0].raw_spans == (Span(start=1, end=5),)
        result = renderer.render(
            owner, "zh-Hant", (BoundTarget(frame, binding, target),)
        )
        assert result.rendered is not None
        assert result.rendered.text == "Quick"
        assert result.rendered.origin == "machine"
        assert result.rendered.low_confidence
        occurrence = result.rendered.annotation.occurrences[0]
        assert occurrence.reference == reference
        assert occurrence.ranges == (Span(start=0, end=5),)
        assert result.rendered.leaves[0].source_ordinals == (0,)
        assert result.rendered.occurrences()[0].binding_id == binding.id
        frames.append(frame)
        bindings.append(binding)
    assert frames[0] == frames[1]
    assert bindings[0].id != bindings[1].id
    assert bindings[0].source.owner != bindings[1].source.owner


def test_literal_cannot_replace_the_required_quick_reference() -> None:
    raw = "{クイック}"
    facts = context(raw)
    frame, _ = derive_source(
        raw, facts.source, classifier((QUICK,)), context=facts
    ).bind(0)
    target = Target(format=1, nodes=(LiteralNode(kind="Literal", text="【Quick】"),))
    with pytest.raises(ValueError, match="required"):
        target.verify(frame.leaf_schema, "zh-Hant", {})


def test_valid_quick_target_with_a_missing_label_preserves_whole_field_fallback() -> (
    None
):
    raw = "{クイック}"
    facts = context(raw)
    engine = classifier((QUICK,))
    frame, binding = derive_source(raw, facts.source, engine, context=facts).bind(0)
    target = SelectedTarget(
        Target(format=1, nodes=(LeafRef(kind="LeafRef", slot="leaf_0"),))
    )
    result = Renderer({}, {}, {}, domains=engine.domains).render(
        "context:synthetic", "zh-Hant", (BoundTarget(frame, binding, target),)
    )
    assert result.rendered is None
    assert result.issues == ("missing_term_translation",)


@pytest.mark.parametrize(
    ("phase", "kind"),
    [("evolved", "spell"), ("normal", "follower"), ("normal", "amulet")],
)
def test_quick_needs_its_intrinsic_spell_context(phase: str, kind: str) -> None:
    raw = "{クイック}"
    facts = replace(context(raw), phase=phase, card_kind=kind)
    derived = derive_source(raw, facts.source, classifier((QUICK,)), context=facts)
    frame, binding = derived.bind(0)
    assert frame.semantic_variant.state == "pending"
    assert frame.semantic_variant.scope == binding.occurrence_key()
    assert derived.pending_causes == (("unresolved_whole_line_semantics",),)


def test_quick_without_context_does_not_share_across_owners() -> None:
    raw = "{クイック}"
    frames = [
        derive_source(raw, context(raw, owner).source, classifier((QUICK,))).bind(0)[0]
        for owner in ("revision:first", "revision:second")
    ]
    assert frames[0].id != frames[1].id
    assert all(frame.semantic_variant.state == "pending" for frame in frames)


@pytest.mark.parametrize(
    "raw",
    [
        "条件なら{クイック}",
        "{クイック}を持つ。",
        "{クイック}仮。",
        "{クイック}【仮旗】",
        "{クイック}、",
        "{クイック}、{クイック}",
    ],
)
def test_granted_conditional_or_compound_flags_do_not_become_quick_card_fields(
    raw: str,
) -> None:
    facts = context(raw)
    found = derive_source(
        raw, facts.source, classifier((QUICK, KEYWORD)), context=facts
    ).recognized[0]
    assert found.semantics is None


def test_keyword_occurrences_are_not_deduplicated_and_names_are_not_keywords() -> None:
    raw = "【仮旗】、【仮旗】"
    engine = classifier((KEYWORD, Term("term:name.flag", "card_name", "仮旗")))
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    assert frame.semantic_variant.state == "resolved"
    assert tuple(slot.role for slot in frame.leaf_schema.slots) == (
        "keyword",
        "keyword",
    )
    assert tuple(binding.values.values()) == (
        GlossaryReference(kind="glossary", key=KEYWORD.id),
        GlossaryReference(kind="glossary", key=KEYWORD.id),
    )
    assert tuple(o.raw_spans for o in binding.occurrences) == (
        (Span(start=1, end=3),),
        (Span(start=6, end=8),),
    )
    binding.verify(frame, engine.domains)


@pytest.mark.parametrize(
    ("terms", "reason"),
    [
        ((), "missing_keyword_concept"),
        (
            (KEYWORD, Term("term:keyword.other", "keyword", "仮旗")),
            "ambiguous_keyword_concept",
        ),
        ((Term(QUICK.id, "keyword", QUICK.source_ja),), "invalid_quick_category"),
    ],
)
def test_missing_ambiguous_or_wrong_category_concepts_cannot_bind(
    terms: tuple[Term, ...], reason: str
) -> None:
    raw = "{クイック}" if reason == "invalid_quick_category" else "【仮旗】"
    derived = derive_source(raw, source(raw), classifier(terms))
    assert reason in derived.pending_causes[0]
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        derived.bind(0)


def test_quick_inside_a_card_name_does_not_create_an_ability_leaf() -> None:
    raw = "『仮{クイック}😀』を９９枚選ぶ。"
    term = Term("term:name.synthetic", "card_name", "仮{クイック}😀")
    engine = classifier((QUICK, term))
    derived = derive_source(raw, source(raw), engine)
    assert derived.recognized[0].semantics is None
    assert all(slot.role != "ability" for slot in derived.recognized[0].schema.slots)


def test_new_keyword_leaves_keep_the_original_whitespace_partition() -> None:
    raw = "  【仮旗】\r\n【仮旗】  "
    derived = derive_source(raw, source(raw), classifier((KEYWORD,)))
    bodies = tuple(
        derived.bind(part.ordinal)
        for part in derived.field.parts
        if part.source_span.role == "body"
    )
    assert bodies[0][0] == bodies[1][0]
    assert bodies[0][1].occurrences[0].raw_spans == (Span(start=3, end=5),)
    assert bodies[1][1].occurrences[0].raw_spans == (Span(start=9, end=11),)
    assert tuple(part.source_span.role for part in derived.field.parts) == (
        "layout",
        "body",
        "layout",
        "layout",
        "body",
        "layout",
    )


def test_context_cannot_be_borrowed_from_another_owner_with_equal_text() -> None:
    raw = "{クイック}"
    with pytest.raises(ValueError, match="another exact owner field"):
        derive_source(
            raw,
            context(raw, "revision:first").source,
            classifier((QUICK,)),
            context=context(raw, "revision:second"),
        )


def test_missing_quick_concept_is_lexical_pending_rather_than_a_literal_flag() -> None:
    raw = "{クイック}"
    derived = derive_source(raw, source(raw), classifier())
    assert derived.pending_causes == (("missing_keyword_concept",),)
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        derived.bind(0)


def test_spelling_from_a_catalog_does_not_register_a_new_quick_alias() -> None:
    raw = "{仮速度}"
    term = Term(QUICK.id, "ability", "仮速度")
    facts = context(raw)
    derived = derive_source(
        raw,
        facts.source,
        classifier((term,), ("braced_ability_reference",)),
        context=facts,
    )
    assert derived.recognized[0].semantics is None


def test_quick_in_a_section_cannot_borrow_the_whole_card_spell_flag() -> None:
    raw = "{クイック}"
    facts = replace(context(raw), source=source(raw, "section"))
    derived = derive_source(raw, facts.source, classifier((QUICK,)), context=facts)
    assert derived.recognized[0].semantics is None


def test_stale_source_is_rejected_before_any_shared_frame_is_derived() -> None:
    with pytest.raises(ValueError, match="stale exact bytes"):
        derive_source("【仮旗】仮。", source("【仮旗】"), classifier((KEYWORD,)))


def test_numeric_placeholder_cannot_impersonate_an_adopted_keyword_spelling() -> None:
    raw = "【仮９９】"
    term = Term("term:keyword.synthetic", "keyword", "仮N")
    derived = derive_source(raw, source(raw), classifier((term,)))
    assert "keyword_source_replacement_unresolved" in derived.pending_causes[0]
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        derived.bind(0)


def test_quick_nfkc_equivalence_keeps_distinct_source_spelling_trace() -> None:
    engine = classifier((QUICK,))
    pairs = []
    for raw in ("{クイック}", "｛ｸｲｯｸ｝"):
        facts = context(raw)
        derived = derive_source(raw, facts.source, engine, context=facts)
        pairs.append(derived.bind(0))
    assert pairs[0][0] == pairs[1][0]
    assert pairs[0][1].source.source_hash != pairs[1][1].source.source_hash
    assert pairs[0][1].trace != pairs[1][1].trace
    assert any(piece.rule == "nfkc" for piece in pairs[1][1].trace)


def test_unknown_whole_lines_keep_existing_typed_values_but_do_not_share() -> None:
    raw = "仮。自分の手札の『仮札』を９９枚選ぶ。"
    engine = classifier((Term("term:name.synthetic", "card_name", "仮札"),))
    frames = []
    for owner in ("revision:first", "revision:second"):
        facts = context(raw, owner)
        derived = derive_source(raw, facts.source, engine, context=facts)
        frame, binding = derived.bind(0)
        assert frame.projection.projection_kind == "pending"
        assert frame.semantic_variant.scope == binding.occurrence_key()
        assert frame.leaf_schema.slots[-1].role == "selection_count"
        assert binding.occurrences[-1].source_unit == "枚"
        frames.append(frame)
    assert frames[0].id != frames[1].id


def test_existing_entry_semantics_and_leaf_values_survive_preparation() -> None:
    raw = "{進化}{コスト９９}:これは進化する。"
    facts = replace(context(raw), card_kind="follower")
    engine = classifier()
    field = normalize_source(raw, facts.source)
    n0 = engine.recognize(raw, facts.source, field.parts[0], context=facts, field=field)
    derived = derive_source(raw, facts.source, engine, context=facts)
    assert derived.recognized[0] == n0
    old_frame, old_binding = n0.bind(facts.source, field.parts[0])
    frame, binding = derived.bind(0)
    assert frame.id != old_frame.id
    assert frame.semantic_variant == old_frame.semantic_variant
    assert frame.projection == old_frame.projection
    assert binding.values == old_binding.values
    assert binding.occurrences == old_binding.occurrences
    assert binding.trace == old_binding.trace


def test_preparation_does_not_enable_n1_in_the_production_reader_or_classifier() -> (
    None
):
    raw = "{クイック}"
    facts = context(raw)
    engine = classifier((QUICK,), ("braced_ability_reference",))
    field = normalize_source(raw, facts.source)
    n0 = engine.recognize(raw, facts.source, field.parts[0], context=facts, field=field)
    old_frame, _ = n0.bind(facts.source, field.parts[0])
    assert old_frame.source.normalizer_version == "four-layer-jp-v1"
    assert old_frame.semantic_variant.state == "pending"
    assert old_frame.leaf_schema.slots[0].role == "ability"
    frame, _ = derive_source(raw, facts.source, engine, context=facts).bind(0)
    payload = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [
                {"kind": "sentence_template", "data": frame.model_dump(mode="json")}
            ],
        }
    )
    with pytest.raises(ValueError, match="Unsupported four-layer authored normalizer"):
        from_files((("translations/templates/definitions/000.yaml", payload, payload),))

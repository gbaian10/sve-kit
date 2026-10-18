"""Lexical terms retain their source ranges and selected glossary dependencies."""

import pytest

from sve_carddb.contracts.four_layer import (
    GlossaryReference,
    LeafRef,
    LiteralNode,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .test_four_layer_classification import classifier, source
from .test_four_layer_n1 import n1_classifier


def test_repeated_token_words_render_selected_glossary_with_source_ranges() -> None:
    raw = "トークンとトークンを仮置きする。"
    engine = classifier()
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding)
    slots = frame.leaf_schema.slots
    assert len(slots) == 2
    assert all(s.required and s.type == "TokenStatus" for s in slots)
    reference = GlossaryReference(kind="glossary", key="term:object.token")
    key = canonical(reference.model_dump(mode="json")), "zh-Hant"
    renderer = Renderer(
        {key: Label(reference, "zh-Hant", "Token", "project", False, True)}, {}, {}
    )
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=(
                LeafRef(kind="LeafRef", slot=slots[0].name),
                LiteralNode(kind="Literal", text="與"),
                LeafRef(kind="LeafRef", slot=slots[1].name),
            ),
        )
    )
    plan = (BoundTarget(frame, binding, selected),)
    first = renderer.render("ctx:token", "zh-Hant", plan)
    assert first.rendered is not None
    assert first.rendered.text == "Token與Token"
    assert len(first.rendered.annotation.occurrences) == 2
    assert len(first.rendered.leaves) == 2
    assert [o.raw_spans for o in binding.occurrences] == [s.occurrences for s in slots]
    renderer.labels[key] = Label(reference, "zh-Hant", "替代詞", "project", True, True)
    changed = renderer.render("ctx:token", "zh-Hant", plan)
    assert changed.rendered is not None
    assert changed.rendered.text == "替代詞與替代詞"
    assert changed.rendered.low_confidence is True
    assert changed.rendered.dependency_key != first.rendered.dependency_key
    del renderer.labels[key]
    missing = renderer.render("ctx:token", "zh-Hant", plan)
    assert missing.rendered is None
    assert missing.issues == ("missing_term_translation",)


def test_quoted_token_spelling_is_a_card_name() -> None:
    raw = "『仮トークン』を仮置きする。"
    engine = classifier((Term("term:name.synthetic", "card_name", "仮トークン"),))
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding)
    assert [s.type for s in frame.leaf_schema.slots] == ["CardName"]


def test_existing_token_filter_keeps_one_source_leaf() -> None:
    raw = "自分の場のトークン・フォロワー1体を選ぶ。"
    engine = n1_classifier()
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding)
    token_slots = [s for s in frame.leaf_schema.slots if s.type == "TokenStatus"]
    assert len(token_slots) == 1
    assert token_slots[0].role == "token_filter"


def test_token_word_in_a_reminder_has_a_required_glossary_leaf() -> None:
    raw = "（戻したトークンは仮の場所へ移す）"
    engine = classifier()
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding)
    assert len(frame.leaf_schema.slots) == 1
    assert frame.leaf_schema.slots[0].type == "TokenStatus"


def test_trait_type_word_tracks_glossary_changes_and_missing_translation() -> None:
    raw = "仮甲タイプと仮乙タイプを付ける。"
    engine = classifier((Term("term:rule.trait_type", "rule_term", "タイプ"),))
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding)
    slots = frame.leaf_schema.slots
    assert len(slots) == 2
    assert all(s.required and s.type == "Concept" for s in slots)
    reference = GlossaryReference(kind="glossary", key="term:rule.trait_type")
    key = canonical(reference.model_dump(mode="json")), "zh-Hant"
    renderer = Renderer(
        {key: Label(reference, "zh-Hant", "類型", "project", False, False)},
        {},
        {},
        domains=engine.domains,
    )
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=(
                LiteralNode(kind="Literal", text="賦予甲"),
                LeafRef(kind="LeafRef", slot=slots[0].name),
                LiteralNode(kind="Literal", text="與乙"),
                LeafRef(kind="LeafRef", slot=slots[1].name),
            ),
        )
    )
    plan = (BoundTarget(frame, binding, selected),)
    first = renderer.render("ctx:trait-type", "zh-Hant", plan)
    assert first.rendered is not None
    assert first.rendered.text == "賦予甲類型與乙類型"
    assert len(first.rendered.annotation.occurrences) == 2
    assert all(o.bold is False for o in first.rendered.annotation.occurrences)
    assert len(first.rendered.leaves) == 2
    assert [o.raw_spans for o in binding.occurrences] == [s.occurrences for s in slots]
    renderer.labels[key] = Label(reference, "zh-Hant", "替代詞", "project", True, False)
    changed = renderer.render("ctx:trait-type", "zh-Hant", plan)
    assert changed.rendered is not None
    assert changed.rendered.text == "賦予甲替代詞與乙替代詞"
    assert changed.rendered.low_confidence is True
    assert changed.rendered.dependency_key != first.rendered.dependency_key
    del renderer.labels[key]
    missing = renderer.render("ctx:trait-type", "zh-Hant", plan)
    assert missing.rendered is None
    assert missing.issues == ("missing_term_translation",)


def test_trait_type_spelling_in_card_name_keeps_its_name_reference() -> None:
    raw = "『仮タイプ』を仮置きする。"
    engine = classifier(
        (
            Term("term:name.synthetic", "card_name", "仮タイプ"),
            Term("term:rule.trait_type", "rule_term", "タイプ"),
        )
    )
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding)
    assert [s.type for s in frame.leaf_schema.slots] == ["CardName"]


def test_unresolved_braced_term_stays_pending_without_an_extra_lexical_leaf() -> None:
    raw = "{タイプ}を仮置きする。"
    engine = classifier((Term("term:rule.trait_type", "rule_term", "タイプ"),))
    derived = derive_source(raw, source(raw, "section"), engine)
    recognized = derived.recognized[0]
    assert not recognized.schema.slots
    assert recognized.issues == ("term_role_requires_review",)
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        derived.bind(0)


def test_trait_type_word_requires_its_registered_concept() -> None:
    raw = "仮タイプを付ける。"
    engine = classifier()
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify(raw, engine, frame, binding)
    assert not frame.leaf_schema.slots

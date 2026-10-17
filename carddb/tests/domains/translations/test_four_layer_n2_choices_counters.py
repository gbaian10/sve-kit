"""Sealed choice interfaces and counter capacity leave production recognition alone."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import (
    GlossaryReference,
    LeafRef,
    LiteralNode,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_analysis import analyze_source
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .test_four_layer_classification import classifier, source


def test_constant_choice_changes_only_the_interface_and_preserves_target() -> None:
    raw = "下記から１つチョイスする。【１】仮。【２】別。"
    engine = classifier(extra=("bracket_choice_index", "suffix_unit_items"))
    old = analyze_source(raw, source(raw), engine)
    prepared = derive_source(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    prior, prior_binding = old.recognized[0].bind(old.field.source, old.field.parts[0])
    assert frame.leaf_schema == prior.leaf_schema
    assert binding.values == prior_binding.values
    assert binding.occurrences == prior_binding.occurrences
    assert binding.trace == prior_binding.trace
    assert frame.semantic_variant == prior.semantic_variant
    assert frame.semantic_variant.state == "pending"
    assert len(frame.projection.exports) == 1
    assert [p.scope for p in frame.projection.imports] == ["choice_1", "choice_2"]
    assert {p.type for p in (*frame.projection.exports, *frame.projection.imports)} == {
        "ChoiceContext"
    }
    assert frame.projection.interface_key(frame.id) != prior.projection.interface_key(
        prior.id
    )
    target = Target(
        format=1,
        nodes=(
            LiteralNode(kind="Literal", text="選擇"),
            LeafRef(kind="LeafRef", slot="leaf_0"),
            LiteralNode(kind="Literal", text="項：【"),
            LeafRef(kind="LeafRef", slot="leaf_1"),
            LiteralNode(kind="Literal", text="】甲；【"),
            LeafRef(kind="LeafRef", slot="leaf_2"),
            LiteralNode(kind="Literal", text="】乙。"),
        ),
    )
    renderer = Renderer({}, {}, {}, domains=engine.domains)

    def render(item: BoundTarget) -> str:
        result = renderer.render("synthetic", "zh-Hant", (item,))
        assert result.rendered is not None
        return result.rendered.text

    selected = SelectedTarget(target)
    assert render(BoundTarget(frame, binding, selected)) == render(
        BoundTarget(prior, prior_binding, selected)
    )
    prepared.verify(raw, engine, frame, binding, target=target)
    changed = replace(prepared, interfaces=(prior.projection,))
    with pytest.raises(ValueError, match="reconstructed"):
        changed.verify(raw, engine, frame, binding)
    with pytest.raises(ValueError, match="construction"):
        prepared.verify(
            raw,
            engine,
            frame.model_copy(update={"projection": prior.projection}),
            binding,
        )


@pytest.mark.parametrize(
    "raw",
    [
        "下記から１つチョイスする。\n【１】仮。\n【２】別。",
        "下記から１つチョイスする。【１】仮。\n【２】別。",
        "【１】仮。【２】別。",
        "下記から１つチョイスする。【２】仮。【１】別。",
        "下記から１つチョイスする。【１】仮。【２】別。下記から１つチョイスする。",
        "下記から１つチョイスする。【１】仮。\n\n【２】別。",
        "下記から１つチョイスする。\n{起動}【１】仮。【２】別。",
    ],
)
def test_unsealed_choice_connections_gain_no_interface(raw: str) -> None:
    engine = classifier(extra=("bracket_choice_index", "suffix_unit_items"))
    old = analyze_source(raw, source(raw), engine)
    prepared = derive_source(raw, source(raw), engine)
    assert prepared.derived == old
    assert not any(prepared.interfaces)


def test_capacity_keeps_named_kind_literal_and_requires_counter_label() -> None:
    raw = "これに仮印カウンターは３個まで置ける。"
    counter = GlossaryReference(kind="glossary", key="term:object.counter")
    engine = classifier((Term(counter.key, "rule_term", "カウンター"),))
    old = analyze_source(raw, source(raw), engine)
    assert old.recognized[0].issues
    prepared = derive_source(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    roles = {s.role: s.name for s in frame.leaf_schema.slots}
    assert binding.values[roles["counter_capacity"]] == 3
    assert binding.values[roles["rule_term"]] == counter
    assert frame.semantic_variant.state == "pending"
    target = Target(
        format=1,
        nodes=(
            LiteralNode(kind="Literal", text="這張卡片最多可以放置"),
            LeafRef(kind="LeafRef", slot=roles["counter_capacity"]),
            LiteralNode(kind="Literal", text="個假印"),
            LeafRef(kind="LeafRef", slot=roles["rule_term"]),
            LiteralNode(kind="Literal", text="。"),
        ),
    )
    prepared.verify(raw, engine, frame, binding, target=target)
    plan = (BoundTarget(frame, binding, SelectedTarget(target)),)
    label = Label(counter, "zh-Hant", "指示物", "project", False, False)
    renderer = Renderer(
        {(canonical(counter.model_dump(mode="json")), label.lang): label},
        {},
        {},
        domains=engine.domains,
    )
    result = renderer.render("synthetic", "zh-Hant", plan)
    assert result.rendered is not None
    assert result.rendered.text == "這張卡片最多可以放置3個假印指示物。"
    assert result.rendered.leaves
    assert (
        Renderer({}, {}, {}, domains=engine.domains)
        .render("synthetic", "zh-Hant", plan)
        .rendered
        is None
    )
    missing = derive_source(raw, source(raw), classifier())
    with pytest.raises(ValueError, match="Unresolved"):
        missing.bind(0)


@pytest.mark.parametrize(
    "raw",
    [
        "それに仮印カウンターは３個まで置ける。",
        "これにスタックカウンターは３個まで置ける。",
        "これに仮印カウンター３個を置く。",
        "これに仮印カウンターはX個まで置ける。",
    ],
)
def test_capacity_does_not_authorize_other_holders_or_amounts(raw: str) -> None:
    engine = classifier((Term("term:object.counter", "rule_term", "カウンター"),))
    assert derive_source(raw, source(raw), engine).derived == analyze_source(
        raw, source(raw), engine
    )

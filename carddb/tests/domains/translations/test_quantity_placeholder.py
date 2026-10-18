"""Source-bound choice counts use X only beside their independently rendered definition."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Expression,
    FormDefinition,
    FormNode,
    LeafRef,
    LiteralNode,
    QuantitySpec,
    SlotRef,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import (
    definition,
    target_dependencies,
    verify_target_shape,
)
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Form,
    Label,
    Renderer,
    SelectedTarget,
    validate_form,
)

from .test_four_layer_classification import source
from .test_four_layer_n3 import n3_classifier

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.contracts.four_layer import Node


def placeholder_form() -> FormDefinition:
    return FormDefinition.model_validate_json(
        canonical(
            {
                "id": "quantity.choice_mode_count.placeholder",
                "lang": "zh-Hant",
                "signature": [
                    {
                        "name": "quantity",
                        "type": "QuantitySpec",
                        "role": "choice_mode_count",
                    },
                    {
                        "name": "definition",
                        "type": "QuantityExpr",
                        "role": "count_expression",
                    },
                ],
                "rule": "quantity.choice_mode_count.placeholder.v1",
                "cases": {
                    "up_to": [
                        {"kind": "Literal", "text": "至多"},
                        {"kind": "QuantityPlaceholder", "arg": "quantity"},
                    ]
                },
            }
        )
    )


@pytest.fixture
def choice() -> tuple[Renderer, BoundTarget, FormNode]:
    raw = "仮。Xは「自分の場の仮族・カードの枚数」である。下記からX個までチョイスする。【1】1枚引く。【2】2枚引く。【3】3枚引く。"
    engine = n3_classifier()
    derived = derive_source(raw, source(raw), engine)
    frame, binding = derived.bind(0)
    derived.verify_binding(raw, engine, frame, binding)
    slots = {slot.role: slot.name for slot in frame.leaf_schema.slots}
    quantity = binding.values[slots["choice_mode_count"]]
    assert isinstance(quantity, QuantitySpec)
    assert isinstance(quantity.expr, Expression)
    form = placeholder_form()
    form_node = FormNode(
        kind="Form",
        form_id=form.id,
        args={
            "quantity": SlotRef(slot=slots["choice_mode_count"]),
            "definition": SlotRef(slot=slots["count_expression"]),
        },
    )
    dependencies = target_dependencies(frame, binding, registry=derived.registry)
    nested = {name for names in dependencies.values() for name in names}
    nodes: tuple[Node, ...] = (
        *(
            LeafRef(kind="LeafRef", slot=slot.name)
            for slot in frame.leaf_schema.slots
            if slot.required
            and slot.name not in nested
            and slot.role not in {"choice_mode_count", "count_expression"}
        ),
        LiteralNode(kind="Literal", text="選"),
        form_node,
        LiteralNode(kind="Literal", text="項。X為「"),
        LeafRef(kind="LeafRef", slot=slots["count_expression"]),
        LiteralNode(kind="Literal", text="」。"),
    )
    expression = Target(
        format=1,
        nodes=tuple(
            LeafRef(kind="LeafRef", slot=name)
            for name in definition(quantity.expr).arguments(
                quantity.expr, frame.leaf_schema
            )
        ),
    )
    item = BoundTarget(
        frame,
        binding,
        SelectedTarget(
            Target(format=1, nodes=nodes),
            expression_targets={
                quantity.expr.expression_id: expression,
            },
        ),
    )
    renderer = Renderer(
        {}, {}, {(form.id, form.lang): Form(form)}, domains=engine.domains
    )
    add_labels(renderer, item)
    return renderer, item, form_node


def add_labels(renderer: Renderer, item: BoundTarget) -> None:
    references = {
        canonical(ref.model_dump(mode="json")): ref
        for slot in item.frame.leaf_schema.slots
        for ref in renderer.references(item, slot.name)
    }
    renderer.labels.update(
        {
            (key, "zh-Hant"): Label(
                ref,
                "zh-Hant",
                "仮標籤" + str(i),
                "project",
                False,
                True,
            )
            for i, (key, ref) in enumerate(sorted(references.items()))
        }
    )


@pytest.mark.parametrize("definition_first", [False, True])
def test_placeholder_renders_once_with_traceable_ranges(
    choice: tuple[Renderer, BoundTarget, FormNode],
    *,
    definition_first: bool,
) -> None:
    renderer, item, node = choice
    assert item.selected is not None
    if definition_first:
        nodes = item.selected.target.nodes
        item = replace(
            item,
            selected=replace(
                item.selected,
                target=Target(
                    format=1,
                    nodes=(nodes[-2], *nodes[:-2], nodes[-1]),
                ),
            ),
        )
    forms = {key: value.definition for key, value in renderer.forms.items()}
    assert item.selected is not None
    verify_target_shape(
        item.frame,
        item.selected.target,
        item.selected.expression_targets,
        "zh-Hant",
        forms,
    )
    result = renderer.render("synthetic", "zh-Hant", (item,))
    assert result.rendered is not None
    output = result.rendered
    assert output.text.count("至多X") == 1
    quantity = next(
        leaf for leaf in output.leaves if leaf.slot == node.args["quantity"].slot
    )
    definition_leaf = next(
        leaf for leaf in output.leaves if leaf.slot == node.args["definition"].slot
    )
    span = quantity.ranges[0]
    assert output.text[span.start : span.end] == "X"
    span = definition_leaf.ranges[0]
    definition_text = output.text[span.start : span.end]
    assert output.text.count(definition_text) == 1
    assert quantity.source_ordinals
    assert definition_leaf.source_ordinals
    for annotation in output.annotation.occurrences:
        assert all(
            not (r.start <= quantity.ranges[0].start < r.end) for r in annotation.ranges
        )
    assert any(leaf.node_path != definition_leaf.node_path for leaf in output.leaves)
    assert not output.low_confidence
    low = replace(renderer.forms[node.form_id, "zh-Hant"], low_confidence=True)
    renderer.forms[node.form_id, "zh-Hant"] = low
    changed = renderer.render("synthetic", "zh-Hant", (item,)).rendered
    assert changed is not None
    assert changed.low_confidence
    assert changed.text == output.text
    assert changed.dependency_key != output.dependency_key


def test_placeholder_missing_operand_label_keeps_whole_field_fallback(
    choice: tuple[Renderer, BoundTarget, FormNode],
) -> None:
    renderer, item, _ = choice
    for key in tuple(renderer.labels):
        labels = dict(renderer.labels)
        del labels[key]
        incomplete = Renderer(labels, {}, renderer.forms, domains=renderer.domains)
        result = incomplete.render("synthetic", "zh-Hant", (item,))
        assert result.rendered is None
        assert result.issues == ("missing_term_translation",)


def test_placeholder_requires_direct_definition_even_when_operands_are_rendered(
    choice: tuple[Renderer, BoundTarget, FormNode],
) -> None:
    renderer, item, node = choice
    assert item.selected is not None
    target = Target(
        format=1,
        nodes=tuple(
            other
            for other in item.selected.target.nodes
            if not isinstance(other, LeafRef)
            or other.slot != node.args["definition"].slot
        ),
    )
    item = replace(item, selected=replace(item.selected, target=target))
    assert item.selected is not None
    forms = {key: value.definition for key, value in renderer.forms.items()}
    with pytest.raises(ValueError, match="direct definition LeafRef"):
        verify_target_shape(
            item.frame, target, item.selected.expression_targets, "zh-Hant", forms
        )
    with pytest.raises(ValueError, match="direct definition LeafRef"):
        renderer.render("synthetic", "zh-Hant", (item,))


@pytest.mark.parametrize(
    "mutation", ["mode", "constant", "bound", "ordered_leaves", "definition", "role"]
)
def test_placeholder_rejects_incompatible_binding_pairs(
    choice: tuple[Renderer, BoundTarget, FormNode],
    mutation: str,
) -> None:
    renderer, item, node = choice
    name = node.args["quantity"].slot
    values = dict(item.binding.values)
    quantity = values[name]
    assert isinstance(quantity, QuantitySpec)
    assert isinstance(quantity.expr, Expression)
    if mutation == "role":
        item = replace(
            item,
            frame=item.frame.model_copy(
                update={
                    "leaf_schema": item.frame.leaf_schema.model_copy(
                        update={
                            "slots": tuple(
                                slot.model_copy(update={"role": "selection_count"})
                                if slot.name == name
                                else slot
                                for slot in item.frame.leaf_schema.slots
                            )
                        }
                    )
                }
            ),
        )
    else:
        data: dict[str, JsonValue] = quantity.model_dump(mode="json")
        if mutation == "constant":
            data["expr"] = {"kind": "constant", "value": 2}
        elif mutation == "bound":
            data["expr"] = {"kind": "bound", "import": "unknown"}
        elif mutation == "mode":
            data["mode"] = "exact"
        elif mutation == "ordered_leaves":
            values[node.args["definition"].slot] = quantity.expr.model_copy(
                update={
                    "leaves": tuple(reversed(quantity.expr.leaves)),
                }
            )
        else:
            values[node.args["definition"].slot] = quantity.expr.model_copy(
                update={
                    "expression_id": "count.cards.zone.v1",
                }
            )
        if mutation in {"mode", "constant", "bound"}:
            values[name] = QuantitySpec.model_validate_json(canonical(data))
        item = replace(item, binding=item.binding.model_copy(update={"values": values}))
    with pytest.raises(
        ValueError,
        match=r"[Pp]laceholder|[Cc]hoice|[Ee]xpression|[Ff]orm|[Ll]eaf|[Qq]uantity|[Ss]ignature",
    ):
        renderer.render("synthetic", "zh-Hant", (item,))


@pytest.mark.parametrize(
    "mutation", ["role", "literal_only", "case", "part_arg", "rule", "id"]
)
def test_placeholder_form_is_closed(mutation: str) -> None:
    data = placeholder_form().model_dump(mode="json")
    if mutation == "role":
        data["signature"][0]["role"] = "selection_count"
    elif mutation == "literal_only":
        data["cases"]["up_to"] = [{"kind": "Literal", "text": "至多X"}]
    elif mutation == "case":
        data["cases"]["default"] = data["cases"].pop("up_to")
    elif mutation == "part_arg":
        data["cases"]["up_to"][1]["arg"] = "definition"
    else:
        data[mutation] = "unregistered"
    with pytest.raises(
        ValueError,
        match=r"[Pp]laceholder|[Cc]hoice|[Ee]xpression|[Ff]orm|[Ll]eaf|[Qq]uantity|[Ss]ignature",
    ):
        validate_form(FormDefinition.model_validate_json(canonical(data)))


def test_paid_context_choice_cannot_supply_an_independent_definition() -> None:
    raw = "仮。場の『仮甲』X枚を消滅:下記からX個までチョイスする。【1】1枚引く。【2】2枚引く。【3】3枚引く。"
    derived = derive_source(raw, source(raw), n3_classifier())
    frame, binding = derived.bind(0)
    name = next(
        s.name for s in frame.leaf_schema.slots if s.role == "choice_mode_count"
    )
    quantity = binding.values[name]
    assert isinstance(quantity, QuantitySpec)
    assert isinstance(quantity.expr, ContextExpression)
    assert all(s.role != "count_expression" for s in frame.leaf_schema.slots)
    form = placeholder_form()
    target = Target(
        format=1,
        nodes=(
            FormNode(
                kind="Form",
                form_id=form.id,
                args={
                    "quantity": SlotRef(slot=name),
                    "definition": SlotRef(slot=name),
                },
            ),
            LeafRef(kind="LeafRef", slot=name),
        ),
    )
    with pytest.raises(ValueError, match="leaf type disagrees"):
        target.referenced_slots(
            frame.leaf_schema, "zh-Hant", {(form.id, form.lang): form}
        )

"""Finite expressions use typed positional arguments and normal rendering evidence."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.four_layer import (
    Bound,
    Domain,
    Expression,
    FormDefinition,
    FormNode,
    LeafRef,
    LeafSchema,
    LeafSlot,
    LeafType,
    LiteralNode,
    Port,
    Projection,
    Scope,
    SlotRef,
    Target,
)
from sve_carddb.contracts.source_binding import verify_value
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_expressions import (
    DEFINITIONS,
    DOMAIN,
    DOMAIN_CODE,
    ExpressionTargets,
    PreparationRenderer,
    definition,
    verify_import,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from . import test_four_layer_render
from .test_four_layer_classification import classifier
from .test_four_layer_preparation import mixed_case

EXPRESSION_ID = "count.current_zone.owner.zone.v1"


def sample() -> tuple[BoundTarget, Renderer, ExpressionTargets]:
    _, frame = mixed_case()
    slots = (
        LeafSlot(
            name="src_owner",
            type="Player",
            role="read_owner",
            domain=Domain(values=("player.relative.v1",), min=None, max=None),
            required=True,
            occurrences=(),
        ),
        LeafSlot(
            name="src_zone",
            type="ZoneSet",
            role="read_zone",
            domain=Domain(values=("zone.single.v1",), min=None, max=None),
            required=True,
            occurrences=(),
        ),
        LeafSlot(
            name="count",
            type="QuantityExpr",
            role="count_expression",
            domain=Domain(values=(DOMAIN_CODE,), min=None, max=None),
            required=True,
            occurrences=(),
        ),
    )
    frame = frame.model_copy(update={"leaf_schema": LeafSchema(format=2, slots=slots)})
    base = test_four_layer_render.sample(
        Target(format=1, nodes=(LiteralNode(kind="Literal", text="甲"),))
    )
    binding = base.binding.model_copy(
        update={
            "frame_id": frame.id,
            "values": {
                "src_owner": "self",
                "src_zone": ("hand",),
                "count": Expression(
                    kind="expression",
                    expression_id=EXPRESSION_ID,
                    leaves=("src_owner", "src_zone"),
                ),
            },
        }
    )
    target = Target(
        format=1,
        nodes=(LeafRef(kind="LeafRef", slot="count"),),
    )
    item = BoundTarget(frame, binding, SelectedTarget(target))
    refs = Renderer({}, {}, {}).code_references
    labels = {}
    entries: tuple[tuple[LeafType, str, str], ...] = (
        ("Player", "self", "自己"),
        ("ZoneSet", "hand", "手牌"),
    )
    for type_name, code, text in entries:
        assert type_name in {"Player", "ZoneSet"}
        ref = refs[type_name, code]
        label = Label(ref, "zh-Hant", text, "project", False, True)
        labels[canonical(ref.model_dump(mode="json")), "zh-Hant"] = label
    renderer = Renderer(
        labels, {}, {}, domains={**classifier().domains, DOMAIN_CODE: DOMAIN}
    )
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=(
                LeafRef(kind="LeafRef", slot="owner"),
                LiteralNode(kind="Literal", text="的"),
                LeafRef(kind="LeafRef", slot="zone"),
                LiteralNode(kind="Literal", text="張數"),
            ),
        ),
        origin="machine",
        low_confidence=True,
    )
    return item, renderer, ExpressionTargets({(EXPRESSION_ID, "zh-Hant"): selected})


def test_expression_formals_bind_different_actual_names_and_propagate_render_evidence() -> (
    None
):
    item, renderer, targets = sample()
    prepared = PreparationRenderer(renderer, targets)
    result = prepared.render("ctx:synthetic", "zh-Hant", (item,))
    assert result.rendered is not None
    assert result.rendered.text == "自己的手牌張數"
    assert result.rendered.low_confidence
    assert result.rendered.origin == "machine"
    assert {leaf.slot for leaf in result.rendered.leaves} == {
        "src_owner",
        "src_zone",
        "count",
    }
    assert any(
        leaf.node_path == (0, 0) and leaf.slot == "src_owner"
        for leaf in result.rendered.leaves
    )
    assert len(result.rendered.annotation.occurrences) == 2
    assert prepared.required_references(item, "zh-Hant")
    with pytest.raises(ValueError, match="required"):
        renderer.render("ctx:synthetic", "zh-Hant", (item,))
    missing = PreparationRenderer(renderer, ExpressionTargets({})).render(
        "ctx:synthetic", "zh-Hant", (item,)
    )
    assert missing.issues == ("missing_expression_translation",)


@pytest.mark.parametrize(
    "change", ["type", "role", "domain", "arity", "unknown", "missing_value", "cycle"]
)
def test_expression_rejects_wrong_arguments_and_dependency_graphs(change: str) -> None:
    item, _, _ = sample()
    value = item.binding.values["count"]
    assert isinstance(value, Expression)
    frame, binding = item.frame, item.binding
    if change in {"type", "role", "domain"}:
        update: dict[str, object] = {
            "type": "Phase",
            "role": "source_owner",
            "domain": Domain(values=("player.self.v1",), min=None, max=None),
        }
        frame = frame.model_copy(
            update={
                "leaf_schema": frame.leaf_schema.model_copy(
                    update={
                        "slots": (
                            frame.leaf_schema.slots[0].model_copy(
                                update={change: update[change]}
                            ),
                            *frame.leaf_schema.slots[1:],
                        )
                    }
                )
            }
        )
    else:
        values = dict(binding.values)
        if change == "arity":
            values["count"] = value.model_copy(update={"leaves": ("src_owner",)})
        elif change == "unknown":
            values["count"] = value.model_copy(
                update={"expression_id": "arbitrary.read.v1"}
            )
        elif change == "missing_value":
            values.pop("src_owner")
        else:
            values["src_owner"] = value
        binding = binding.model_copy(update={"values": values})
    with pytest.raises(ValueError, match=r"[Ee]xpression|[Cc]yclic"):
        verify_quantities(frame, binding, scopes={})


def test_constant_domains_do_not_accept_new_expression_values() -> None:
    item, _, _ = sample()
    slot = item.frame.leaf_schema.slots[-1]
    expr = item.binding.values["count"]
    verify_value(slot, expr, {DOMAIN_CODE: DOMAIN})
    with pytest.raises(ValueError, match="Unknown"):
        verify_value(slot, expr, classifier().domains)
    with pytest.raises(ValueError, match="Unknown"):
        definition(
            Expression(
                kind="expression",
                expression_id="unsealed.history.v1",
                leaves=("src_owner",),
            )
        )
    with pytest.raises(ValueError, match="unknown"):
        ExpressionTargets(
            {
                ("unsealed.history.v1", "zh-Hant"): SelectedTarget(
                    Target(format=1, nodes=(LiteralNode(kind="Literal", text="甲"),))
                )
            }
        )
    assert all(
        rule.read_structure == "current_zone_count"
        and rule.result_measure == "card_count"
        for rule in DEFINITIONS.values()
    )


def test_imports_require_source_scope_and_cannot_cross_sibling_branches() -> None:
    projection = Projection(
        projection_kind="ability_body",
        discriminator="synthetic.v1",
        scopes=(
            Scope(id="ability", parent=None, kind="ability"),
            Scope(id="left", parent="ability", kind="branch"),
            Scope(id="right", parent="ability", kind="branch"),
        ),
        imports=(
            Port(
                name="x",
                type="QuantityExpr",
                source_role="count_expression",
                scope="left",
            ),
        ),
        exports=(),
    )
    verify_import(projection, "x", "left")
    for name, scope in (("missing", "left"), ("x", "right"), ("x", "absent")):
        with pytest.raises(ValueError, match=r"[Ee]xpression|[Cc]yclic"):
            verify_import(projection, name, scope)
    with pytest.raises(ValueError, match="mistyped"):
        verify_import(
            projection.model_copy(
                update={
                    "imports": (
                        projection.imports[0].model_copy(
                            update={"type": "ChoiceContext"}
                        ),
                    )
                }
            ),
            "x",
            "left",
        )
    item, _, _ = sample()
    binding = item.binding.model_copy(
        update={
            "values": {"count": Bound.model_validate({"kind": "bound", "import": "x"})}
        }
    )
    frame = item.frame.model_copy(update={"projection": projection})
    verify_quantities(frame, binding, scopes={"count": "left"})
    with pytest.raises(ValueError, match="scope"):
        verify_quantities(frame, binding, scopes={})


def test_expression_target_uses_normal_form_validation_and_actual_slot_substitution() -> (
    None
):
    item, _, _ = sample()
    expr = item.binding.values["count"]
    assert isinstance(expr, Expression)
    form = FormDefinition.model_validate(
        {
            "id": "owner.read",
            "lang": "zh-Hant",
            "rule": "synthetic.read.v1",
            "signature": ({"name": "player", "type": "Player", "role": "read_owner"},),
            "cases": {"default": ({"kind": "Label", "arg": "player"},)},
        }
    )
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=(
                FormNode(
                    kind="Form",
                    form_id="owner.read",
                    args={"player": SlotRef(slot="owner")},
                ),
                LeafRef(kind="LeafRef", slot="zone"),
            ),
        )
    )
    targets = ExpressionTargets({(EXPRESSION_ID, "zh-Hant"): selected})
    expanded = targets.expand(
        expr, item.frame.leaf_schema, "zh-Hant", {("owner.read", "zh-Hant"): form}
    )
    node = expanded.target.nodes[0]
    assert isinstance(node, FormNode)
    assert node.args["player"].slot == "src_owner"
    with pytest.raises(ValueError, match="form"):
        targets.expand(expr, item.frame.leaf_schema, "zh-Hant", {})
    incomplete = replace(
        selected,
        target=Target(format=1, nodes=(LiteralNode(kind="Literal", text="缺參數"),)),
    )
    with pytest.raises(ValueError, match="required"):
        ExpressionTargets({(EXPRESSION_ID, "zh-Hant"): incomplete}).expand(
            expr, item.frame.leaf_schema, "zh-Hant", {}
        )


def test_offset_is_a_typed_magnitude_and_unsealed_target_shapes_are_rejected() -> None:
    item, renderer, _ = sample()
    expr = Expression(
        kind="expression",
        expression_id="count.current_zone.owner.zone.subtract_offset.v1",
        leaves=("src_owner", "src_zone", "magnitude"),
    )
    slot = LeafSlot(
        name="magnitude",
        type="Nat",
        role="arithmetic_offset",
        domain=Domain(values=(), min=0, max=9007199254740991),
        required=True,
        occurrences=(),
    )
    schema = item.frame.leaf_schema.model_copy(
        update={"slots": (*item.frame.leaf_schema.slots, slot)}
    )
    assert definition(expr).arguments(expr, schema)["offset"].name == "magnitude"
    selected = SelectedTarget(
        Target.model_validate(
            {
                "format": 1,
                "nodes": (
                    {
                        "kind": "NP",
                        "constructor": "CountExpr",
                        "args": {"expr": {"slot": "owner"}},
                    },
                ),
            }
        )
    )
    targets = ExpressionTargets({(expr.expression_id, "zh-Hant"): selected})
    with pytest.raises(ValueError, match="only Literal"):
        targets.expand(expr, schema, "zh-Hant", {})
    old_frame = item.frame.model_copy(
        update={
            "source": item.frame.source.model_copy(
                update={"normalizer_version": "four-layer-jp-v2"}
            )
        }
    )
    with pytest.raises(ValueError, match="requires a v3"):
        PreparationRenderer(renderer, ExpressionTargets({})).render(
            "ctx:synthetic", "zh-Hant", (replace(item, frame=old_frame),)
        )

"""Mode preparation proves source reachability without implementing runtime history."""

from dataclasses import replace
from typing import Literal

import pytest

from sve_carddb.contracts.four_layer import (
    ChoiceContext,
    Constant,
    LeafRef,
    LinkedCondition,
    LiteralNode,
    PendingCondition,
    PredicateExpr,
    Projection,
    ResolvedCondition,
    Scope,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_analysis import analyze_source
from sve_carddb.domains.translations.four_layer_context import verify_mode_count
from sve_carddb.domains.translations.four_layer_derivation import (
    DerivedField,
    derive_source,
)
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_identity import RESOLVED
from sve_carddb.domains.translations.four_layer_mode_context import FAMILY, mode_sources
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.four_layer_semantics import Classified

from .context_fixtures import contract_case
from .expression_fixtures import expression_plan
from .test_four_layer_classification import source
from .test_four_layer_n1 import n1_classifier

_INTRO = "下記から1つチョイスする。"
_CONDITION = "自分の墓場が8枚以上なら、代わりに2つまで。"
_OPTIONS = "【1】仮甲。【2】仮乙。"


def prepare(raw: str) -> DerivedField:
    engine = n1_classifier()
    prepared = derive_source(raw, source(raw), engine)
    for index, found in enumerate(prepared.derived.recognized):
        assert not found.issues
        prepared.verify(raw, engine, *prepared.bind(index))
    return prepared


def test_cross_line_choice_keeps_constants_and_exact_source_edges() -> None:
    raw = _INTRO + _CONDITION + "\n" + _OPTIONS
    prepared = prepare(raw)
    (group,) = mode_sources(raw, prepared)
    assert group.members == (0, 2)
    (intro,) = prepared.mode_rules[0]
    (options,) = prepared.mode_rules[2]
    assert intro.base.mode == "exact"
    assert isinstance(intro.base.expr, Constant)
    assert intro.base.expr.value == 1
    assert intro.alternative.mode == "up_to"
    assert isinstance(intro.alternative.expr, Constant)
    assert intro.alternative.expr.value == 2
    assert isinstance(intro.condition, ResolvedCondition)
    assert isinstance(options.condition, LinkedCondition)
    assert options.condition.condition == intro.condition
    assert prepared.interfaces[0] == prepared.interfaces[2]


@pytest.mark.parametrize(
    ("condition", "reason"),
    [
        ("自分の場に仮の存在がいるなら、", "unsealed_source"),
        ("このターン、相手のフォロワーが場に出ていたなら、", "unsealed_source"),
    ],
)
def test_pending_conditions_retain_source_and_provider_distinction(
    condition: str, reason: str
) -> None:
    raw = _INTRO + condition + "代わりに2つまで。\n" + _OPTIONS
    prepared = prepare(raw)
    (rule,) = prepared.mode_rules[0]
    assert isinstance(rule.condition, PendingCondition)
    assert (
        raw[rule.condition.source_span.start : rule.condition.source_span.end]
        == condition
    )
    assert rule.condition.reason == reason


def test_faceup_evolve_condition_reuses_the_counted_operands() -> None:
    raw = (
        _INTRO
        + "自分のエボルヴデッキの表向きの『仮場手札自分』が8枚以上なら、代わりに2つまで。\n"
        + _OPTIONS
    )
    plain = analyze_source(raw, source(raw), n1_classifier()).recognized[0]
    prepared = prepare(raw)
    (rule,) = prepared.mode_rules[0]
    assert isinstance(rule.condition, ResolvedCondition)
    found = prepared.derived.recognized[0]
    predicate = found.values[rule.condition.predicate]
    assert isinstance(predicate, PredicateExpr)
    assert predicate.expression_id == "choice.condition.faceup_evolve.at_least.v1"
    before = [(s.role, plain.values[s.name]) for s in plain.schema.slots]
    operands = [
        (s.role, found.values[s.name])
        for s in found.schema.slots
        if s.name in predicate.leaves
    ]
    assert operands == before
    roles = {role for role, _ in before}
    assert sum(s.role in roles for s in found.schema.slots) == len(before)


@pytest.mark.parametrize(
    "raw",
    [
        _OPTIONS,
        _INTRO + _CONDITION + "\n【2】仮甲。【1】仮乙。",
        _INTRO + _INTRO + _CONDITION + "\n" + _OPTIONS,
        _INTRO + _CONDITION + "\n" + _INTRO + _CONDITION + "\n" + _OPTIONS,
        _INTRO
        + _CONDITION
        + "\n"
        + _OPTIONS
        + "\n"
        + _INTRO
        + _CONDITION
        + "\n"
        + _OPTIONS,
        _INTRO + _CONDITION + "\n\n" + _OPTIONS,
        _INTRO + _CONDITION + "\n{起動}" + _OPTIONS,
        _INTRO + _CONDITION + "\n【1】仮甲。{ファンファーレ}【2】仮乙。",
        "「" + _INTRO + _CONDITION + "\n" + _OPTIONS + "」を持つ。",
        _INTRO + "代わりに好きな数まで。\n" + _OPTIONS,
        _INTRO
        + "「このターン、この能力でチョイスした選択肢はチョイスできない。」"
        + _OPTIONS,
    ],
)
def test_neighboring_choice_shapes_are_not_admitted(raw: str) -> None:
    prepared = derive_source(raw, source(raw), n1_classifier())
    assert mode_sources(raw, prepared) == ()
    assert not any(prepared.mode_rules)


def test_optional_payment_is_linked_only_to_the_adjacent_unique_producer() -> None:
    raw = (
        "これをプレイする際、コストを+7してよい。\n"
        + _INTRO
        + "コストを+7したなら、代わりに2つまで。\n"
        + _OPTIONS
    )
    prepared = prepare(raw)
    (group,) = mode_sources(raw, prepared)
    assert group.cost is not None
    assert group.members == (0, 2, 4)
    interface = prepared.interfaces[2]
    assert interface is not None
    payment = next(p for p in interface.imports if p.name == "mode_payment")
    assert payment.evidence is not None
    assert payment.evidence.source.line_ordinal == 0
    assert prepared.interfaces[0] is not None
    assert payment in prepared.interfaces[0].exports
    assert isinstance(prepared.mode_rules[2][0].condition, ResolvedCondition)
    missing_raw = raw.replace("+7してよい", "+6してよい")
    missing = derive_source(missing_raw, source(missing_raw), n1_classifier())
    assert (
        mode_sources(raw.replace("+7してよい", "+6してよい"), missing)[0].cost is None
    )
    assert isinstance(missing.mode_rules[2][0].condition, PendingCondition)


def test_mode_history_carries_ability_instance_mode_and_turn_without_name_sharing() -> (
    None
):
    raw = (
        _INTRO
        + "このターン、この能力でチョイスした選択肢はチョイスできない。"
        + _OPTIONS
    )
    prepared = prepare(raw)
    frame, binding = prepared.bind(0)
    predicate = next(v for v in binding.values.values() if isinstance(v, PredicateExpr))
    assert predicate.expression_id == "mode.unused_this_turn.v1"
    port = next(p for p in frame.projection.imports if p.name == "mode_choice")
    assert port.evidence is not None
    context = port.evidence.value
    assert isinstance(context, ChoiceContext)
    assert context.modes == ("mode_1", "mode_2")
    assert context != context.model_copy(
        update={"ability_instance": "another-instance"}
    )
    assert context != context.model_copy(update={"turn": "another-turn"})
    changed = port.model_copy(
        update={
            "evidence": port.evidence.model_copy(
                update={
                    "value": context.model_copy(
                        update={"ability_instance": "regranted-instance"}
                    )
                }
            )
        }
    )
    with pytest.raises(ValueError, match="construction"):
        prepared.verify(
            raw,
            n1_classifier(),
            frame.model_copy(
                update={
                    "projection": frame.projection.model_copy(
                        update={"imports": (changed,)}
                    )
                }
            ),
            binding,
        )


@pytest.mark.parametrize("family", ["A1", "A2", "A11"])
def test_mode_rule_accepts_the_sealed_typed_provider_contract(
    family: Literal["A1", "A2", "A11"],
) -> None:
    prepared = prepare(_INTRO + _CONDITION + "\n" + _OPTIONS)
    case = contract_case(family)
    rule = prepared.mode_rules[0][0].model_copy(
        update={
            "condition": case.mode_rule.condition,
            "choice_context": case.mode_rule.choice_context,
            "scope": case.mode_rule.scope,
        }
    )
    frame = case.item.frame.model_copy(update={"mode_rules": (rule,)})
    verify_quantities(frame, case.item.binding, scopes={})
    verify_mode_count(rule, frame, case.item.binding)
    with pytest.raises(ValueError, match="scope"):
        verify_mode_count(
            rule.model_copy(update={"scope": "foreign_ability"}),
            frame,
            case.item.binding,
        )


def test_finite_condition_renders_its_required_operands_with_annotations() -> None:
    prepared = prepare(_INTRO + _CONDITION + "\n" + _OPTIONS)
    frame, binding = prepared.bind(0)
    labels = {}
    refs = Renderer({}, {}, {}).code_references
    for ref, text in [
        (refs["Player", "self"], "自己"),
        (refs["ZoneSet", "graveyard"], "墓場"),
    ]:
        label = Label(ref, "zh-Hant", text, "project", False, True)
        labels[canonical(ref.model_dump(mode="json")), "zh-Hant"] = label
    renderer = Renderer(
        labels, {}, {}, domains={**n1_classifier().domains, **prepared.registry.domains}
    )
    targets = ExpressionTargets(
        {
            ("choice.condition.zone.at_least.v1", "zh-Hant"): SelectedTarget(
                Target(
                    format=1,
                    nodes=(
                        LeafRef(kind="LeafRef", slot="owner"),
                        LeafRef(kind="LeafRef", slot="zone"),
                        LeafRef(kind="LeafRef", slot="threshold"),
                        LiteralNode(kind="Literal", text="以上"),
                    ),
                )
            )
        },
        registry=prepared.registry,
    )
    condition = next(s for s in frame.leaf_schema.slots if s.type == "PredicateExpr")
    expression = binding.values[condition.name]
    assert isinstance(expression, PredicateExpr)
    operands = set(expression.leaves)
    target = Target(
        format=1,
        nodes=tuple(
            LeafRef(kind="LeafRef", slot=s.name)
            for s in frame.leaf_schema.slots
            if s.name not in operands
        ),
    )
    item = BoundTarget(frame, binding, SelectedTarget(target))
    prepared.verify(
        _INTRO + _CONDITION + "\n" + _OPTIONS,
        n1_classifier(),
        frame,
        binding,
        target=target,
    )
    result = renderer.render("synthetic", "zh-Hant", expression_plan((item,), targets))
    assert result.rendered is not None
    assert "自己墓場至少8以上" in result.rendered.text
    assert len(result.rendered.annotation.occurrences) == 2
    missing = replace(
        item,
        selected=SelectedTarget(
            Target(format=1, nodes=(LiteralNode(kind="Literal", text="仮"),))
        ),
    )
    with pytest.raises(ValueError, match="required"):
        renderer.render("synthetic", "zh-Hant", expression_plan((missing,), targets))


def test_leader_life_condition_does_not_borrow_a_card_set_owner_role() -> None:
    prepared = prepare(
        _INTRO + "相手のリーダーの体力が8以下なら、代わりに2つまで。\n" + _OPTIONS
    )
    frame, binding = prepared.bind(0)
    owner = next(slot for slot in frame.leaf_schema.slots if slot.type == "Player")
    assert owner.role == "mode_condition_owner"
    assert binding.values[owner.name] == "opponent"
    changed = frame.model_copy(
        update={
            "leaf_schema": frame.leaf_schema.model_copy(
                update={
                    "slots": tuple(
                        slot.model_copy(update={"role": "counted_owner"})
                        if slot.name == owner.name
                        else slot
                        for slot in frame.leaf_schema.slots
                    )
                }
            )
        }
    )
    with pytest.raises(ValueError, match="role"):
        verify_quantities(changed, binding, scopes={}, registry=prepared.registry)


def test_mode_family_preserves_existing_semantics_and_projection_kind(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setitem(RESOLVED, "synthetic.existing.v1", {"kind": "synthetic"})
    raw = _INTRO + _CONDITION + "\n" + _OPTIONS
    base = analyze_source(raw, source(raw), n1_classifier())
    original = DerivedField(
        base,
        tuple(None for _ in base.field.parts),
        source_domains=n1_classifier().domains,
        families=(FAMILY,),
    )
    projection = Projection(
        projection_kind="ability_body",
        discriminator="synthetic.existing.v1",
        scopes=(Scope(id="synthetic", parent=None, kind="ability"),),
        imports=(),
        exports=(),
    )
    semantics = Classified("synthetic.existing.v1", projection)
    original = replace(
        original,
        derived=replace(
            original.derived,
            recognized=(
                replace(original.derived.recognized[0], semantics=semantics),
                *original.derived.recognized[1:],
            ),
        ),
    )
    prepared = FAMILY.prepare(raw, original, n1_classifier(), None)
    frame, _ = prepared.bind(0)
    assert frame.projection.projection_kind == projection.projection_kind
    assert frame.projection.discriminator == projection.discriminator
    assert frame.semantic_variant.key == semantics.key
    assert Projection.model_validate(frame.projection.model_dump()) == frame.projection


def test_independent_ability_heads_keep_distinct_choice_contexts() -> None:
    raw = (
        "{ファンファーレ}"
        + _INTRO
        + _CONDITION
        + "\n"
        + _OPTIONS
        + "\n{起動}"
        + _INTRO
        + _CONDITION
        + "\n"
        + _OPTIONS
    )
    prepared = prepare(raw)
    groups = mode_sources(raw, prepared)
    assert len(groups) == 2
    first = prepared.interfaces[groups[0].members[0]]
    second = prepared.interfaces[groups[1].members[0]]
    assert first is not None
    assert second is not None
    first_context = next(p.evidence for p in first.imports if p.name == "mode_choice")
    second_context = next(p.evidence for p in second.imports if p.name == "mode_choice")
    assert first_context is not None
    assert second_context is not None
    assert first_context.value != second_context.value


@pytest.mark.parametrize(
    ("prefix", "reason"),
    [
        (
            "これをプレイする際、コストを+7してよい。\nこれをプレイする際、コストを+7してよい。\n",
            "ambiguous_source",
        ),
        (
            "「仮の規則\nこれをプレイする際、コストを+7してよい。\n」",
            "missing_provider",
        ),
    ],
)
def test_optional_cost_cannot_borrow_an_ambiguous_or_quoted_producer(
    prefix: str, reason: str
) -> None:
    raw = prefix + _INTRO + "コストを+7したなら、代わりに2つまで。\n" + _OPTIONS
    prepared = derive_source(raw, source(raw), n1_classifier())
    (group,) = mode_sources(raw, prepared)
    assert group.cost is None
    rules = [rule for rs in prepared.mode_rules for rule in rs]
    assert any(
        isinstance(rule.condition, PendingCondition) and rule.condition.reason == reason
        for rule in rules
    )

"""Aggregate preparation preserves operands and rejects unsealed pair/general math shapes."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    ContextExpression,
    Expression,
    Frame,
    LeafRef,
    LiteralNode,
    ObjectGeneration,
    PredicateExpr,
    Target,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    PreparationRenderer,
    target_dependencies,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_preparation import (
    PreparedField,
    derive_preparation,
)
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.four_layer_total_sources import FAMILY
from sve_carddb.domains.translations.four_layer_totals import (
    disjoint_pair_count,
    lowering,
)

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import SourceBinding

from .test_four_layer_classification import classifier, source, type_vocabulary


def engine() -> Classifier:
    return classifier(
        (
            Term("term:object.card", "rule_term", "カード"),
            Term("term:trait.synthetic_a", "trait", "合成甲"),
            Term("term:trait.synthetic_b", "trait", "合成乙"),
            Term("term:trait.mage", "trait", "合成術者"),
            Term("term:name.blaster_blade", "card_name", "架空甲"),
            Term("term:name.blaster_dark", "card_name", "架空乙"),
        ),
        vocabulary=type_vocabulary(),
    )


def prepared(raw: str) -> tuple[PreparedField, Frame, SourceBinding]:
    parser = engine()
    result = derive_preparation(raw, source(raw), parser, families=(FAMILY,))
    frame, binding = result.bind(0)
    result.verify(raw, parser, frame, binding)
    return result, frame, binding


@pytest.mark.parametrize(
    ("comparison", "code"), [("以上", "ge"), ("以下", "le"), ("", "eq")]
)
@pytest.mark.parametrize("position", ["selection", "payment", "condition"])
def test_bounds_keep_direction_threshold_property_and_position(
    comparison: str, code: str, position: str
) -> None:
    if position == "selection":
        raw = f"自分の墓場の合成甲・フォロワーを元のコストの合計が7{comparison}になるように2枚まで選ぶ。それを手札に戻す。"
    elif position == "payment":
        raw = f"場のフォロワーを元のコストの合計が7{comparison}になるように好きな枚数{{アクト}}：合成処理を行う。"
    else:
        raw = f"自分の場の合成甲・カードの元のコストの合計が7{comparison}なら、自分のデッキから3枚引く。"
    result, frame, binding = prepared(raw)
    predicate = next(v for v in binding.values.values() if isinstance(v, PredicateExpr))
    assert predicate.expression_id == f"aggregate.bound.{position}.{code}.v1"
    summed = binding.values[predicate.leaves[0]]
    assert isinstance(summed, ContextExpression)
    assert result.registry.result_measure(summed, binding) == "cost"
    assert binding.values[predicate.leaves[1]] == 7
    before = derive_preparation(raw, source(raw), engine())
    assert before.derived.recognized[0].schema != frame.leaf_schema
    assert before.families == ()


def test_earlier_aggregate_keeps_later_sealed_count_actuals() -> None:
    _, _, binding = prepared(
        "自分の場の合成甲・カードの元のコストの合計が7以上なら、それにXダメージ。Xは「自分の手札の枚数」である。"
    )
    count = next(v for v in binding.values.values() if isinstance(v, Expression))
    assert [binding.values[n] for n in count.leaves] == ["self", ("hand",)]


def test_attack_sum_keeps_original_cost_filter_separate_from_bound() -> None:
    _, frame, binding = prepared(
        "自分の場の元のコスト2のフォロワーの攻撃力の合計が13以上なら、自分のデッキから3枚引く。"
    )
    values = [
        binding.values[s.name]
        for s in frame.leaf_schema.slots
        if s.role in {"aggregate_cost", "aggregate_threshold"}
    ]
    assert values == [2, 13]
    assert any(
        isinstance(v, ContextExpression) and v.expression_id.endswith(".attack.v1")
        for v in binding.values.values()
    )


@pytest.mark.parametrize(
    "selector",
    [
        "自分の場と自分の手札のカードの枚数の合計",
        "自分の墓場の合成甲・カードやスペルの枚数の合計",
        "自分の墓場の『架空甲』『架空乙』の合計+3",
    ],
)
def test_totals_keep_two_count_operands_and_explicit_lowering_gap(
    selector: str,
) -> None:
    result, frame, binding = prepared(
        f"自分のデッキからX枚引く。Xは「{selector}」である。"
    )
    total = next(
        v
        for v in binding.values.values()
        if isinstance(v, ContextExpression)
        and v.expression_id.startswith("aggregate.total.")
    )
    left, right = (binding.values[n] for n in total.leaves[:2])
    assert isinstance(left, ContextExpression)
    assert isinstance(right, ContextExpression)
    assert left.leaves != right.leaves
    skeleton = lowering(total.expression_id, total.leaves)
    assert skeleton.operands == total.leaves
    assert not skeleton.executable
    if "や" in selector:
        assert skeleton.unresolved == ("overlapping_total",)
    verify_quantities(frame, binding, scopes={}, registry=result.registry)


@pytest.mark.parametrize(
    "noun", ["『架空甲』と『架空乙』", "合成術者・フォロワーと合成術者・スペル"]
)
def test_pair_signatures_preserve_independent_selectors(noun: str) -> None:
    result, _, binding = prepared(
        f"自分の墓場の{noun}各1枚1組につき、自分のデッキから2枚引く。"
    )
    pair = next(v for v in binding.values.values() if isinstance(v, ContextExpression))
    rule = result.registry.definition(pair)
    assert tuple(p.name for p in rule.parameters if p.type == "Player") == (
        "owner_left",
        "owner_right",
    )
    assert tuple(p.name for p in rule.parameters if p.type == "ZoneSet") == (
        "zone_left",
        "zone_right",
    )
    assert "min(left_count,right_count)" in (
        lowering(pair.expression_id, pair.leaves).pair_policy or ""
    )


@pytest.mark.parametrize(
    ("sizes", "expected"), [((0, 0), 0), ((0, 3), 0), ((1, 4), 1), ((5, 2), 2)]
)
def test_pair_count_uses_minimum_with_empty_and_unequal_groups(
    sizes: tuple[int, int], expected: int
) -> None:
    left = tuple(
        ObjectGeneration(object_id=f"left-{i}", generation=1) for i in range(sizes[0])
    )
    right = tuple(
        ObjectGeneration(object_id=f"right-{i}", generation=1) for i in range(sizes[1])
    )
    assert disjoint_pair_count(left, right) == expected
    if left:
        with pytest.raises(ValueError, match="disjoint"):
            disjoint_pair_count(left, left)
        with pytest.raises(ValueError, match="at most once"):
            disjoint_pair_count((*left, left[0]), right)


@pytest.mark.parametrize(
    "raw",
    [
        "自分の墓場の『架空甲』と『架空甲』各1枚1組につき、2枚引く。",
        "自分の墓場の『未登録名』と『架空乙』各1枚1組につき、2枚引く。",
        "自分の墓場の合成甲・カードと合成甲・スペル各1枚1組につき、2枚引く。",
        "自分の墓場の『架空甲』と『架空乙』と『架空甲』各1枚1組につき、2枚引く。",
        "自分の墓場の『架空甲』と『架空乙』各2枚1組につき、2枚引く。",
        "相手の墓場の『架空甲』と『架空乙』各1枚1組につき、2枚引く。",
        "自分の手札の『架空甲』と『架空乙』各1枚1組につき、2枚引く。",
        "自分のデッキからX枚引く。Xは「自分の場と手札とEXエリアのカードの合計」である。",
        "自分のデッキからX枚引く。Xは「自分の墓場のカードの合計*2」である。",
        "自分のデッキからX枚引く。【攻撃時】Xは「自分の場と手札のカードの合計」である。",
    ],
)
def test_unsealed_shapes_preserve_preparation_result(raw: str) -> None:
    parser = engine()
    before = derive_preparation(raw, source(raw), parser)
    after = derive_preparation(raw, source(raw), parser, families=(FAMILY,))
    assert replace(after, families=()) == before


def test_required_references_expand_with_normal_annotations_and_missing_labels_fail() -> (
    None
):
    raw = "自分の墓場の『架空甲』と『架空乙』各1枚1組につき、自分のデッキから2枚引く。"
    result, frame, binding = prepared(raw)
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=tuple(
                LeafRef(kind="LeafRef", slot=s.name)
                for s in frame.leaf_schema.slots
                if s.required
                and s.role
                not in {"aggregate_name", "aggregate_owner", "aggregate_zone"}
            ),
        )
    )
    expression_targets = {}
    for value in binding.values.values():
        if not isinstance(value, (ContextExpression, PredicateExpr)):
            continue
        rule = result.registry.definition(value)
        expression_targets[value.expression_id, "zh-Hant"] = SelectedTarget(
            Target(
                format=1,
                nodes=tuple(
                    LeafRef(kind="LeafRef", slot=p.name) for p in rule.parameters
                ),
            )
        )
    labels = {}
    for value in binding.values.values():
        if isinstance(value, CardNameReference):
            labels[canonical(value.model_dump(mode="json")), "zh-Hant"] = Label(
                value, "zh-Hant", "測試名稱", "project", False, True
            )
    for reference in Renderer({}, {}, {}).code_references.values():
        labels[canonical(reference.model_dump(mode="json")), "zh-Hant"] = Label(
            reference, "zh-Hant", "測試區域", "project", False, False
        )
    renderer = PreparationRenderer(
        Renderer(
            labels, Renderer({}, {}, {}).code_references, {}, domains=engine().domains
        ),
        ExpressionTargets(expression_targets, registry=result.registry),
    )
    item = BoundTarget(frame, binding, selected)
    rendered = renderer.render("synthetic", "zh-Hant", (item,))
    assert rendered.rendered is not None
    assert "測試名稱" in rendered.rendered.text
    assert rendered.rendered.annotation.occurrences
    assert rendered.issues == ()
    with pytest.raises(ValueError, match="required"):
        Target(format=1, nodes=(LiteralNode(kind="Literal", text="省略引用"),)).verify(
            frame.leaf_schema,
            "zh-Hant",
            {},
            dependencies=target_dependencies(frame, binding, registry=result.registry),
        )
    missing = PreparationRenderer(
        Renderer(
            {}, Renderer({}, {}, {}).code_references, {}, domains=engine().domains
        ),
        ExpressionTargets(expression_targets, registry=result.registry),
    ).render("synthetic", "zh-Hant", (item,))
    assert missing.rendered is None


def test_pair_signature_rejects_substituting_another_known_name() -> None:
    result, frame, binding = prepared(
        "自分の墓場の『架空甲』と『架空乙』各1枚1組につき、自分のデッキから2枚引く。"
    )
    names = [s.name for s in frame.leaf_schema.slots if s.type == "CardName"]
    changed = binding.model_copy(
        update={"values": binding.values | {names[1]: binding.values[names[0]]}}
    )
    with pytest.raises(ValueError, match="sealed descriptor"):
        verify_quantities(frame, changed, scopes={}, registry=result.registry)


def test_missing_pair_name_remains_pending_without_literal_or_optional_operands() -> (
    None
):
    raw = (
        "自分の墓場の『未登録名』と『架空乙』各1枚1組につき、自分のデッキから2枚引く。"
    )
    result = derive_preparation(raw, source(raw), engine(), families=(FAMILY,))
    assert "missing_card_name_concept" in result.derived.recognized[0].issues
    with pytest.raises(ValueError, match="Unresolved"):
        result.bind(0)

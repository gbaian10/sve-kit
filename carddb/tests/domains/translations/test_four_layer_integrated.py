"""Real family providers compose through source occurrences, never fixture ports."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    ContextExpression,
    Frame,
    GlossaryReference,
    LeafRef,
    LinkedCondition,
    LiteralNode,
    ObjectSetRef,
    PendingCondition,
    PredicateExpr,
    ResolvedCondition,
    Target,
    VocabularyReference,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations import four_layer_integrated, four_layer_names
from sve_carddb.domains.translations.four_layer_context import verify_mode_count
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import ExpressionTargets
from sve_carddb.domains.translations.four_layer_integrated import verify_claims
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .expression_fixtures import expression_plan
from .test_four_layer_classification import source
from .test_four_layer_event_sources import event_classifier
from .test_four_layer_names import engine

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField


def prepare(raw: str, classifier: Classifier | None = None) -> DerivedField:
    parser = classifier or engine()
    prepared = derive_source(raw, source(raw), parser)
    for i, found in enumerate(prepared.derived.recognized):
        assert not found.issues
        frame, binding = prepared.bind(i)
        prepared.verify(raw, parser, frame, binding)
        frame.verify(prepared.derived.field.parts[i].canonical_source)
        assert Frame.model_validate_json(frame.model_dump_json()) == frame
    return prepared


@pytest.mark.parametrize(
    "condition",
    [
        "これが墓場から場に出ていたなら、",
        "これが能力によって場に出ていたなら、",
        "これがウマ娘・カードの能力によって場に出ていたなら、",
        "このターン、自分の場のフォロワーが進化していたなら、",
    ],
)
def test_mode_condition_consumes_real_provider_and_links_options(
    condition: str,
) -> None:
    raw = (
        "下記から1つチョイスする。"
        + condition
        + "代わりに2つまで。\n【1】仮甲。【2】仮乙。"
    )
    prepared = prepare(raw, event_classifier())
    intro = prepared.mode_rules[0][0]
    assert isinstance(intro.condition, ResolvedCondition)
    options = [r for rules in prepared.mode_rules[1:] for r in rules]
    assert options
    for rule in options:
        assert isinstance(rule.condition, LinkedCondition)
        assert rule.condition.condition == intro.condition
        assert rule.scope == intro.scope


def test_unsealed_condition_stays_named_pending_across_link() -> None:
    prepared = prepare(
        "下記から1つチョイスする。自分の場に仮の存在がいるなら、代わりに2つまで。\n【1】仮甲。【2】仮乙。"
    )
    condition = prepared.mode_rules[0][0].condition
    assert isinstance(condition, PendingCondition)
    assert condition.reason == "unsealed_source"
    linked = next(r.condition for rules in prepared.mode_rules[1:] for r in rules)
    assert isinstance(linked, LinkedCondition)
    assert linked.condition == condition


@pytest.mark.parametrize(
    ("noun", "zone", "expected"),
    [
        (
            "「これと同名を除く仮族・カード」",
            "デッキ",
            "name.relative.holder.not_equal.v1",
        ),
        ("仮族・仮別族・フォロワー", "墓場", "traits.all2.v1"),
        (
            "「それぞれカード名が異なる仮族・仮別族・フォロワー」",
            "デッキ",
            "name.selection.distinct.v1",
        ),
    ],
)
def test_aggregate_consumes_name_predicates_once(
    noun: str, zone: str, expected: str
) -> None:
    prepared = prepare(
        f"自分の{zone}から{noun}を元のコストの合計が7以下になるように2枚選ぶ。"
    )
    _, binding = prepared.bind(0)
    predicates = {
        n: v for n, v in binding.values.items() if isinstance(v, PredicateExpr)
    }
    assert sum(v.expression_id == expected for v in predicates.values()) == 1
    summed = next(
        v
        for v in binding.values.values()
        if isinstance(v, ContextExpression)
        and v.expression_id.startswith("aggregate.sum.")
    )
    assert any(
        n in predicates and predicates[n].expression_id == expected
        for n in summed.leaves
    )
    assert not any(
        "exclude_name" in v.expression_id
        or "distinct_names" in v.expression_id
        or "traits_kind" in v.expression_id
        for v in binding.values.values()
        if isinstance(v, ContextExpression)
    )


def test_historical_name_exclusion_has_one_owner() -> None:
    prepared = prepare(
        "このターン中に自分の「これと同名を除く仮族・フォロワー」が場から手札に戻っていたなら、7枚引く。"
    )
    _, binding = prepared.bind(0)
    ids = [
        v.expression_id for v in binding.values.values() if isinstance(v, PredicateExpr)
    ]
    assert any("exclude_same_name" in identifier for identifier in ids)
    assert "name.relative.holder.not_equal.v1" not in ids


def test_history_read_does_not_gain_another_read_wrapper() -> None:
    prepared = prepare(
        "相手のフォロワー1体にXダメージ。Xは「このターン中に自分のリーダーの体力が減少した回数」である。"
    )
    _, binding = prepared.bind(0)
    reads = [v for v in binding.values.values() if isinstance(v, ContextExpression)]
    assert len(reads) == 1
    assert reads[0].expression_id.startswith("history.life_decrease.")


def test_distinct_name_count_preserves_definition_and_use_occurrences() -> None:
    prepared = prepare(
        "相手のフォロワー1体にXダメージ。Xは「自分の墓場にあるスペルのカード名の種類数」である。"
    )
    _, binding = prepared.bind(0)
    name, value = next(
        (n, v) for n, v in binding.values.items() if isinstance(v, ContextExpression)
    )
    assert value.expression_id == "name.distinct_count.objects.v1"
    assert len([o for o in binding.occurrences if o.slot == name]) == 3


@pytest.mark.parametrize(
    "head", ["【出走時】", "【ドライブ獲得時】", "{アドバンス起動}"]
)
def test_additional_ability_heads_prevent_x_borrowing(head: str) -> None:
    prepared = prepare(
        "{ファンファーレ}{コストX}：仮甲。" + head + "相手のフォロワー1体にXダメージ。"
    )
    _, binding = prepared.bind(0)
    read = next(
        (
            n
            for n, v in binding.values.items()
            if isinstance(v, ContextExpression)
            and v.expression_id == "read.parameter.chosen_x.v1"
        ),
        None,
    )
    assert read is not None
    assert len([o for o in binding.occurrences if o.slot == read]) == 1


def test_option_rename_keeps_the_read_port_source_leaf() -> None:
    prepared = prepare(
        "下記から1つチョイスする。自分の墓場が7枚以上なら、代わりに2つまで。\n"
        "【1】相手のフォロワー1体にXダメージ。Xは「自分のPP最大値」である。\n【2】仮乙。"
    )
    leaves = [
        binding.values[name]
        for frame, binding in (
            prepared.bind(i) for i in range(len(prepared.interfaces))
        )
        for port in frame.projection.imports
        if port.evidence
        for name in port.evidence.source_leaves
    ]
    assert leaves == ["self"]


def test_holder_symbols_agree_across_name_and_entry_providers() -> None:
    prepared = prepare(
        "これが手札以外から場に出ていたなら、自分の墓場の「これと同名を除く仮族・フォロワー」を選ぶ。"
    )
    frame, _ = prepared.bind(0)
    holders = [
        p.evidence.value.objects
        for p in frame.projection.imports
        if p.evidence
        and isinstance(p.evidence.value, ObjectSetRef)
        and p.evidence.value.origin == "holder"
    ]
    assert len(holders) == 2
    assert holders[0] == holders[1]


def test_duplicate_source_claim_is_rejected() -> None:
    prepared = prepare("自分のデッキから「これと同名を除く仮族・カード」7枚を探す。")
    found = prepared.derived.recognized[0]
    slot = next(s for s in found.schema.slots if s.type == "PredicateExpr")
    duplicate = slot.model_copy(update={"name": "duplicate"})
    forged = replace(
        found,
        schema=found.schema.model_copy(
            update={"slots": (*found.schema.slots, duplicate)}
        ),
        values={**found.values, "duplicate": found.values[slot.name]},
        occurrences=(
            *found.occurrences,
            *(
                o.model_copy(update={"slot": "duplicate"})
                for o in found.occurrences
                if o.slot == slot.name
            ),
        ),
    )
    after = replace(prepared, derived=replace(prepared.derived, recognized=(forged,)))
    with pytest.raises(ValueError, match=r"owned source construction|duplicate"):
        verify_claims(prepared, after)


@pytest.mark.parametrize(
    ("raw", "undo"),
    [
        (
            "このターン中に自分の「これと同名を除く仮族・フォロワー」が場から手札に戻っていたなら、7枚引く。",
            "history",
        ),
        (
            "自分のデッキから「これと同名を除く仮族・カード」を元のコストの合計が7以下になるように2枚選ぶ。",
            "aggregate",
        ),
    ],
)
def test_second_encoding_of_an_owned_construction_is_rejected(
    monkeypatch: pytest.MonkeyPatch, raw: str, undo: str
) -> None:
    if undo == "history":
        monkeypatch.setattr(four_layer_names, "name_filter_spans", lambda _: ())
    else:
        # Totals before names re-encode the exclusion in the aggregate noun shape.
        history, entry, names, totals, *rest = four_layer_integrated.PROVIDERS
        order = (history, entry, totals, names, *rest)
        monkeypatch.setattr(four_layer_integrated, "PROVIDERS", order)
    with pytest.raises(ValueError, match="one source construction twice"):
        derive_source(raw, source(raw), engine())


def test_linked_condition_rejects_a_different_source_occurrence() -> None:
    prepared = prepare(
        "下記から1つチョイスする。自分の墓場が7枚以上なら、代わりに2つまで。\n【1】仮甲。【2】仮乙。"
    )
    index = next(
        i
        for i, rules in enumerate(prepared.mode_rules)
        if rules and isinstance(rules[0].condition, LinkedCondition)
    )
    frame, binding = prepared.bind(index)
    rule = frame.mode_rules[0]
    assert isinstance(rule.condition, LinkedCondition)
    foreign = rule.condition.source.model_copy(
        update={"line_ordinal": rule.condition.source.line_ordinal + 1}
    )
    bad = rule.model_copy(
        update={"condition": rule.condition.model_copy(update={"source": foreign})}
    )
    with pytest.raises(ValueError, match="different source ability"):
        verify_mode_count(bad, frame, binding)


def test_composed_aggregate_renders_required_traits_in_complete_chinese_field() -> None:  # ruff: ignore[too-many-locals] -- keep derivation, required labels and whole-field fallback in one public-boundary case
    raw = "自分の墓場の仮族・仮別族・フォロワーの元のコストの合計が7以上なら、仮処理。"
    prepared = prepare(raw)
    frame, binding = prepared.bind(0)
    base = Renderer({}, {}, {}, domains=engine().domains)
    labels = {}
    for slot in frame.leaf_schema.slots:
        value = binding.values[slot.name]
        if isinstance(
            value, (GlossaryReference, VocabularyReference, CardNameReference)
        ):
            reference = value
        elif slot.type in {"Player", "ZoneSet"}:
            code = value[0] if isinstance(value, tuple) else value
            assert isinstance(code, str)
            reference = base.code_references[slot.type, code]
        else:
            continue
        labels[canonical(reference.model_dump(mode="json")), "zh-Hant"] = Label(
            reference, "zh-Hant", "合成詞", "project", False, True
        )
    selections = {}
    for value in binding.values.values():
        if isinstance(value, (ContextExpression, PredicateExpr)):
            rule = prepared.registry.definition(value)
            selections[value.expression_id, "zh-Hant"] = SelectedTarget(
                Target(
                    format=1,
                    nodes=tuple(
                        LeafRef(kind="LeafRef", slot=p.name) for p in rule.parameters
                    ),
                )
            )
    predicate = next(
        s for s in frame.leaf_schema.slots if s.role == "aggregate_condition_constraint"
    )
    target = Target(
        format=1,
        nodes=(
            LiteralNode(kind="Literal", text="若"),
            LeafRef(kind="LeafRef", slot=predicate.name),
            LiteralNode(kind="Literal", text="以上，執行合成處理。"),
        ),
    )
    parser = engine()
    prepared.verify(raw, parser, frame, binding, target=target)
    renderer = Renderer(
        labels, {}, {}, domains={**parser.domains, **prepared.registry.domains}
    )
    item = BoundTarget(frame, binding, SelectedTarget(target))
    result = renderer.render(
        "ctx:synthetic",
        "zh-Hant",
        expression_plan(
            (item,), ExpressionTargets(selections, registry=prepared.registry)
        ),
    ).rendered
    assert result is not None
    assert result.text.startswith("若")
    assert result.text.endswith("以上，執行合成處理。")
    assert "7" in result.text
    assert result.annotation.occurrences
    missing = next(
        k for k, label in labels.items() if label.reference.kind == "glossary"
    )
    without = {k: value for k, value in labels.items() if k != missing}
    missing_result = Renderer(
        without, {}, {}, domains={**parser.domains, **prepared.registry.domains}
    ).render(
        "ctx:synthetic",
        "zh-Hant",
        expression_plan(
            (item,), ExpressionTargets(selections, registry=prepared.registry)
        ),
    )
    assert missing_result.rendered is None
    assert missing_result.issues

"""Synthetic event sources exercise the public preparation and rendering boundaries."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    ChoiceContext,
    ContextExpression,
    EventRef,
    LeafRef,
    LiteralNode,
    ModeCountRule,
    ObjectGeneration,
    Port,
    PortEvidence,
    PredicateExpr,
    Target,
)
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_context import verify_mode_count
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    PreparationRenderer,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_history import HISTORY_FAMILY
from sve_carddb.domains.translations.four_layer_preparation import derive_preparation
from sve_carddb.domains.translations.four_layer_provenance import PROVENANCE_FAMILY
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Renderer,
    SelectedTarget,
)
from sve_carddb.domains.translations.recognition.rules import Rule

from .test_four_layer_classification import source
from .test_four_layer_expressions import sample
from .test_four_layer_n1 import n1_classifier

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_preparation import PreparedField

FAMILIES = (HISTORY_FAMILY, PROVENANCE_FAMILY)


def event_classifier() -> Classifier:
    """Adopt only structural vocabulary needed by the synthetic event examples."""
    base = n1_classifier()
    terms = (
        *base.terms.values(),
        Term("term:trait.umamusume", "trait", "ウマ娘"),
        Term("term:ability.union_burst", "ability", "UB"),
        Term("term:ability.earth_rite", "ability", "土の秘術"),
        Term("term:ability.stack", "ability", "スタック"),
    )
    expected: dict[str, list[tuple[str, str]]] = {}
    for term in terms:
        expected.setdefault(term.source_ja, []).append((term.id, term.category))
    rules = base.rules.model_copy(
        update={
            "rules": (
                *base.rules.rules,
                *(
                    Rule(
                        rule_id=key,
                        enabled=True,
                        origin="project",
                        low_confidence=False,
                    )
                    for key in ("bracket_choice_index", "suffix_unit_items")
                ),
            )
        }
    )
    return Classifier(terms, replace(base.references, terms=expected), rules)


def prepared_case(raw: str) -> tuple[PreparedField, Frame, SourceBinding]:
    """Use the same derive/bind/verify entry that private frozen fields use."""
    engine = event_classifier()
    prepared = derive_preparation(raw, source(raw), engine, families=FAMILIES)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    frame.verify(prepared.derived.field.parts[0].canonical_source)
    return prepared, frame, binding


@pytest.mark.parametrize(
    ("clause", "descriptor"),
    [
        ("これが手札から場に出ていたなら", "entry.self.from_zone.eq.v1"),
        ("これが手札以外から場に出ていたなら", "entry.self.from_zone.ne.v1"),
        ("これが墓場から場に出ていたなら", "entry.self.from_zone.eq.v1"),
        ("これが能力によって場に出ていたなら", "entry.self.method.ability.v1"),
        (
            "これがウマ娘・カードの能力によって場に出ていたなら",
            "entry.self.method.ability_source_trait.v1",
        ),
        ("それが墓場から場に出ていたなら", "entry.event_subject.from_zone.eq.v1"),
        ("これがEXエリアからプレイされていたなら", "play.self.from_zone.eq.v1"),
        ("これを手札からプレイしていたなら", "play.self.from_zone.eq.v1"),
    ],
)
def test_entry_and_play_sources_are_distinct(clause: str, descriptor: str) -> None:
    raw = (
        (
            "自分の場にフォロワーが出たとき、"
            if descriptor.startswith("entry.event_subject")
            else ""
        )
        + clause
        + "、７枚引く。"
    )
    prepared, frame, binding = prepared_case(raw)
    expr = next(v for v in binding.values.values() if isinstance(v, PredicateExpr))
    assert expr.expression_id == descriptor
    facts = [
        p.evidence.value
        for p in frame.projection.imports
        if p.evidence and isinstance(p.evidence.value, EventRef)
    ]
    assert len(facts) == 1
    assert facts[0].event_kind == (
        "Play" if descriptor.startswith("play.") else "EnterField"
    )
    assert all(s.required for s in frame.leaf_schema.slots)
    assert frame.semantic_variant.state == "pending"
    old = derive_source(raw, source(raw), event_classifier())
    assert old == derive_preparation(raw, source(raw), event_classifier()).derived
    assert prepared.derived != old


@pytest.mark.parametrize(
    ("clause", "event", "aggregate"),
    [
        (
            "このターン、自分の場の仮族・フォロワーが７回以上攻撃していたなら",
            "attack",
            "count_facts",
        ),
        ("このターン、自分の場のフォロワーが進化していたなら", "evolve", "exists"),
        ("このターン、自分がスペルをプレイしていたなら", "play", "exists"),
        ("このターン、自分の手札を捨てていたなら", "discard", "exists"),
        ("このターン、自分のフォロワーが破壊されていたなら", "destroy", "exists"),
        (
            "このターン、自分のEXエリアの『仮場手札自分』が消滅していたなら",
            "banish",
            "exists",
        ),
        (
            "このターン、相手のフォロワーが場から墓場に置かれていたなら",
            "field_to_graveyard",
            "exists",
        ),
        (
            "このターン、自分のカードが場から手札に戻っていたなら",
            "return_to_hand",
            "exists",
        ),
        (
            "このターン、自分のリーダーの{体力}を+７以上していたなら",
            "life_increase",
            "sum",
        ),
        ("このターン、これの{体力}を+していたなら", "self_buff_health", "exists"),
        ("このターン、自分がサイコロをふっていたなら", "dice", "exists"),
        (
            "このターン中に自分の【土の秘術】によって自分の場の【スタック】を７個以上取っていたなら",
            "earth_rite",
            "sum",
        ),
    ],
)
def test_history_events_keep_aggregate_and_window(
    clause: str, event: str, aggregate: str
) -> None:
    prepared, _, binding = prepared_case(clause + "、７枚引く。")
    rules = [
        prepared.registry.definition(v)
        for v in binding.values.values()
        if isinstance(v, (ContextExpression, PredicateExpr))
    ]
    query = next(r.history for r in rules if r.history)
    assert query.event_kind == event
    assert query.aggregate == aggregate
    assert query.window == "CurrentTurn"
    assert query.cutoff == "at_query"
    assert query.include_execution
    if event == "field_to_graveyard":
        expr = next(v for v in binding.values.values() if isinstance(v, PredicateExpr))
        assert "subject_owner" in expr.imports
        assert "actor" not in expr.imports
    if event == "life_increase":
        quantity = next(
            v for v in binding.values.values() if isinstance(v, ContextExpression)
        )
        assert "resource_owner" in quantity.imports


def test_multi_event_requires_each_exists_before_and() -> None:
    prepared, _, binding = prepared_case(
        "このターン、自分が仮族・フォロワーと仮族・スペルをプレイしていたなら、７枚引く。"
    )
    expr = next(
        v
        for v in binding.values.values()
        if isinstance(v, PredicateExpr)
        and v.expression_id == "history.play.both.exists.v1"
    )
    children = [binding.values[name] for name in expr.leaves]
    assert len(children) == 2
    descriptors = []
    for child in children:
        assert isinstance(child, PredicateExpr)
        history = prepared.registry.definition(child).history
        assert history is not None
        assert history.aggregate == "exists"
        descriptors.append(child.expression_id)
    assert descriptors[0] != descriptors[1]


def test_history_x_and_single_multiplier_keep_dimension() -> None:
    prepared, _, binding = prepared_case(
        "それにXダメージ。Xは「このターン中に自分がプレイしたカードの枚数」である。"
    )
    name, expr = next(
        (n, v) for n, v in binding.values.items() if isinstance(v, ContextExpression)
    )
    history = prepared.registry.definition(expr).history
    assert history is not None
    assert history.aggregate == "count_subjects"
    assert len([o for o in binding.occurrences if o.slot == name]) == 3
    scaled, _, values = prepared_case(
        "それに「このターン、自分がプレイした仮族・カードや仮族・カードの枚数」の７倍のダメージ。"
    )
    expr = next(
        v
        for v in values.values.values()
        if isinstance(v, ContextExpression)
        and v.expression_id == "quantity.scale.once.v1"
    )
    assert scaled.registry.result_measure(expr, values) == "card_count"


def test_granted_history_keeps_its_own_query_scope() -> None:
    _, frame, binding = prepared_case(
        "それは「【攻撃時】７枚引く。それに「このターン中に自分のリーダーの体力が減少した回数」と同じダメージ」を持つ。"
    )
    expr = next(v for v in binding.values.values() if isinstance(v, ContextExpression))
    assert "granted_" in expr.scope
    assert (
        next(
            p
            for p in frame.projection.imports
            if p.name == expr.imports["resource_owner"]
        ).scope
        == expr.scope
    )


@pytest.mark.parametrize("mode", ["wrong_kind", "new_generation", "missing_fact"])
def test_event_import_rejects_wrong_or_ambiguous_evidence(mode: str) -> None:
    prepared, frame, binding = prepared_case(
        "これが墓場から場に出ていたなら、７枚引く。"
    )
    ports = list(frame.projection.imports)
    index = next(i for i, p in enumerate(ports) if p.type == "EventRef")
    port = ports[index]
    assert port.evidence
    assert isinstance(port.evidence.value, EventRef)
    fact = port.evidence.value
    if mode == "missing_fact":
        del ports[index]
    else:
        fact = (
            fact.model_copy(update={"event_kind": "Play"})
            if mode == "wrong_kind"
            else fact.model_copy(
                update={
                    "subject": ObjectGeneration(
                        object_id=fact.subject.object_id, generation=1
                    )
                }
            )
        )
        ports[index] = port.model_copy(
            update={"evidence": port.evidence.model_copy(update={"value": fact})}
        )
    frame = frame.model_copy(
        update={
            "projection": frame.projection.model_copy(update={"imports": tuple(ports)})
        }
    )
    with pytest.raises(ValueError, match=r"kind|generation|missing"):
        verify_quantities(frame, binding, scopes={}, registry=prepared.registry)


@pytest.mark.parametrize(
    "raw",
    [
        "次にこれが場に出たとき、７枚引く。",
        "このターン、自分の場に出たフォロワーを選ぶ。",
        "これがデッキから場に出ていたなら、７枚引く。",
        "これが墓場以外から場に出ていたなら、７枚引く。",
        "それが手札から場に出ていたなら、７枚引く。",
        "これが仮族・カードの能力によって場に出ていたなら、７枚引く。",
        "これが場に出たとき、７枚引く。",
        "このターン、自分のリーダーの体力が減少したとき、７枚引く。",
    ],
)
def test_neighboring_unsealed_sources_remain_unchanged(raw: str) -> None:
    engine = event_classifier()
    base = derive_preparation(raw, source(raw), engine)
    assert base == replace(
        derive_preparation(raw, source(raw), engine, families=FAMILIES), families=()
    )


def test_required_zone_renders_through_the_predicate_with_annotation() -> None:
    raw = "これが手札から場に出ていたなら、７枚引く。"
    prepared, frame, binding = prepared_case(raw)
    name = next(n for n, v in binding.values.items() if isinstance(v, PredicateExpr))
    expr = binding.values[name]
    assert isinstance(expr, PredicateExpr)
    target = Target(
        format=1,
        nodes=(
            LeafRef(kind="LeafRef", slot=name),
            LiteralNode(kind="Literal", text="，"),
            *(
                LeafRef(kind="LeafRef", slot=s.name)
                for s in frame.leaf_schema.slots
                if s.type == "Nat"
            ),
        ),
    )
    prepared.verify(raw, event_classifier(), frame, binding, target=target)
    _, base, _ = sample()
    renderer = Renderer(
        base.labels,
        {},
        {},
        domains={**event_classifier().domains, **prepared.registry.domains},
    )
    targets = ExpressionTargets(
        {
            (expr.expression_id, "zh-Hant"): SelectedTarget(
                Target(
                    format=1,
                    nodes=(
                        LiteralNode(kind="Literal", text="測試入場自"),
                        LeafRef(kind="LeafRef", slot="zone"),
                    ),
                )
            )
        },
        registry=prepared.registry,
    )
    result = PreparationRenderer(renderer, targets).render(
        "ctx:synthetic",
        "zh-Hant",
        (BoundTarget(frame, binding, SelectedTarget(target)),),
    )
    assert result.rendered is not None
    assert "手牌" in result.rendered.text
    assert result.rendered.annotation.occurrences


def test_excluded_execution_and_ability_availability_are_source_properties() -> None:
    prepared, _, binding = prepared_case(
        "この能力はこれを含めず、このターン中に自分の{UB}能力が７回以上発動していたなら使える。"
    )
    expr = next(v for v in binding.values.values() if isinstance(v, ContextExpression))
    history = prepared.registry.definition(expr).history
    assert history is not None
    assert not history.include_execution
    assert "ability_availability" in expr.scope


def test_later_entry_condition_does_not_capture_preceding_action() -> None:
    raw = "自分の場のフォロワー１体を選ぶ。それに７ダメージ。これが手札から場に出ていたなら、９枚引く。"
    prepared = derive_preparation(
        raw, source(raw), event_classifier(), families=FAMILIES
    )
    found = prepared.derived.recognized[0]
    name = next(n for n, v in found.values.items() if isinstance(v, PredicateExpr))
    spans = [s for o in found.occurrences if o.slot == name for s in o.raw_spans]
    assert all(s.start >= raw.index("これが") for s in spans)


def test_real_source_predicate_can_feed_the_mode_contract() -> None:

    prepared, frame, binding = prepared_case(
        "下記から１つチョイスする。これが墓場から場に出ていたなら、代わりに２つまで。【1】仮。【2】別。"
    )
    name, expr = next(
        (n, v) for n, v in binding.values.items() if isinstance(v, PredicateExpr)
    )
    assert "mode_count" in expr.scope
    port = Port(
        name="mode_choice",
        type="ChoiceContext",
        source_role="choice_context",
        scope=expr.scope,
        evidence=PortEvidence(
            source=binding.occurrence_key(),
            capture="play_parameter",
            value=ChoiceContext(
                kind="ChoiceContext",
                ability_instance="synthetic",
                turn="turn",
                modes=("one", "two"),
            ),
        ),
    )
    frame = frame.model_copy(
        update={
            "projection": frame.projection.model_copy(
                update={
                    "imports": tuple(
                        sorted((*frame.projection.imports, port), key=lambda p: p.name)
                    )
                }
            )
        }
    )
    rule = ModeCountRule.model_validate(
        {
            "base": {"mode": "exact", "expr": {"kind": "constant", "value": 1}},
            "alternative": {"mode": "up_to", "expr": {"kind": "constant", "value": 2}},
            "condition": {"state": "resolved", "predicate": name},
            "choice_context": "mode_choice",
            "scope": expr.scope,
        }
    )
    verify_mode_count(rule, frame, binding)
    verify_quantities(frame, binding, scopes={}, registry=prepared.registry)


def test_event_subject_without_unique_enter_trigger_stays_pending() -> None:
    raw = "それが墓場から場に出ていたなら、７枚引く。"
    prepared = derive_preparation(
        raw, source(raw), event_classifier(), families=FAMILIES
    )
    assert (
        "missing_unique_entry_event_provider" in prepared.derived.recognized[0].issues
    )
    assert any(
        isinstance(v, PredicateExpr)
        for v in prepared.derived.recognized[0].values.values()
    )
    with pytest.raises(ValueError, match="Unresolved"):
        prepared.bind(0)


@pytest.mark.parametrize(
    ("raw", "reason"),
    [
        (
            "これがウマ娘・カードの能力によって場に出ていたなら、７枚引く。",
            "missing_entry_source_trait",
        ),
        (
            "このターン、自分のEXエリアの『未登録仮名』が消滅していたなら、７枚引く。",
            "missing_history_operand",
        ),
        (
            "このターン、自分の場の未登録仮族・フォロワーが７回以上攻撃していたなら、９枚引く。",
            "missing_history_operand",
        ),
    ],
)
def test_missing_required_concepts_never_fall_back_to_active_literal_bindings(
    raw: str, reason: str
) -> None:
    prepared = derive_preparation(raw, source(raw), n1_classifier(), families=FAMILIES)
    assert reason in prepared.derived.recognized[0].issues
    with pytest.raises(ValueError, match="Unresolved"):
        prepared.bind(0)

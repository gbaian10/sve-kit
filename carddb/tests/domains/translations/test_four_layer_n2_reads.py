"""A1/A4 source evidence stays typed, scoped and isolated from production."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Expression,
    GlossaryReference,
    LeafRef,
    LiteralNode,
    PredicateExpr,
    ReceiptRef,
    Target,
    VocabularyReference,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    PreparationRenderer,
    verify_quantities,
)
from sve_carddb.domains.translations.four_layer_n2_reads import FAMILY
from sve_carddb.domains.translations.four_layer_names import FAMILY as NAMES
from sve_carddb.domains.translations.four_layer_preparation import derive_preparation
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .test_four_layer_classification import classifier, source
from .test_four_layer_n1 import n1_classifier
from .test_four_layer_names import engine as names_engine

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier


def test_current_read_uses_shared_source_quantity() -> None:
    raw = "相手にXダメージ。Xは「これの攻撃力」である。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    expr = next(v for v in binding.values.values() if isinstance(v, ContextExpression))
    assert expr.expression_id == "read.object.attack.v1"


@pytest.mark.parametrize(
    ("raw", "identifier"),
    [
        (
            "それに「自分の手札の枚数」と同じダメージ。",
            "count.current_zone.owner.zone.v1",
        ),
        (
            "それに「自分の場の他の仮族・フォロワーの数」と同じダメージ。",
            "count.current_zone.owner.zone.kind.trait.exclude_this.v1",
        ),
        (
            "それに「自分の場のフォロワーの数」の3倍のダメージ。",
            "quantity.scale.once.v1",
        ),
        ("X枚引く。Xは「自分のPP最大値」である。", "read.player.pp_maximum.v1"),
        (
            "自分の場のフォロワー1体を選ぶ。それにXダメージ。Xは「選んだフォロワーの攻撃力」である。",
            "read.selected.attack.v1",
        ),
        ("{コストX}：相手にXダメージ。", "read.parameter.chosen_x.v1"),
        (
            "{コストX}{コストX}：自分のデッキから『仮場手札自分』X枚まで探し、場に出す。",
            "read.parameter.chosen_x.v1",
        ),
        (
            "場の『仮場手札自分』X枚を消滅：下記からX個までチョイスする。【1】3枚引く。【2】4枚引く。",
            "read.parameter.chosen_x.v1",
        ),
    ],
    ids=[
        "hand",
        "filtered",
        "scaled",
        "pp",
        "selection",
        "pp-payment",
        "two-costs",
        "materials",
    ],
)
def test_sealed_surfaces_derive_and_verify(raw: str, identifier: str) -> None:
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    assert any(
        isinstance(v, (Expression, ContextExpression)) and v.expression_id == identifier
        for v in binding.values.values()
    )


@pytest.mark.parametrize(
    "raw",
    [
        "Xダメージを割り振る。Xは「これの攻撃力」である。",
        "これに仮カウンターX個を置く。Xは「自分のPP最大値」である。",
        "X枚引く。Xは「自分の場のクレストの数」である。",
        "X枚引く。Xは「場のフォロワーの数」である。",
        "それに「自分と相手の場のフォロワーの数」と同じダメージ。",
        "それに「「自分の手札の枚数」の3倍」の2倍のダメージ。",
        "{起動}X枚引く。{起動}Xは「これの攻撃力」である。",
        "【1】X枚引く。【2】Xは「これの攻撃力」である。",
        "それに「最大の攻撃力」と同じダメージ。",
        "それに「これの攻撃力の半分」の2倍のダメージ。",
        "それに「自分の手札の枚数」の0倍のダメージ。",
    ],
    ids=[
        "distribution",
        "place-counter",
        "crest",
        "no-zone-owner",
        "both-players",
        "nested",
        "other-ability",
        "sibling-mode",
        "maximum",
        "scaled-half",
        "zero-multiplier",
    ],
)
def test_unsealed_consumers_and_sources_are_unchanged(raw: str) -> None:
    engine = n1_classifier()
    baseline = derive_preparation(raw, source(raw), engine)
    candidate = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    assert candidate.derived == baseline.derived
    assert candidate.interfaces == baseline.interfaces


def test_declared_x_is_shared_and_rejects_receipt_or_wrong_capture() -> None:

    raw = "{コストX}{コストX}：相手にXダメージ。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    expr_name, expr = next(
        (n, v) for n, v in binding.values.items() if isinstance(v, ContextExpression)
    )
    assert len([o for o in binding.occurrences if o.slot == expr_name]) == 3
    declared = next(
        p for p in frame.projection.imports if p.name == expr.imports["value"]
    )
    assert declared.evidence is not None
    for patch in (
        {"capture": "receipt"},
        {
            "value": ReceiptRef(
                kind="ReceiptId", receipt_id="synthetic-actual-payment", result="amount"
            )
        },
    ):
        wrong = declared.model_copy(
            update={"evidence": declared.evidence.model_copy(update=patch)}
        )
        projection = frame.projection.model_copy(
            update={
                "imports": tuple(
                    wrong if p.name == declared.name else p
                    for p in frame.projection.imports
                )
            }
        )
        with pytest.raises(ValueError, match="construction"):
            prepared.verify(
                raw,
                engine,
                frame.model_copy(update={"projection": projection}),
                binding,
            )


def read_engine() -> Classifier:

    base = n1_classifier()
    return classifier(
        (
            *base.terms.values(),
            Term("term:object.counter", "rule_term", "カウンター"),
            Term("term:object.die", "rule_term", "サイコロ"),
        ),
        ("braced_ability_reference", "bracket_keyword_reference"),
        vocabulary=base.references.vocabulary,
    )


@pytest.mark.parametrize(
    ("raw", "identifier"),
    [
        ("X枚引く。Xは「これの仮カウンターの数」である。", "read.object.counter.v1"),
        (
            "X枚引く。Xは「これに置かれていた仮カウンターの数」である。",
            "read.holder.previous_counter.v1",
        ),
        ("サイコロをふる。それに「出た目」と同じダメージ。", "read.captured.dice.v1"),
        (
            "それに「自分のエボルヴデッキの表向きの仮族・エボルヴフォロワーの枚数」と同じダメージ。",
            "read.faceup_evolve.trait.v1",
        ),
        (
            "それに「自分のエボルヴデッキの表向きの『仮場手札自分』の枚数」と同じダメージ。",
            "read.faceup_evolve.count.v1",
        ),
        (
            "X枚引く。Xは「自分の墓場にあるカードの元のコストの種類数」である。",
            "read.distinct_original_cost.zone.v1",
        ),
        (
            "自分の場のフォロワー1体を選ぶ。それを破壊する。それにXダメージ。Xは「選んだフォロワーの攻撃力」である。",
            "read.lki.attack.v1",
        ),
        (
            "相手のフォロワー1体を選ぶ。それを手札に戻す。自分のリーダーは{体力}+Xする。Xは「選んだフォロワーの元のコスト」である。",
            "read.lki.original_cost.v1",
        ),
        (
            "自分の場のフォロワー1体を選ぶ。それにXダメージ。Xは「それの体力-1」である。",
            "read.offset.subtract.v1",
        ),
        (
            "自分の場のフォロワー1体と相手の場のフォロワー1体を選ぶ。前者に「後者の攻撃力」と同じダメージ。",
            "read.selected.attack.v1",
        ),
        (
            "これのコストは「自分の手札の枚数」と同じだけ-1する。",
            "count.current_zone.owner.zone.v1",
        ),
        ("それに「これの攻撃力の半分」と同じダメージ。", "read.half_up.v1"),
        (
            "これは{攻撃力}+Xする。Xは「これの攻撃力」である。Xが7以上なら、3枚引く。",
            "read.object.attack.v1",
        ),
        (
            "自分のデッキの上3枚を墓場に置く。それに「これによって墓場に置いたスペルの枚数」と同じダメージ。",
            "read.result.moved_spell.v1",
        ),
    ],
    ids=[
        "counter",
        "previous-counter",
        "dice",
        "faceup-trait",
        "faceup-name",
        "distinct-cost",
        "lki",
        "returned-lki",
        "offset",
        "ordered-pair",
        "per-unit-reduction",
        "half-up",
        "later-x",
        "result-count",
    ],
)
def test_capture_and_measure_source_shapes(raw: str, identifier: str) -> None:
    engine = read_engine()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    assert any(
        isinstance(v, (Expression, ContextExpression)) and v.expression_id == identifier
        for v in binding.values.values()
    )


def test_scaling_renders_required_operands_and_annotations() -> None:

    raw = "それに「自分の場の仮族・フォロワーの数」の3倍のダメージ。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    expression_name = next(
        n
        for n, v in binding.values.items()
        if isinstance(v, ContextExpression)
        and v.expression_id == "quantity.scale.once.v1"
    )
    base = Renderer({}, {}, {}, domains={**engine.domains, **prepared.registry.domains})
    labels = {}
    for label_ref, text in (
        (base.code_references["Player", "self"], "自己"),
        (base.code_references["ZoneSet", "battlefield"], "戰場"),
        (VocabularyReference(kind="vocabulary", key=("type", "follower")), "從者"),
        (GlossaryReference(kind="glossary", key="term:trait.synthetic"), "測試族"),
    ):
        labels[canonical(label_ref.model_dump(mode="json")), "zh-Hant"] = Label(
            label_ref, "zh-Hant", text, "project", False, True
        )

    def literal(text: str) -> LiteralNode:
        return LiteralNode(kind="Literal", text=text)

    def ref(name: str) -> LeafRef:
        return LeafRef(kind="LeafRef", slot=name)

    targets = ExpressionTargets(
        {
            ("count.current_zone.owner.zone.kind.trait.v1", "zh-Hant"): SelectedTarget(
                Target(
                    format=1,
                    nodes=(
                        ref("owner"),
                        literal("的"),
                        ref("zone"),
                        literal("上的"),
                        ref("trait"),
                        ref("kind"),
                        literal("張數"),
                    ),
                )
            ),
            ("quantity.scale.once.v1", "zh-Hant"): SelectedTarget(
                Target(
                    format=1,
                    nodes=(ref("value"), literal("的"), ref("factor"), literal("倍")),
                )
            ),
        },
        registry=prepared.registry,
    )
    renderer = PreparationRenderer(
        Renderer(labels, {}, {}, domains=base.domains), targets
    )
    selected = SelectedTarget(
        Target(
            format=1, nodes=(literal("造成"), ref(expression_name), literal("的傷害。"))
        )
    )
    prepared.verify(raw, engine, frame, binding, target=selected.target)
    item = BoundTarget(frame, binding, selected)
    result = renderer.render("ctx:synthetic", "zh-Hant", (item,))
    assert result.rendered is not None
    assert result.rendered.text == "造成自己的戰場上的測試族從者張數的3倍的傷害。"
    assert len(result.rendered.annotation.occurrences) == 4
    assert renderer.required_references(item, "zh-Hant")
    with pytest.raises(ValueError, match="required"):
        prepared.verify(
            raw,
            engine,
            frame,
            binding,
            target=Target(format=1, nodes=(literal("固定字面"),)),
        )
    renderer.labels.clear()
    assert renderer.render("ctx:synthetic", "zh-Hant", (item,)).rendered is None


def test_non_single_selection_does_not_fabricate_an_object() -> None:
    raw = "自分の場のフォロワー2体を選ぶ。それにXダメージ。Xは「選んだフォロワーの攻撃力」である。"
    engine = n1_classifier()
    before = derive_preparation(raw, source(raw), engine)
    after = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    assert after.derived == before.derived
    assert after.interfaces == before.interfaces


def test_both_result_read_branches_keep_independent_class_and_provider() -> None:

    raw = (
        "下記から1つチョイスする。"
        "【1】墓場の{仮クラス}フォロワー1枚を消滅：それに「これによって消滅した{仮クラス}フォロワーの攻撃力」の3倍のダメージ。"
        "【2】墓場の「{仮クラス}でないフォロワー」1枚を消滅：それに「これによって消滅した「{仮クラス}でないフォロワー」の攻撃力」と同じダメージ。"
    )
    base_engine = n1_classifier()
    engine = classifier(
        tuple(base_engine.terms.values()),
        (*base_engine.rules.enabled(), "suffix_unit_items", "bracket_choice_index"),
        vocabulary=base_engine.references.vocabulary,
    )
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    reads = [
        (n, v)
        for n, v in binding.values.items()
        if isinstance(v, ContextExpression)
        and v.expression_id.startswith("read.result_object.")
    ]
    assert {v.expression_id for _, v in reads} == {
        "read.result_object.banished_class_attack.v1",
        "read.result_object.banished_not_class_attack.v1",
    }
    (first_name, first), (_, second) = reads
    assert first.scope != second.scope
    assert first.imports != second.imports
    assert all(len(v.leaves) == 2 for _, v in reads)
    wrong = first.model_copy(update={"imports": second.imports})
    wrong_frame = frame.model_copy(
        update={
            "leaf_schema": frame.leaf_schema.model_copy(
                update={
                    "slots": tuple(
                        s.model_copy(update={"expression": wrong})
                        if s.name == first_name
                        else s
                        for s in frame.leaf_schema.slots
                    )
                }
            )
        }
    )
    wrong_binding = binding.model_copy(
        update={"values": dict(binding.values) | {first_name: wrong}}
    )
    with pytest.raises(ValueError, match="branch or ability"):
        verify_quantities(
            wrong_frame, wrong_binding, scopes={}, registry=prepared.registry
        )


def test_mixed_payment_and_other_ability_x_stay_separate() -> None:
    raw = (
        "{起動}{コストX}これの仮カウンターX個を取る：自分のデッキから元のコストX以下のフォロワーを探す。\n"
        "{ファンファーレ}これに仮カウンターX個を置く。Xは「自分のPP最大値」である。"
    )
    engine = read_engine()
    before = derive_preparation(raw, source(raw), engine)
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    declared = next(
        n for n, v in binding.values.items() if isinstance(v, ContextExpression)
    )
    assert len([o for o in binding.occurrences if o.slot == declared]) == 3
    assert prepared.derived.recognized[-1] == before.derived.recognized[-1]
    assert prepared.derived.field.parts[-1] == before.derived.field.parts[-1]


@pytest.mark.parametrize("result", ["amount", "payment_complete"])
def test_card_receipt_rejects_other_results(result: str) -> None:
    raw = "自分のデッキの上3枚を墓場に置く。それに「これによって墓場に置いたスペルの枚数」と同じダメージ。"
    engine = read_engine()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    port = next(p for p in frame.projection.imports if p.type == "ReceiptId")
    assert port.evidence is not None
    wrong = port.model_copy(
        update={
            "evidence": port.evidence.model_copy(
                update={
                    "value": ReceiptRef.model_validate(
                        {
                            "kind": "ReceiptId",
                            "receipt_id": "synthetic-payment",
                            "result": result,
                        }
                    )
                }
            )
        }
    )
    projection = frame.projection.model_copy(
        update={
            "imports": tuple(
                wrong if p.name == port.name else p for p in frame.projection.imports
            )
        }
    )
    with pytest.raises(ValueError, match="wrong result kind"):
        verify_quantities(
            frame.model_copy(update={"projection": projection}),
            binding,
            scopes={},
            registry=prepared.registry,
        )


def test_granted_read_uses_holder_in_its_own_ability_scope() -> None:
    raw = "それは「自分がカードを引いたとき、相手に「これの攻撃力の半分」と同じダメージ」を持つ。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    attack = next(
        v
        for v in binding.values.values()
        if isinstance(v, ContextExpression)
        and v.expression_id == "read.object.attack.v1"
    )
    assert attack.scope.startswith("read_grant_")
    port = next(
        p for p in frame.projection.imports if p.name == attack.imports["subject"]
    )
    assert port.scope == attack.scope
    assert port.evidence is not None
    assert port.evidence.value.kind == "ObjectSet"
    assert port.evidence.value.origin == "holder"


def test_missing_read_reference_preserves_pending_source() -> None:
    raw = "それに「自分の場の『未採用の仮名』の数」と同じダメージ。"
    engine = n1_classifier()
    before = derive_preparation(raw, source(raw), engine)
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    assert prepared.derived == before.derived
    assert "missing_card_name_concept" in prepared.derived.recognized[0].issues
    with pytest.raises(ValueError, match="Unresolved"):
        prepared.bind(0)


@pytest.mark.parametrize("granted", [False, True], ids=["own-cost", "granted-cost"])
def test_previous_line_extra_cost_requires_its_own_source(granted: bool) -> None:
    cost = "これをプレイする際、追加コストとして場の仮族・カードを好きな枚数アクトしてよい。"
    raw = (
        (f"それは「{cost}」を持つ。" if granted else cost)
        + "\nそれに「これの追加コストとしてアクトした仮族・カードの枚数」の3倍のダメージ。"
    )
    engine = n1_classifier()
    before = derive_preparation(raw, source(raw), engine)
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    if granted:
        assert prepared.derived == before.derived
    else:
        index = next(
            i
            for i, found in enumerate(prepared.derived.recognized)
            if any(isinstance(v, ContextExpression) for v in found.values.values())
        )
        frame, binding = prepared.bind(index)
        prepared.verify(raw, engine, frame, binding)
        port = next(p for p in frame.projection.imports if p.type == "ReceiptId")
        assert port.evidence is not None
        assert port.evidence.source.line_ordinal < binding.line_ordinal


def test_context_player_operand_must_be_presented() -> None:
    raw = "X枚引く。Xは「自分のPP最大値」である。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
    frame, binding = prepared.bind(0)
    name = next(
        n for n, v in binding.values.items() if isinstance(v, ContextExpression)
    )
    base = Renderer({}, {}, {}, domains={**engine.domains, **prepared.registry.domains})
    reference = base.code_references["Player", "self"]
    labels = {
        (canonical(reference.model_dump(mode="json")), "zh-Hant"): Label(
            reference, "zh-Hant", "自己", "project", False, True
        )
    }
    expression = Target(
        format=1,
        nodes=(
            LeafRef(kind="LeafRef", slot="player.0"),
            LiteralNode(kind="Literal", text="的PP最大值"),
        ),
    )
    targets = ExpressionTargets(
        {("read.player.pp_maximum.v1", "zh-Hant"): SelectedTarget(expression)},
        registry=prepared.registry,
    )
    renderer = PreparationRenderer(
        Renderer(labels, {}, {}, domains=base.domains), targets
    )
    selected = SelectedTarget(
        Target(format=1, nodes=(LeafRef(kind="LeafRef", slot=name),))
    )
    result = renderer.render(
        "ctx:synthetic", "zh-Hant", (BoundTarget(frame, binding, selected),)
    )
    assert result.rendered is not None
    assert result.rendered.text == "自己的PP最大值"
    assert len(result.rendered.annotation.occurrences) == 1
    missing = ExpressionTargets(
        {
            ("read.player.pp_maximum.v1", "zh-Hant"): SelectedTarget(
                Target(format=1, nodes=(LiteralNode(kind="Literal", text="固定值"),))
            )
        },
        registry=prepared.registry,
    )
    with pytest.raises(ValueError, match="required"):
        PreparationRenderer(
            Renderer(labels, {}, {}, domains=base.domains), missing
        ).render("ctx:synthetic", "zh-Hant", (BoundTarget(frame, binding, selected),))


@pytest.mark.parametrize(
    "names_first", [True, False], ids=["names-first", "reads-first"]
)
def test_other_family_predicates_keep_their_operands(names_first: bool) -> None:
    raw = (
        "自分のデッキから「カード名に『仮場手札自分』を含むフォロワー」1枚を探し、手札に加える。"
        "それに「自分の手札の枚数」と同じダメージ。"
    )
    engine = names_engine()
    families = (NAMES, FAMILY) if names_first else (FAMILY, NAMES)
    prepared = derive_preparation(raw, source(raw), engine, families=families)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    assert any(isinstance(v, PredicateExpr) for v in binding.values.values())
    assert any(isinstance(v, Expression) for v in binding.values.values())


def test_current_zone_surfaces_reuse_b11_definition() -> None:
    engine = n1_classifier()
    for raw in (
        "それに「自分の手札の枚数」と同じダメージ。",
        "相手にXダメージ。Xは「自分の手札の枚数」である。",
        "それに「自分の手札の枚数」の3倍のダメージ。",
    ):
        prepared = derive_preparation(raw, source(raw), engine, families=(FAMILY,))
        frame, binding = prepared.bind(0)
        prepared.verify(raw, engine, frame, binding)
        reads = [v for v in binding.values.values() if isinstance(v, Expression)]
        assert len(reads) == 1
        assert reads[0].expression_id == "count.current_zone.owner.zone.v1"
        assert not any(
            d.startswith("read.current_zone.") for d in prepared.registry.descriptors
        )

"""Sealed local reads preserve exact uses, required references and production behavior."""

import pytest

from sve_carddb.contracts.four_layer import (
    CountArgs,
    CountExpr,
    Expression,
    LeafRef,
    LeafSchema,
    LeafType,
    LiteralNode,
    SlotRef,
    Target,
    hash_payload,
)
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    PreparationRenderer,
    target_dependencies,
)
from sve_carddb.domains.translations.four_layer_matching import bind_recognized
from sve_carddb.domains.translations.four_layer_preparation import derive_preparation
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .test_four_layer_classification import source
from .test_four_layer_n1 import n1_classifier


@pytest.mark.parametrize(
    ("definition", "suffix", "expected"),
    [
        ("自分の消滅領域の枚数", "owner.zone.v1", ("self", ("banish",))),
        ("自分のEXエリアのカードの枚数", "owner.zone.card.v1", ("self", ("ex",))),
        (
            "相手の場のフォロワーの数",
            "owner.zone.kind.v1",
            ("opponent", ("battlefield",)),
        ),
        (
            "自分の墓場の仮族・カードの枚数",
            "owner.zone.card.trait.v1",
            ("self", ("graveyard",)),
        ),
        (
            "自分の場の仮族・トークンの数",
            "owner.zone.token_status.trait.token.v1",
            ("self", ("battlefield",)),
        ),
        (
            "自分の場の他の【仮旗】を持つフォロワーの数",
            "owner.zone.keyword.kind.exclude_this.v1",
            ("self", ("battlefield",)),
        ),
        (
            "自分の場の{仮クラス}カードの枚数",
            "owner.zone.class.card.v1",
            ("self", ("battlefield",)),
        ),
        (
            "自分の場の『仮場手札自分』の数+7",
            "owner.zone.name.add_offset.v1",
            ("self", ("battlefield",)),
        ),
        ("自分のデッキの枚数-7", "owner.zone.subtract_offset.v1", ("self", ("deck",))),
        (
            "自分の場のアミュレットの数+7",
            "owner.zone.kind.add_offset.v1",
            ("self", ("battlefield",)),
        ),
    ],
)
def test_count_uses_only_sealed_typed_reads(
    definition: str, suffix: str, expected: tuple[object, ...]
) -> None:
    raw = f"それにXダメージ。Xは「{definition}」である。"
    engine = n1_classifier()
    old = derive_source(raw, source(raw), engine)
    prepared = derive_preparation(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    expr = next(v for v in binding.values.values() if isinstance(v, Expression))
    assert expr.expression_id == "count.current_zone." + suffix
    assert tuple(binding.values[n] for n in expr.leaves[:2]) == expected
    assert frame.semantic_variant.state == "pending"
    assert all(slot.required for slot in frame.leaf_schema.slots)
    assert not any(isinstance(v, Expression) for v in old.recognized[0].values.values())
    if "+7" in definition or "-7" in definition:
        assert binding.values[expr.leaves[-1]] == 7
        assert frame.leaf_schema.slots[-1].role == "arithmetic_offset"
    if "カード" not in definition:
        assert not any(s.role == "read_card" for s in frame.leaf_schema.slots)


def test_repeated_x_keeps_print_order_and_rebinds_expression_actual_names() -> None:
    raw = "元のコストX以下を選ぶ。それにXダメージ。Xは「自分の手札の枚数」である。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    expr_name = next(n for n, v in binding.values.items() if isinstance(v, Expression))
    occurrences = [o for o in binding.occurrences if o.slot == expr_name]
    assert len(occurrences) == 3
    assert [raw[o.raw_spans[0].start : o.raw_spans[0].end] for o in occurrences] == [
        "X"
    ] * 3
    assert len({o.canonical_spans for o in occurrences}) == 3
    expression_slot = next(s for s in frame.leaf_schema.slots if s.name == expr_name)
    split = frame.model_copy(
        update={
            "leaf_schema": LeafSchema(
                format=2,
                slots=(
                    expression_slot.model_copy(
                        update={"occurrences": expression_slot.occurrences[:1]}
                    ),
                    expression_slot.model_copy(
                        update={
                            "name": "another_x",
                            "occurrences": expression_slot.occurrences[1:],
                        }
                    ),
                    *(s for s in frame.leaf_schema.slots if s.name != expr_name),
                ),
            )
        }
    )
    assert (
        bind_recognized(
            split,
            prepared.derived.field,
            prepared.derived.field.parts[0],
            prepared.derived.recognized[0],
            mixed=True,
        )
        is None
    )
    renamed = frame.model_copy(
        update={
            "leaf_schema": LeafSchema(
                format=2,
                slots=tuple(
                    s.model_copy(update={"name": "actual_" + s.name})
                    for s in frame.leaf_schema.slots
                ),
            )
        }
    )
    checksum = hash_payload(
        renamed.payload(prepared.derived.field.parts[0].canonical_source)
    )
    renamed = renamed.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    rebound = bind_recognized(
        renamed,
        prepared.derived.field,
        prepared.derived.field.parts[0],
        prepared.derived.recognized[0],
        mixed=True,
    )
    assert rebound is not None
    prepared.verify(raw, engine, renamed, rebound)
    expr = rebound.values["actual_" + expr_name]
    assert isinstance(expr, Expression)
    assert all(n.startswith("actual_") for n in expr.leaves)
    bad = rebound.model_copy(
        update={
            "values": dict(rebound.values)
            | {
                "actual_" + expr_name: expr.model_copy(
                    update={"leaves": tuple(reversed(expr.leaves))}
                )
            }
        }
    )
    with pytest.raises(ValueError, match=r"identity|payload|construction"):
        prepared.verify(raw, engine, renamed, bad)


@pytest.mark.parametrize("condition", ["なら、", "である限り、"])
def test_sep_explicit_zero_is_a_threshold(condition: str) -> None:
    raw = f"自分のSEPが０{condition}7枚引く。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    prepared.verify(raw, engine, frame, binding)
    slot = next(s for s in frame.leaf_schema.slots if s.role == "threshold")
    assert binding.values[slot.name] == 0
    assert slot.type == "Nat"
    assert slot.domain.min == 0
    assert not any(s.type == "Player" for s in frame.leaf_schema.slots)
    assert "自分のSEPがN" in prepared.derived.field.parts[0].canonical_source
    assert derive_source(raw, source(raw), engine).recognized[0].issues


@pytest.mark.parametrize(
    "raw",
    [
        "X個を置く。Xは「自分の手札の枚数」である。",
        "Xダメージを割り振る。Xは「自分のEXエリアのカードの枚数」である。",
        "X枚引く。Xは「場のフォロワーの数」である。",
        "X枚引く。Xは「自分の場のクレストの数」である。",
        "X枚引く。Xは「自分の進化デッキの表向きの枚数」である。",
        "X枚引く。Xは「自分の場の仮族・仮族・フォロワーの数」である。",
        "X枚引く。{起動}Xは「自分の手札の枚数」である。",
        "X枚引く。\nXは「自分の手札の枚数」である。",
        "自分のSEPが1なら、7枚引く。",
        "相手のSEPが0なら、7枚引く。",
    ],
)
def test_neighboring_unsealed_sources_keep_their_v2_payload(raw: str) -> None:
    engine = n1_classifier()
    old = derive_source(raw, source(raw), engine)
    assert derive_preparation(raw, source(raw), engine).derived == old


def test_missing_card_name_cannot_be_replaced_by_a_literal_count() -> None:
    raw = "X枚引く。Xは「自分の場の『未採用の仮名』の数」である。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine)
    assert "missing_card_name_concept" in prepared.derived.recognized[0].issues
    with pytest.raises(ValueError, match="Unresolved"):
        prepared.bind(0)


@pytest.mark.parametrize("offset", [False, True], ids=["B11", "B03"])
def test_prepared_count_renders_through_normal_labels_and_annotation_paths(  # ruff: ignore[too-many-locals] -- keep source, rendering and reference evidence in one synthetic case
    offset: bool,
) -> None:
    raw = "X枚引く。Xは「自分の消滅領域の枚数" + ("-7" if offset else "") + "」である。"
    engine = n1_classifier()
    prepared = derive_preparation(raw, source(raw), engine)
    frame, binding = prepared.bind(0)
    name, expr = next(
        (n, v) for n, v in binding.values.items() if isinstance(v, Expression)
    )
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=(
                LiteralNode(kind="Literal", text="抽取與"),
                CountExpr(
                    kind="NP",
                    constructor="CountExpr",
                    args=CountArgs(expr=SlotRef(slot=name)),
                ),
                LiteralNode(kind="Literal", text="相同張數的卡片。"),
            ),
        )
    )
    prepared.verify(raw, engine, frame, binding, target=selected.target)
    target = Target(
        format=1,
        nodes=(
            LeafRef(kind="LeafRef", slot="owner"),
            LiteralNode(kind="Literal", text="的"),
            LeafRef(kind="LeafRef", slot="zone"),
            LiteralNode(kind="Literal", text="張數"),
            *(
                (
                    LiteralNode(kind="Literal", text="減去"),
                    LeafRef(kind="LeafRef", slot="offset"),
                )
                if offset
                else ()
            ),
        ),
    )
    expression = ExpressionTargets(
        {(expr.expression_id, "zh-Hant"): SelectedTarget(target, low_confidence=True)}
    )
    base = Renderer({}, {}, {}, domains=engine.domains)
    labels = {}
    entries: tuple[tuple[LeafType, str, str], ...] = (
        ("Player", "self", "自己"),
        ("ZoneSet", "banish", "消失區"),
    )
    for type_name, code, text in entries:
        assert type_name in {"Player", "ZoneSet"}
        reference = base.code_references[type_name, code]
        labels[canonical(reference.model_dump(mode="json")), "zh-Hant"] = Label(
            reference, "zh-Hant", text, "project", False, True
        )
    renderer = PreparationRenderer(
        Renderer(labels, {}, {}, domains=engine.domains), expression
    )
    item = BoundTarget(frame, binding, selected)
    result = renderer.render("ctx:synthetic", "zh-Hant", (item,))
    assert result.rendered is not None
    assert result.rendered.text == (
        "抽取與自己的消失區張數" + ("減去7" if offset else "") + "相同張數的卡片。"
    )
    assert result.rendered.low_confidence
    assert len(result.rendered.annotation.occurrences) == 2
    assert any(len(o.node_path) > 1 for o in result.rendered.leaves)
    assert len(result.rendered.leaves) == (4 if offset else 3)
    for occurrence in result.rendered.annotation.occurrences:
        assert len(occurrence.ranges) == 1
        span = occurrence.ranges[0]
        assert result.rendered.text[span.start : span.end] in {"自己", "消失區"}
    assert renderer.required_references(item, "zh-Hant")
    missing_expression = Target(
        format=1,
        nodes=tuple(LeafRef(kind="LeafRef", slot=n) for n in expr.leaves),
    )
    with pytest.raises(ValueError, match="required"):
        prepared.verify(raw, engine, frame, binding, target=missing_expression)
    with pytest.raises(ValueError, match="required"):
        renderer.render(
            "ctx:synthetic",
            "zh-Hant",
            (BoundTarget(frame, binding, SelectedTarget(missing_expression)),),
        )
    with pytest.raises(ValueError, match="required"):
        selected.target.verify(frame.leaf_schema, "zh-Hant", {})
    with pytest.raises(ValueError, match="required"):
        base.render("ctx:synthetic", "zh-Hant", (item,))
    old_frame = frame.model_copy(
        update={
            "source": frame.source.model_copy(
                update={"normalizer_version": "four-layer-jp-v2"}
            )
        }
    )
    with pytest.raises(ValueError, match="v3"):
        target_dependencies(old_frame, binding)
    values = dict(binding.values)
    values.pop(expr.leaves[0])
    missing_actual = binding.model_copy(update={"values": values})
    with pytest.raises(ValueError, match="missing actual"):
        renderer.render(
            "ctx:synthetic", "zh-Hant", (BoundTarget(frame, missing_actual, selected),)
        )
    with pytest.raises(ValueError, match="missing required leaf values"):
        prepared.verify(raw, engine, frame, missing_actual, target=selected.target)
    renderer.labels.clear()
    assert renderer.render("ctx:synthetic", "zh-Hant", (item,)).rendered is None

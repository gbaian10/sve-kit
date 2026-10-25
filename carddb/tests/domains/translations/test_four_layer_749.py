"""Sealed #749 interfaces use synthetic source text through derive and bind."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import (
    AbilitySetRef,
    CardNameReference,
    Constant,
    ContextExpression,
    EventRef,
    Expression,
    FaceRevisionOwner,
    GlossaryReference,
    LeafRef,
    LiteralNode,
    ObjectGeneration,
    ObjectSetRef,
    PlayerRef,
    PoolContext,
    PredicateExpr,
    QuantitySpec,
    Target,
    VocabularyReference,
)
from sve_carddb.contracts.source_binding import expression_node
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_classification import Classifier, Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import (
    ExpressionTargets,
    definition,
    target_dependencies,
    verify_quantities,
    verify_target_shape,
)
from sve_carddb.domains.translations.four_layer_n3_events import HOLDER_REVISIONS
from sve_carddb.domains.translations.four_layer_n3_ports import (
    verify_counter_witness,
    verify_pool_allocation,
)
from sve_carddb.domains.translations.four_layer_n3_shapes import CHOICE_DOMAIN
from sve_carddb.domains.translations.four_layer_render import (
    BoundTarget,
    Label,
    Renderer,
    SelectedTarget,
)

from .expression_fixtures import expression_plan
from .test_four_layer_classification import classifier, source
from .test_four_layer_n3 import n3_classifier

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField


@pytest.fixture
def engine() -> Classifier:
    base_engine = n3_classifier()
    terms = (
        Term("term:object.die", "rule_term", "サイコロ"),
        Term("term:object.counter", "rule_term", "カウンター"),
        Term("term:ability.stack", "ability", "スタック"),
        Term("term:ability.fusion", "ability", "融合"),
        Term("term:name.earth_magic_fragment", "card_name", "仮土片"),
        *(Term(f"term:name.search_{i}", "card_name", f"仮検索{i}") for i in range(10)),
    )
    return classifier(
        (*base_engine.terms.values(), *terms),
        base_engine.rules.enabled(),
        vocabulary=base_engine.references.vocabulary,
        kind_facts=base_engine.references.kind_facts,
    )


SEARCH_SOURCE = (
    "仮。自分のデッキから「カード名に"
    + "か".join(f"『仮検索{i}』" for i in range(10))
    + "を含むカード」をカード名が異なるように10枚まで探す。その中から、好きな枚数を場に出す。好きな枚数をEXエリアに置く。"
)

CASES = (
    ("P02", SEARCH_SOURCE, 0),
    (
        "P01",
        "仮。『仮甲』9枚の中から、好きな枚数を場に出す。好きな枚数をEXエリアに置く。",
        0,
    ),
    (
        "P01-pair",
        "仮。『仮甲』1枚と『仮乙』1枚の中から、1枚を場に出す。1枚をEXエリアに置く。",
        0,
    ),
    (
        "H04",
        "仮。【スタック】を+1する。（【スタック】を持つ『仮土片』1つを場に出し、【スタック】を+する）",
        1,
    ),
    (
        "R01",
        "仮。手札をすべて捨てる。X枚引く。Xは「これによって捨てた手札の枚数+1」である。自分のデッキが1枚以下なら、仮。",
        0,
    ),
    (
        "X02-paid",
        "仮。場の『仮甲』X枚を消滅:下記からX個までチョイスする。【1】1枚引く。【2】2枚引く。【3】3枚引く。",
        0,
    ),
    ("P03", "仮。『仮甲』3枚を、場が上限になるまで出す。残りをEXエリアに置く。", 0),
    (
        "P04",
        "仮。場のフォロワーすべてを破壊する。これによって破壊された自身の場のフォロワー1体につき、『仮甲』1体を場に出す。",
        0,
    ),
    (
        "H01",
        "【進化時】自分のEXエリアのカード1枚の情熱カウンターが4個以上なら、仮。10個以上なら、仮。",
        0,
    ),
    ("H02", "仮。自分の場に『仮甲』がいるなら、これに仮熱カウンター2個まで置く。", 0),
    (
        "H03",
        "【スタック】仮。（これが場に出たとき、これにスタックカウンター1つを置く。これが場を離れるとき、代わりにスタックカウンター1つを取り除く。スタックカウンターが0なら、仮。）",
        1,
    ),
    ("D01", "仮。このターン、自分がふったサイコロの出た目に6があるなら、仮。", 0),
    (
        "D02",
        "【進化時】1から6の中から、好きな数1つを指定してサイコロをふる。指定した数なら、仮。",
        0,
    ),
    (
        "X01",
        "【融合】元のコスト1以上の仮族・カードX枚:これに融合カウンターX個を置く。Xは2以下。",
        0,
    ),
    (
        "X02",
        "仮。Xは「自分の場の仮族・カードの枚数」である。下記からX個までチョイスする。【1】1枚引く。【2】2枚引く。【3】3枚引く。",
        0,
    ),
    ("R02", "仮。これがダメージを受けたとき、そのダメージが5以上なら、仮。", 0),
    ("A01", "仮。これの【進化時】能力1つをプレイする。", 0),
)


def derive(raw: str, engine: Classifier, case: str = "") -> DerivedField:
    descriptor = source(raw)
    if case == "H01":
        descriptor = descriptor.model_copy(
            update={
                "owner": FaceRevisionOwner(
                    kind="face_revision", revision_id=min(HOLDER_REVISIONS)
                )
            }
        )
    return derive_source(raw, descriptor, engine)


@pytest.mark.parametrize(("case", "raw", "ordinal"), CASES, ids=[x[0] for x in CASES])
def test_finite_sources_bind(
    case: str, raw: str, ordinal: int, engine: Classifier
) -> None:
    found = derive(raw, engine, case)
    assert not found.recognized[ordinal].issues
    frame, binding = found.bind(ordinal)
    found.verify_binding(raw, engine, frame, binding)
    assert frame.source.normalizer_version == "four-layer-jp-v9"
    assert frame.semantic_variant.state == "pending"
    assert frame == type(frame).model_validate_json(frame.model_dump_json())
    ports = frame.projection.imports
    if case.startswith("P01") or case == "P03":
        pool = next(
            p.evidence.value
            for p in ports
            if p.evidence and isinstance(p.evidence.value, PoolContext)
        )
        remainder = next(
            p.evidence.value
            for p in ports
            if p.evidence
            and isinstance(p.evidence.value, ObjectSetRef)
            and p.evidence.value.origin == "pool_remainder"
        )
        assert pool.source_kind == "generated"
        assert remainder.pool in {p.name for p in ports}
        assert remainder.placement in {p.name for p in frame.projection.exports}
        assert sum(p.type == "ReceiptId" for p in frame.projection.exports) == 1
    if case == "D02":
        event = next(
            p.evidence.value
            for p in ports
            if p.evidence and isinstance(p.evidence.value, EventRef)
        )
        assert event.subject is None
        assert event.actor == PlayerRef(kind="PlayerRef", player="self")
        assert {s.role for s in frame.leaf_schema.slots} >= {
            "declaration_lower_bound",
            "declaration_upper_bound",
            "declaration_match",
        }
    if case == "A01":
        assert any(
            p.evidence and isinstance(p.evidence.value, AbilitySetRef) for p in ports
        )
        assert not any(p.type == "ReceiptId" for p in frame.projection.exports)
    if case in {"X02", "X02-paid"}:
        slot = next(s for s in frame.leaf_schema.slots if s.role == "choice_mode_count")
        assert slot.domain.values == (CHOICE_DOMAIN,)
        assert isinstance(binding.values[slot.name], QuantitySpec)


@pytest.mark.parametrize(("case", "raw", "ordinal"), CASES, ids=[x[0] for x in CASES])
def test_wrong_units_cannot_discharge_source_veto(
    case: str, raw: str, ordinal: int, engine: Classifier
) -> None:
    bad = (
        raw.replace("枚", "人")
        .replace("体", "人")
        .replace("個", "人")
        .replace("つ", "人")
    )
    if bad == raw:
        bad = raw.replace("6", "7").replace("5", "4")
    found = derive(bad, engine, case)
    assert found.recognized[ordinal].issues or not any(
        r.startswith("N3-749") for r in found.derived.registry_rows[ordinal]
    )


@pytest.mark.parametrize("kind", ["EnterField", "Play", "Damage"])
def test_existing_events_require_object_subject(kind: str) -> None:
    payload = {"kind": "EventRef", "event_kind": kind, "event_id": "event:synthetic"}
    subject = ObjectGeneration(object_id="card:synthetic", generation=1)
    assert EventRef.model_validate({**payload, "subject": subject}).subject == subject
    for invalid in (
        {},
        {"subject": PlayerRef(kind="PlayerRef", player="self")},
        {"subject": subject, "actor": PlayerRef(kind="PlayerRef", player="self")},
    ):
        with pytest.raises(ValueError, match=r".+"):
            EventRef.model_validate({**payload, **invalid})


def test_dice_event_requires_actor_and_forbids_object_subject() -> None:
    payload = {
        "kind": "EventRef",
        "event_kind": "DiceRoll",
        "event_id": "event:synthetic",
    }
    actor = PlayerRef(kind="PlayerRef", player="self")
    assert EventRef.model_validate({**payload, "actor": actor}).actor == actor
    for invalid in (
        {},
        {"subject": ObjectGeneration(object_id="card:synthetic", generation=0)},
        {
            "actor": actor,
            "subject": ObjectGeneration(object_id="card:synthetic", generation=0),
        },
    ):
        with pytest.raises(ValueError, match=r".+"):
            EventRef.model_validate({**payload, **invalid})


@pytest.mark.parametrize(
    ("counts", "expected", "valid"),
    [
        ((4, 10), (True, True), (1,)),
        ((4, 9), (True, False), (0, 1)),
        ((10, 12), (True, True), (0, 1)),
        ((3, 2), (False, False), ()),
        ((0, 10), (True, True), (1,)),
        ((), (False, False), ()),
    ],
)
def test_highest_satisfied_witness(
    counts: tuple[int, ...], expected: tuple[bool, bool], valid: tuple[int, ...]
) -> None:
    holders = tuple(
        ObjectGeneration(object_id=f"card:{i}", generation=2)
        for i in range(len(counts))
    )
    candidates = dict(zip(holders, counts, strict=True))
    if not valid:
        assert verify_counter_witness(candidates, None, None) == expected
    for index, holder in enumerate(holders):
        if index in valid:
            assert verify_counter_witness(candidates, holder, holder) == expected
        else:
            with pytest.raises(ValueError, match=r".+"):
                verify_counter_witness(candidates, holder, holder)
    if len(holders) == 2:
        with pytest.raises(ValueError, match="same holder"):
            verify_counter_witness(candidates, holders[0], holders[1])
        stale = holders[0].model_copy(update={"generation": 3})
        with pytest.raises(ValueError, match=r".+"):
            verify_counter_witness(candidates, stale, stale)


@pytest.mark.parametrize(
    "case", ["P03", "P04", "H01", "D02", "X01", "X02", "R02", "A01"]
)
def test_source_context_mutations_are_rejected(case: str, engine: Classifier) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == case)
    found = derive(raw, engine, case)
    frame, binding = found.bind(ordinal)
    assert frame.projection.imports
    port = frame.projection.imports[0]
    assert port.evidence is not None
    for mutation in (
        port.model_copy(update={"scope": "other_ability"}),
        port.model_copy(
            update={"evidence": port.evidence.model_copy(update={"capture": "lki"})}
        ),
        port.model_copy(
            update={
                "evidence": port.evidence.model_copy(
                    update={
                        "source": port.evidence.source.model_copy(
                            update={"source_hash": "sha256:" + "d" * 64}
                        )
                    }
                )
            }
        ),
    ):
        invalid = frame.model_copy(
            update={
                "projection": frame.projection.model_copy(
                    update={"imports": (mutation, *frame.projection.imports[1:])}
                )
            }
        )
        with pytest.raises(ValueError, match=r".+"):
            found.verify_binding(raw, engine, invalid, binding)


@pytest.mark.parametrize(
    ("lower", "upper", "count"), [(0, 6, 1), (1, 7, 1), (6, 1, 1), (1, 6, 2)]
)
def test_declaration_guards(
    lower: int, upper: int, count: int, engine: Classifier
) -> None:
    raw = f"仮。{lower}から{upper}の中から、好きな数{count}つを指定してサイコロをふる。指定した数なら、仮。"
    found = derive(raw, engine)
    assert found.recognized[0].issues


@pytest.mark.parametrize(
    "labels", [(1, 2), (1, 1, 3), (1, 3, 2), (1, 2, 4), (1, 2, 3, 4)]
)
def test_choice_requires_three_ordered_modes(
    labels: tuple[int, ...], engine: Classifier
) -> None:
    raw = (
        "仮。下記からX個までチョイスする。"
        + "".join(f"【{n}】1枚引く。" for n in labels)
        + "Xは「自分の場の仮族・カードの枚数」である。"
    )
    found = derive(raw, engine)
    assert found.recognized[0].issues


@pytest.mark.parametrize(
    ("case", "key"),
    [
        ("D02", "term:object.die"),
        ("H01", "term:object.counter"),
        ("X01", "term:trait.synthetic"),
        ("A01", "term:rule.ability"),
    ],
)
def test_required_words_cannot_be_literal_fallback(
    case: str, key: str, engine: Classifier
) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == case)
    incomplete = classifier(
        tuple(t for t in engine.terms.values() if t.id != key),
        engine.rules.enabled(),
        vocabulary=engine.references.vocabulary,
    )
    found = derive(raw, incomplete, case)
    assert found.recognized[ordinal].issues


def test_holder_policy_does_not_apply_to_other_revisions(engine: Classifier) -> None:
    _, raw, _ = next(c for c in CASES if c[0] == "H01")
    found = derive(raw, engine)
    assert found.recognized[0].issues


@pytest.mark.parametrize(
    ("case", "before", "after"),
    [
        ("P02", "10枚まで探す", "9枚まで探す"),
        ("P01", "9枚", "8枚"),
        ("P01-pair", "1枚", "2枚"),
        ("P03", "3枚", "4枚"),
        ("P04", "1体につき", "2体につき"),
        ("H01", "4個", "3個"),
        ("H01", "10個", "9個"),
        ("H02", "2個", "3個"),
        ("H03", "1つ", "2つ"),
        ("H03", "が0", "が1"),
        ("H04", "片』1つ", "片』2つ"),
        ("D01", "に6が", "に7が"),
        ("X01", "コスト1", "コスト2"),
        ("X01", "Xは2", "Xは3"),
        ("R01", "が1枚", "が2枚"),
        ("R02", "が5以上", "が4以上"),
        ("A01", "能力1つ", "能力2つ"),
    ],
)
def test_each_sealed_numeric_guard(
    case: str, before: str, after: str, engine: Classifier
) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == case)
    assert before in raw
    found = derive(raw.replace(before, after), engine, case)
    assert found.recognized[ordinal].issues or not any(
        r.startswith("N3-749") for r in found.derived.registry_rows[ordinal]
    )


@pytest.mark.parametrize(
    ("case", "before", "after"),
    [
        ("P01", "。好きな枚数をEX", "。【進化時】好きな枚数をEX"),
        ("P01", "。好きな枚数をEX", "。【2】好きな枚数をEX"),
        ("P04", "。これによって", "。【進化時】これによって"),
        ("H01", "。10個", "。【進化時】10個"),
        ("H01", "。10個", "。【2】10個"),
        ("X02", "【2】", "【進化時】【2】"),
        ("X02", "自分の場", "相手の場"),
        ("X02", "自分の場", "自分の墓場"),
        ("X02", "仮族・カード", "未知族・カード"),
        ("X02", "仮族・カード", "仮族・フォロワー"),
        ("H04", "仮土片", "仮甲"),
        ("R01", "自分のデッキ", "{起動}自分のデッキ"),
        ("R01", "手札をすべて捨てる。", "手札をすべて捨てた。"),
        ("R02", "そのダメージ", "それの体力"),
        ("A01", "これの", "相手の"),
    ],
)
def test_neighboring_sources_cannot_borrow_providers(
    case: str, before: str, after: str, engine: Classifier
) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == case)
    found = derive(raw.replace(before, after), engine, case)
    assert found.recognized[ordinal].issues or not any(
        r.startswith("N3-749") for r in found.derived.registry_rows[ordinal]
    )


@pytest.mark.parametrize(
    "case", ["H01", "H02", "D01", "D02", "X01", "X02", "X02-paid", "R02"]
)
def test_normal_renderer_requires_complete_typed_presentation(
    case: str, engine: Classifier
) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == case)
    found = derive(raw, engine, case)
    frame, binding = found.bind(ordinal)
    dependencies = target_dependencies(frame, binding, registry=found.registry)
    nested = {n for names in dependencies.values() for n in names}
    presentations = _presentations(found, frame, binding)
    labels = _labels(engine, frame, binding)
    selected = SelectedTarget(
        Target(
            format=1,
            nodes=tuple(
                LeafRef(kind="LeafRef", slot=s.name)
                for s in frame.leaf_schema.slots
                if s.required and s.name not in nested
            ),
        )
    )
    plan = expression_plan(
        (BoundTarget(frame, binding, selected),),
        ExpressionTargets(presentations, registry=found.registry),
    )
    result = Renderer(labels, {}, {}, domains=engine.domains).render(
        "ctx:synthetic", "zh-Hant", plan
    )
    assert result.rendered is not None
    if case == "H02":
        assert "至多2" in result.rendered.text
    if case == "X02":
        assert "至多" in result.rendered.text
        assert result.rendered.annotation.occurrences
    if presentations:
        missing = expression_plan(
            (BoundTarget(frame, binding, selected),),
            ExpressionTargets({}, registry=found.registry),
        )
        assert (
            Renderer(labels, {}, {}, domains=engine.domains)
            .render("ctx:synthetic", "zh-Hant", missing)
            .rendered
            is None
        )
    for key in labels:
        incomplete = {k: v for k, v in labels.items() if k != key}
        assert (
            Renderer(incomplete, {}, {}, domains=engine.domains)
            .render("ctx:synthetic", "zh-Hant", plan)
            .rendered
            is None
        )


def _presentations(
    found: DerivedField, frame: Frame, binding: SourceBinding
) -> dict[tuple[str, str], SelectedTarget]:
    targets = ExpressionTargets({}, registry=found.registry)
    presentations = {}
    for value in binding.values.values():
        expr = expression_node(value)
        if isinstance(expr, (Expression, ContextExpression, PredicateExpr)):
            args = (
                definition(expr).arguments(expr, frame.leaf_schema)
                if isinstance(expr, Expression)
                else found.registry.definition(expr).arguments(expr, frame.leaf_schema)
            )
            if isinstance(expr, (ContextExpression, PredicateExpr)):
                args.update(targets.import_arguments(expr, frame.leaf_schema, frame))
            presentations[expr.expression_id, "zh-Hant"] = SelectedTarget(
                Target(
                    format=1,
                    nodes=tuple(LeafRef(kind="LeafRef", slot=name) for name in args)
                    or (LiteralNode(kind="Literal", text="仮值"),),
                )
            )
    return presentations


def _labels(
    engine: Classifier, frame: Frame, binding: SourceBinding
) -> dict[tuple[bytes, str], Label]:
    renderer = Renderer({}, {}, {}, domains=engine.domains)
    labels = {}
    for slot in frame.leaf_schema.slots:
        value = binding.values[slot.name]
        reference = (
            value
            if isinstance(
                value, (GlossaryReference, VocabularyReference, CardNameReference)
            )
            else renderer.code_references.get(
                (slot.type, value[0] if isinstance(value, tuple) else value)
            )
            if isinstance(value, (str, tuple))
            else None
        )
        if reference is not None:
            labels[canonical(reference.model_dump(mode="json")), "zh-Hant"] = Label(
                reference, "zh-Hant", "仮標籤", "project", False, True
            )
    return labels


@pytest.mark.parametrize(
    ("mode", "error"),
    [
        ("pool", "Remainder refers to another pool"),
        ("receipt", "Pool requires its same-operation receipt export"),
        ("capture", "Expression provider has incompatible source or capture"),
        ("pair", "Pool export/import pair differs in identity or evidence"),
        ("actor", "Capacity placement requires one live source actor"),
    ],
)
def test_pool_verifier_rejects_wrong_connections(
    mode: str, error: str, engine: Classifier
) -> None:

    _, raw, ordinal = next(c for c in CASES if c[0] == "P03")
    found = derive(raw, engine)
    frame, binding = found.bind(ordinal)
    ports = list(frame.projection.imports)
    if mode in {"pool", "receipt"}:
        index = next(
            i
            for i, p in enumerate(ports)
            if p.evidence
            and isinstance(p.evidence.value, ObjectSetRef)
            and p.evidence.value.origin == "pool_remainder"
        )
        port = ports[index]
        assert port.evidence is not None
        value = port.evidence.value.model_copy(
            update={"pool" if mode == "pool" else "placement": "other_provider"}
        )
        ports[index] = port.model_copy(
            update={"evidence": port.evidence.model_copy(update={"value": value})}
        )
    elif mode == "capture":
        ports[0] = (
            ports[0].model_copy(
                update={
                    "evidence": ports[0].evidence.model_copy(
                        update={"capture": "receipt"}
                    )
                }
            )
            if ports[0].evidence
            else ports[0]
        )
    elif mode == "actor":
        ports = [p for p in ports if p.source_role != "source_actor"]
    projection = frame.projection.model_copy(
        update={
            "imports": tuple(ports),
            "exports": () if mode == "pair" else frame.projection.exports,
        }
    )
    with pytest.raises(ValueError, match=error):
        verify_pool_allocation(
            frame.model_copy(update={"projection": projection}), binding
        )


@pytest.mark.parametrize("mutation", ["constant", "mode", "expression"])
def test_context_domain_cannot_borrow_a_constant_or_other_provider(
    mutation: str, engine: Classifier
) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == "X02")
    found = derive(raw, engine)
    frame, binding = found.bind(ordinal)
    slot = next(s for s in frame.leaf_schema.slots if s.role == "choice_mode_count")
    value = binding.values[slot.name]
    assert isinstance(value, QuantitySpec)
    assert isinstance(value.expr, Expression)
    invalid = (
        value.model_copy(update={"expr": Constant(kind="constant", value=1)})
        if mutation == "constant"
        else value.model_copy(update={"mode": "exact"})
        if mutation == "mode"
        else value.model_copy(
            update={
                "expr": value.expr.model_copy(
                    update={"expression_id": "read.offset.add.v1"}
                )
            }
        )
    )
    bad = binding.model_copy(update={"values": {**binding.values, slot.name: invalid}})
    with pytest.raises(ValueError, match="Context choice"):
        verify_quantities(frame, bad, scopes={}, registry=found.registry)


@pytest.mark.parametrize(
    "origin",
    ["holder", "event_subject", "selection", "direct_ability_source", "pool_remainder"],
)
@pytest.mark.parametrize("pool", [None, "pool_provider"])
@pytest.mark.parametrize("placement", [None, "placement_receipt"])
def test_object_set_remainder_contract(
    origin: str, pool: str | None, placement: str | None
) -> None:
    document = {
        "kind": "ObjectSet",
        "objects": ({"object_id": "object:synthetic", "generation": 0},),
        "origin": origin,
        "pool": pool,
        "placement": placement,
    }
    valid = (
        (pool is not None and placement is not None)
        if origin == "pool_remainder"
        else pool is None and placement is None
    )
    if valid:
        assert ObjectSetRef.model_validate(document).origin == origin
    else:
        with pytest.raises(
            ValueError,
            match="Pool remainder requires exactly its pool and placement ports",
        ):
            ObjectSetRef.model_validate(document)


@pytest.mark.parametrize(
    ("source_kind", "members", "receipt", "error"),
    [
        ("generated", (("name_a", "count_a"),), None, None),
        ("generated", (("name_a", "count_a"), ("name_b", "count_b")), None, None),
        ("generated", (), None, "Generated pool requires one or two name/count pairs"),
        (
            "generated",
            (("name_a", "count_a"), ("name_b", "count_b"), ("name_c", "count_c")),
            None,
            "Generated pool requires one or two name/count pairs",
        ),
        (
            "generated",
            (("name_a", "count_a"),),
            "receipt_a",
            "Generated pool requires one or two name/count pairs",
        ),
        (
            "generated",
            (("name_a", "name_a"),),
            None,
            "Pool members must remain distinct",
        ),
        (
            "generated",
            (("name_a", "count_a"), ("name_a", "count_b")),
            None,
            "Pool members must remain distinct",
        ),
        ("search_result", (), "receipt_a", None),
        ("search_result", (), None, "Search pool requires exactly one objects receipt"),
        (
            "search_result",
            (("name_a", "count_a"),),
            "receipt_a",
            "Search pool requires exactly one objects receipt",
        ),
    ],
)
def test_pool_context_contract(
    source_kind: str,
    members: tuple[tuple[str, str], ...],
    receipt: str | None,
    error: str | None,
) -> None:
    document = {
        "kind": "PoolContext",
        "pool_id": "pool:synthetic",
        "source_kind": source_kind,
        "members": members,
        "receipt": receipt,
        "scope": "ability_1",
    }
    if error is None:
        assert PoolContext.model_validate(document).members == members
    else:
        with pytest.raises(ValueError, match=error):
            PoolContext.model_validate(document)


def test_discard_definition_must_precede_deck_limit(engine: Classifier) -> None:
    _, raw, _ = next(c for c in CASES if c[0] == "R01")
    definition = "Xは「これによって捨てた手札の枚数+1」である。"
    found = derive(raw.replace(definition, "") + definition, engine)
    assert found.recognized[0].issues or not any(
        r.startswith("N3-749") for r in found.derived.registry_rows[0]
    )


def test_target_shape_requires_plain_nat_leaves(engine: Classifier) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == "P01-pair")
    frame, _ = derive(raw, engine).bind(ordinal)
    assert all(slot.expression is None for slot in frame.leaf_schema.slots)
    target = Target(
        format=1,
        nodes=tuple(
            LeafRef(kind="LeafRef", slot=s.name) for s in frame.leaf_schema.slots
        ),
    )
    verify_target_shape(frame, target, {}, "zh-Hant", {})
    omitted = next(s for s in frame.leaf_schema.slots if s.required and s.type == "Nat")
    invalid = Target(
        format=1,
        nodes=tuple(
            n
            for n in target.nodes
            if not isinstance(n, LeafRef) or n.slot != omitted.name
        ),
    )
    with pytest.raises(ValueError, match="Target must use every required leaf"):
        verify_target_shape(frame, invalid, {}, "zh-Hant", {})


def test_target_shape_defers_binding_supplied_choice_count(engine: Classifier) -> None:
    _, raw, ordinal = next(c for c in CASES if c[0] == "X02")
    frame, _ = derive(raw, engine).bind(ordinal)
    choice = next(s for s in frame.leaf_schema.slots if s.role == "choice_mode_count")
    target = Target(format=1, nodes=(LeafRef(kind="LeafRef", slot=choice.name),))
    with pytest.raises(ValueError, match="Target must use every required leaf"):
        target.verify(frame.leaf_schema, "zh-Hant", {})
    verify_target_shape(frame, target, {}, "zh-Hant", {})

"""Finite N3 source shapes bind synthetic operands without promoting whole-line semantics."""

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.four_layer import GlossaryReference
from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_kinds import KindFacts
from sve_carddb.domains.translations.four_layer_replacements import replacement_action

from .test_four_layer_classification import classifier, source
from .test_four_layer_n1 import n1_classifier

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier


def n3_classifier() -> Classifier:
    """Share the synthetic N3 catalog without coupling tests to fixture internals."""
    base = n1_classifier()
    names = tuple(
        Term("term:name." + key, "card_name", name)
        for key, name in (
            ("first", "仮甲"),
            ("second", "仮乙"),
            ("third", "仮丙"),
            ("fourth", "仮丁"),
        )
    )
    fixed = tuple(
        Term("term:" + key, category, word)
        for key, category, word in (
            ("player.self", "rule_term", "自分"),
            ("player.opponent", "rule_term", "相手"),
            ("zone.deck", "rule_term", "デッキ"),
            ("zone.evolve_deck", "rule_term", "エボルヴデッキ"),
            ("zone.cemetery", "rule_term", "墓場"),
            ("zone.ex", "rule_term", "EXエリア"),
            ("object.card_name", "rule_term", "カード名"),
            ("rule.cost", "rule_term", "コスト"),
            ("rule.original_cost", "rule_term", "元のコスト"),
            ("rule.instead", "rule_term", "代わりに"),
            ("rule.ability", "rule_term", "能力"),
            ("rule.damage", "rule_term", "ダメージ"),
            ("action.banish", "rule_term", "消滅させる"),
            ("event.evolve", "ability", "進化時"),
        )
    )
    return classifier(
        (
            *base.terms.values(),
            *names,
            *fixed,
            Term("term:object.start_amulet", "rule_term", "スタートアミュレット"),
        ),
        (
            *base.rules.enabled(),
            "suffix_unit_items",
            "suffix_damage_amount",
            "prefix_cost_delta",
            "braced_stat_reference",
        ),
        vocabulary=base.references.vocabulary,
        kind_facts=KindFacts(tuple((t.id, "follower") for t in names)),
    )


@pytest.fixture
def engine() -> Classifier:
    return n3_classifier()


@pytest.mark.parametrize(
    ("case", "raw", "role"),
    [
        ("N01", "仮。『仮甲』２体か『仮乙』３体か『仮丙』４体を場に出す。", "count"),
        ("N02", "仮。『仮甲』２体と『仮乙』３体を場に出してよい。", "count"),
        (
            "N03",
            "仮。『仮甲』『仮乙』『仮丙』『仮丁』２枚ずつEXエリアに置いてよい。",
            "count",
        ),
        ("D01", "仮。『仮甲』２枚を場かEXエリアに置いてよい。", "count"),
        (
            "D02",
            "仮。『仮甲』『仮乙』『仮丙』２枚ずつ場やEXエリアに置いてよい。",
            "count",
        ),
        ("R01", "仮。『仮甲』２体を出す。【覚醒】状態なら、代わりに３体。", "count"),
        (
            "R02",
            "仮。『仮甲』２体を出す。【覚醒】状態なら、代わりに３体まで。",
            "selection_count",
        ),
        (
            "R03",
            "仮。２枚引く。自分のEXエリアの『仮甲』が４枚なら、代わりに３枚。",
            "count",
        ),
        ("C01", "仮。自分の場の『仮甲』が２体以上なら、仮。", "existence_count"),
        (
            "C02",
            "仮。自分の墓場の仮族・カードが２枚以上か自分の墓場の{仮クラス}カードが３枚以上なら、仮。",
            "existence_count",
        ),
        ("C03", "仮。自分の墓場の枚数が２枚以上なら、仮。", "existence_count"),
        ("S01", "仮。自分の場の進化していない『仮甲』２体を選ぶ。", "selection_count"),
        ("S02", "仮。自分の場の『仮甲』や『仮乙』２体まで選ぶ。", "selection_count"),
        ("S03", "仮。相手の進化していないフォロワー２体を選ぶ。", "selection_count"),
        (
            "S04",
            "仮。自分のデッキの上２枚を見る。その中から、仮族・カード３枚を公開してデッキの上に置いてよい。残りを好きな順にデッキの下に置く。",
            "selection_count",
        ),
        (
            "S05",
            "仮。自分のエボルヴデッキの表向きの仮族・エボルヴフォロワー２枚を裏向きにしてよい。",
            "count",
        ),
        (
            "S06",
            "仮。自分の墓場や自分の消滅領域の『仮甲』２枚まで選ぶ。それをデッキに戻す。",
            "selection_count",
        ),
        (
            "S07",
            "仮。各プレイヤーは、自身の場のフォロワー２体を残し、それ以外の自身の場のフォロワーすべてを墓場に置く。",
            "selection_count",
        ),
        (
            "S08",
            "仮。自分の場か自分のEXエリアの仮族・トークン２枚を選ぶ。",
            "selection_count",
        ),
        ("S09", "仮。自分の手札２枚をデッキの上か下に置く。", "count"),
        ("S10", "仮。自分の場の『仮甲』２体と『仮乙』３つを選ぶ。", "selection_count"),
        ("S11", "仮。自分のエボルヴデッキの『仮甲』２枚をプレイしてよい。", "count"),
        (
            "P01",
            "仮。これをプレイする際、追加コストとして場の『仮甲』２体と『仮乙』３体をアクトする。",
            "count",
        ),
        (
            "P02",
            "仮。【融合】元のコスト２以上の仮族・カード３枚:これは「これをプレイする際、コストを-４する」を持つ。",
            "count",
        ),
        (
            "P03",
            "仮。{起動}EXエリアの仮族・トークン２枚を消滅し、手札のこれをEXエリアに置く:『仮甲』３体か『仮乙』４体を場に出す。",
            "count",
        ),
        (
            "L01",
            "仮。この能力は自分の場の仮族・フォロワーが２体なら、３ターンに４回使える。",
            "repeat_count",
        ),
    ],
    ids=lambda value: value if isinstance(value, str) and value.isascii() else None,
)
def test_sealed_body_shape_binds_and_rejects_wrong_units(
    case: str, raw: str, role: str, engine: Classifier
) -> None:
    if case == "S10":
        engine.references.kind_facts = KindFacts(
            (("term:name.first", "follower"), ("term:name.second", "amulet"))
        )
    derived = derive_source(raw, source(raw), engine)
    assert not derived.recognized[0].issues
    frame, binding = derived.bind(0)
    derived.verify_binding(raw, engine, frame, binding)
    assert role in {s.role for s in frame.leaf_schema.slots}
    if case in {"C02", "S04", "S08", "P02", "P03"}:
        assert "trait" in {s.role for s in frame.leaf_schema.slots}
    if case in {"S08", "P03"}:
        assert "token_filter" in {s.role for s in frame.leaf_schema.slots}
        assert all(s.type != "CardKind" for s in frame.leaf_schema.slots)
    if case in {"C02", "S04", "P02"}:
        assert "rule_term" in {s.role for s in frame.leaf_schema.slots}
    assert frame.semantic_variant.state == "pending"
    bad = re.sub(r"([0-9０-９]+)(?:体|枚|つ)", r"\1人", raw)
    rejected = derive_source(bad, source(bad), engine)
    assert rejected.recognized[0].issues


def test_static_class_token_has_no_inferred_kind(engine: Classifier) -> None:
    raw = "これが場にいる限り、自分の場の{仮クラス}トークンすべては【仮旗】を持つ。"
    derived = derive_source(raw, source(raw), engine)
    assert not derived.recognized[0].issues
    frame, binding = derived.bind(0)
    derived.verify_binding(raw, engine, frame, binding)
    assert {s.role for s in frame.leaf_schema.slots} >= {"class_filter", "token_filter"}
    assert all(s.type != "CardKind" for s in frame.leaf_schema.slots)
    bad = raw.replace("自分の場の", "自分の墓場の")
    assert derive_source(bad, source(bad), engine).recognized[0].issues


@pytest.mark.parametrize("connector", ["か", "と"])
@pytest.mark.parametrize(
    "tail",
    [
        "か『仮丙』４体と『仮丁』５体",
        "か『仮丙』４体か『仮丁』５体",
        "と『仮丙』４体か『仮丁』５体",
    ],
)
def test_new_emissions_cannot_match_a_mixed_or_long_tail(
    connector: str, tail: str, engine: Classifier
) -> None:
    raw = f"仮。『仮甲』２体{connector}『仮乙』３体{tail}を場に出す。"
    assert derive_source(raw, source(raw), engine).recognized[0].issues


@pytest.mark.parametrize(
    ("case", "raw", "field", "role", "bad"),
    [
        (
            "748-S01",
            "『仮甲』は自分のデッキに２枚まで入れることができる。",
            "section",
            "construction_copy_limit",
            "２枚まで入れる",
        ),
        (
            "748-S02",
            "【スタートアミュレット】（【スタートアミュレット】を持つカードはデッキに１種しか入れられない）",
            "effect",
            "construction_name_kind_limit",
            "１種しか",
        ),
        (
            "748-S03",
            "カード名に『仮甲』を含まないカードは、メインデッキやエボルヴデッキにそれぞれ１枚までしか入れられない。",
            "section",
            "construction_copy_limit",
            "それぞれ１枚までしか",
        ),
        (
            "748-S04",
            "【進化時】自分の墓場にあるカードの元のコストの種類数が２種類以上なら、仮。３種類以上なら、仮。４種類以上なら、仮。５種類以上なら、仮。",
            "effect",
            "threshold",
            "３種類以上",
        ),
        (
            "748-S05",
            "これをプレイする際、墓場の元のコスト１、２、３、４、５、６、７、８、９、１０のカードを１枚ずつ消滅:コストを-２する。",
            "effect",
            "count",
            "９、１０",
        ),
        (
            "748-E01",
            "【レッスン_１】仮。（【レッスン_１】とは、自分のEXエリアの『仮甲』１枚を消滅させること）",
            "effect",
            "count",
            "EXエリア",
        ),
        (
            "748-E02",
            "【ネクロチャージ_２】仮。【NC_３】仮。（自分の墓場が２枚以上なら、【ネクロチャージ_２】の後の能力が働く。３枚以上なら、さらに【NC_３】の後の能力が働く）",
            "effect",
            "existence_count",
            "さらに",
        ),
        (
            "748-E03",
            "これをプレイする際、【スペルチェイン_２】コストを-３する。【SC_４】代わりに-５する。【SC_６】代わりに-７する。",
            "effect",
            "cost_delta_magnitude",
            "代わりに-５",
        ),
        (
            "748-E04",
            "これがいる限り、自分のフォロワーが受ける「相手の能力によるダメージ」を-２する。",
            "effect",
            "damage_delta_magnitude",
            "相手の能力",
        ),
    ],
    ids=lambda value: value if isinstance(value, str) and value.isascii() else None,
)
def test_construction_and_continuation_shapes_bind_only_complete_guards(
    case: str, raw: str, field: str, role: str, bad: str, engine: Classifier
) -> None:
    derived = derive_source(raw, source(raw, field), engine)
    ordinal = 1 if case == "748-S02" else 1 if case in {"748-E01", "748-E02"} else 0
    assert not derived.recognized[ordinal].issues
    frame, binding = derived.bind(ordinal)
    derived.verify_binding(raw, engine, frame, binding)
    assert role in {s.role for s in frame.leaf_schema.slots}
    if case == "748-S04":
        assert sum(s.role == "threshold" for s in frame.leaf_schema.slots) == 4
        assert all(s.role != "read_value" for s in frame.leaf_schema.slots)
    if case == "748-S03":
        assert (
            next(s for s in frame.leaf_schema.slots if s.type == "CardName").role
            == "name_fragment"
        )
    required = {
        "748-S01": {"term:player.self", "term:zone.deck"},
        "748-S02": {"term:object.start_amulet", "term:object.card", "term:zone.deck"},
        "748-S03": {
            "term:object.card_name",
            "term:object.card",
            "term:zone.deck",
            "term:zone.evolve_deck",
        },
        "748-S04": {
            "term:event.evolve",
            "term:player.self",
            "term:zone.cemetery",
            "term:object.card",
            "term:rule.original_cost",
        },
        "748-S05": {
            "term:zone.cemetery",
            "term:object.card",
            "term:rule.original_cost",
            "term:rule.cost",
        },
        "748-E01": {
            "term:ability.lesson",
            "term:player.self",
            "term:zone.ex",
        },
        "748-E02": {
            "term:ability.necrocharge",
            "term:player.self",
            "term:zone.cemetery",
        },
        "748-E03": {"term:ability.spell_chain", "term:rule.cost", "term:rule.instead"},
        "748-E04": {
            "term:player.self",
            "term:player.opponent",
            "term:rule.ability",
            "term:rule.damage",
        },
    }
    assert required[case] <= {
        v.key for v in binding.values.values() if isinstance(v, GlossaryReference)
    }
    if case in {"748-S05", "748-E01"}:
        assert GlossaryReference(kind="glossary", key="term:action.banish") not in (
            binding.values.values()
        )
        assert len(frame.leaf_schema.slots) == (16 if case == "748-S05" else 6)
    malformed = raw.replace(bad, "仮不明")
    rejected = derive_source(malformed, source(malformed, field), engine)
    assert any(p.issues for p in rejected.recognized) or all(
        s.role != role for p in rejected.recognized for s in p.schema.slots
    )


@pytest.mark.parametrize("prefix", ["仮。", "仮。{起動}", "仮。『仮乙』２体を出す。"])
def test_replacement_cannot_cross_an_intervening_action_or_ability(
    prefix: str, engine: Classifier
) -> None:
    raw = "仮。『仮甲』２体を出す。" + prefix + "【覚醒】状態なら、代わりに３体まで。"
    assert derive_source(raw, source(raw), engine).recognized[0].issues


@pytest.mark.parametrize(
    "raw",
    [
        "仮。自分の墓場の仮族・カード２枚を公開してデッキの上に置いてよい。",
        "仮。『仮甲』『仮乙』２枚ずつ場やEXエリアに置いてよい。",
        "仮。『仮甲』『仮乙』『仮丙』『仮丁』２枚ずつ場やEXエリアに置いてよい。",
        "仮。「『仮甲』２体と『仮乙』３体を場に出す」を持つ。",
        "仮。自分の墓場の未知族・カードが２枚以上か自分の墓場の仮族・カードが３枚以上なら、仮。",
        "仮。自分の墓場の仮族・カードが２枚以上か自分の墓場の仮族・カードが３枚以上か自分の墓場の仮族・カードが４枚以上なら、仮。",
        "【レッスン_２】仮。（【レッスン_１】とは、自分のEXエリアの『仮甲』１枚を消滅させること）",
    ],
)
def test_neighboring_shapes_cannot_borrow_an_n3_rule(
    raw: str, engine: Classifier
) -> None:
    assert any(p.issues for p in derive_source(raw, source(raw), engine).recognized)


def test_or_left_existence_accepts_its_explicit_locative(engine: Classifier) -> None:
    raw = "自分の場に仮族・フォロワーが２体以上か「カード名に『仮甲』を含むフォロワー」がいる限り、これは【仮旗】を持つ。"
    derived = derive_source(raw, source(raw), engine)
    assert not derived.recognized[0].issues
    frame, binding = derived.bind(0)
    derived.verify_binding(raw, engine, frame, binding)
    assert "existence_count" in {slot.role for slot in frame.leaf_schema.slots}


def test_flat_filter_cannot_hide_a_missing_required_trait(engine: Classifier) -> None:
    raw = "自分の場か自分のEXエリアの仮族・トークン２枚を選ぶ。"
    without_trait = classifier(
        tuple(t for t in engine.terms.values() if t.category != "trait"),
        engine.rules.enabled(),
        vocabulary=engine.references.vocabulary,
    )
    derived = derive_source(raw, source(raw), without_trait)
    assert "missing_n3_filter_concept" in derived.recognized[0].issues
    with pytest.raises(ValueError, match="Unresolved source leaves"):
        derived.bind(0)


@pytest.mark.parametrize("wrong_category", [False, True])
def test_sealed_reminder_requires_its_exact_registered_concepts(
    engine: Classifier, *, wrong_category: bool
) -> None:
    raw = "【スタートアミュレット】（【スタートアミュレット】を持つカードはデッキに１種しか入れられない）"
    terms = tuple(
        t for t in engine.terms.values() if t.id != "term:object.start_amulet"
    )
    if wrong_category:
        terms += (Term("term:object.start_amulet", "ability", "スタートアミュレット"),)
    incomplete = classifier(
        terms, engine.rules.enabled(), vocabulary=engine.references.vocabulary
    )
    derived = derive_source(raw, source(raw), incomplete)
    assert "missing_n3_required_reference" in derived.recognized[1].issues


@pytest.mark.parametrize(
    ("case", "raw", "field", "evidence", "role"),
    [
        (
            "payment-multiple-copies",
            "これをプレイする際、墓場の元のコスト１、２、３、４、５、６、７、８、９、１０のカードを２枚ずつ消滅:コストを-７する。",
            "effect",
            None,
            "count",
        ),
        (
            "payment-cost-order",
            "これをプレイする際、墓場の元のコスト２、１、３、４、５、６、７、８、９、１０のカードを１枚ずつ消滅:コストを-７する。",
            "effect",
            None,
            "cost_value",
        ),
        (
            "lesson-count-mismatch",
            "【レッスン_１】仮。（【レッスン_１】とは、自分のEXエリアの『仮甲』２枚を消滅させること）",
            "effect",
            None,
            "count",
        ),
        (
            "nc-anchor-mismatch",
            "【ネクロチャージ_４】仮。【NC_６】仮。（自分の墓場が３枚以上なら、【ネクロチャージ_３】の後の能力が働く。６枚以上なら、さらに【NC_６】の後の能力が働く）",
            "effect",
            None,
            "existence_count",
        ),
        (
            "nc-descending",
            "【ネクロチャージ_６】仮。【NC_３】仮。（自分の墓場が６枚以上なら、【ネクロチャージ_６】の後の能力が働く。３枚以上なら、さらに【NC_３】の後の能力が働く）",
            "effect",
            None,
            "existence_count",
        ),
        (
            "sc-descending",
            "これをプレイする際、【スペルチェイン_６】コストを-２する。【SC_４】代わりに-３する。【SC_２】代わりに-４する。",
            "effect",
            None,
            "cost_delta_magnitude",
        ),
        (
            "cost-kinds-unordered",
            "【進化時】自分の墓場にあるカードの元のコストの種類数が５種類以上なら、仮。３種類以上なら、仮。４種類以上なら、仮。２種類以上なら、仮。",
            "effect",
            None,
            "threshold",
        ),
        (
            "cost-kinds-new-head",
            "【進化時】自分の墓場にあるカードの元のコストの種類数が２種類以上なら、仮。３種類以上なら、仮。【攻撃時】４種類以上なら、仮。５種類以上なら、仮。",
            "effect",
            None,
            "threshold",
        ),
        (
            "selection-heterogeneous-kinds",
            "仮。自分の場の『仮甲』や『仮乙』２体まで選ぶ。",
            "effect",
            (("term:name.first", "follower"), ("term:name.second", "amulet")),
            "selection_count",
        ),
        (
            "selection-missing-kind",
            "仮。自分の場の『仮甲』や『仮乙』２体まで選ぶ。",
            "effect",
            (("term:name.first", "follower"),),
            "selection_count",
        ),
        (
            "selection-ambiguous-kind",
            "仮。自分の場の『仮甲』や『仮乙』２体まで選ぶ。",
            "effect",
            (
                ("term:name.first", "follower"),
                ("term:name.first", "amulet"),
                ("term:name.second", "follower"),
            ),
            "selection_count",
        ),
        (
            "existence-missing-kind",
            "仮。自分の場の『仮甲』が２体以上なら、仮。",
            "effect",
            (),
            "existence_count",
        ),
        (
            "construction-effect-field",
            "『仮甲』は自分のデッキに２枚まで入れることができる。",
            "effect",
            None,
            "construction_copy_limit",
        ),
        (
            "construction-evolve-deck",
            "『仮甲』は自分のエボルヴデッキに２枚まで入れることができる。",
            "section",
            None,
            "construction_copy_limit",
        ),
        (
            "start-amulet-two-kinds",
            "【スタートアミュレット】（【スタートアミュレット】を持つカードはデッキに２種しか入れられない）",
            "effect",
            None,
            "construction_name_kind_limit",
        ),
        (
            "damage-positive-sign",
            "これがいる限り、自分のフォロワーが受ける「相手の能力によるダメージ」を+２する。",
            "effect",
            None,
            "damage_delta_magnitude",
        ),
    ],
    ids=lambda value: value if isinstance(value, str) and value.isascii() else None,
)
def test_sealed_numeric_and_kind_guards(
    case: str,
    raw: str,
    field: str,
    evidence: tuple[tuple[str, str], ...] | None,
    role: str,
    engine: Classifier,
) -> None:
    if evidence is not None:
        engine.references.kind_facts = KindFacts(evidence)
    derived = derive_source(raw, source(raw, field), engine)
    assert any(p.issues for p in derived.recognized) or all(
        slot.role != role for p in derived.recognized for slot in p.schema.slots
    ), case


def test_replacement_accepts_a_following_sentence_in_the_same_ability(
    engine: Classifier,
) -> None:
    raw = "仮。『仮甲』２体を出す。【覚醒】状態なら、代わりに３体まで。仮。"
    derived = derive_source(raw, source(raw), engine)
    assert not derived.recognized[0].issues
    frame, binding = derived.bind(0)
    derived.verify_binding(raw, engine, frame, binding)
    assert "selection_count" in {slot.role for slot in frame.leaf_schema.slots}


def test_replacement_followup_cannot_borrow_an_action_before_a_new_head(
    engine: Classifier,
) -> None:
    raw = "仮。『仮甲』２体を出す。【攻撃時】【覚醒】状態なら、代わりに３体まで。仮。"
    assert derive_source(raw, source(raw), engine).recognized[0].issues
    assert (
        replacement_action(
            "仮。『X』N体を出す。【攻撃時】【覚醒】状態なら、代わりに",
            "体まで。仮。",
        )
        is None
    )


@pytest.mark.parametrize(
    "head", ["【攻撃時】", "【進化時】", "【超進化時】", "{進化}{コスト}N:"]
)
def test_replacement_uses_the_shared_head_to_exclude_an_earlier_action(
    head: str,
) -> None:
    before = "『X』N体を出す。" + head + "『X』N体を出す。【覚醒】状態なら、代わりに"
    assert replacement_action(before, "体まで。仮。") is not None

"""Finite N3 source shapes bind synthetic operands without promoting whole-line semantics."""

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.translations.four_layer_classification import Term
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_kinds import KindFacts

from .test_four_layer_classification import classifier, source
from .test_four_layer_n1 import n1_classifier

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier


@pytest.fixture
def engine() -> Classifier:
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
    return classifier(
        (
            *base.terms.values(),
            *names,
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

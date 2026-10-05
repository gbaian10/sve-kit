"""Finite lexical roles for explicit counters, people and deck positions."""

from dataclasses import dataclass
from typing import Literal


@dataclass(frozen=True)
class ExplicitRule:
    role: str
    before: str
    after: str
    minimum: int = 0
    companion: Literal["prefix", "suffix"] | None = None


# An unrecognized name must remain pending rather than borrowing a suffix of a known one.
COUNTER_NAMES = (
    "かばばん君",
    "ギガ",
    "矢",
    "祈り",
    "加虐",
    "融合",
    "戦意",
    "情熱",
    "神湯",
    "雷",
    "休眠",
    "災い",
    "返戻",
    "恩寵",
    "童話",
    "呪い",
    "魔力",
    "四季",
)
COUNTER = "(?:" + "|".join(COUNTER_NAMES) + ")カウンター"

EXPLICIT = {
    "named_counter_place": ExplicitRule(
        "counter_place_quantity",
        "に" + COUNTER + "$",
        r"^個(?:まで)?を置く(?=[。:、）)])",
    ),
    "named_counter_remove": ExplicitRule(
        "counter_remove_quantity", "の" + COUNTER + "$", r"^個を取る(?=[。:、）)])"
    ),
    "named_counter_threshold": ExplicitRule(
        "counter_threshold",
        "の" + COUNTER + "が$",
        r"^個(?:以上|以下)?(?:なら|である限り)",
    ),
    "named_counter_group_size": ExplicitRule(
        "counter_group_size", "の" + COUNTER + "$", r"^個につき", 1
    ),
    "leader_person_quantity": ExplicitRule(
        "leader_person_quantity", r"(?:自分|相手)のリーダー$", r"^人(?:か|を|の)"
    ),
    "player_person_quantity": ExplicitRule(
        "player_person_quantity", r"相手プレイヤー$", r"^人(?:の|は|を)"
    ),
    "deck_top_ordinal": ExplicitRule(
        "deck_top_ordinal", r"(?:自分の|相手の)?デッキの上から$", r"^番目(?:に|か)", 1
    ),
    "cost_assignment": ExplicitRule(
        "cost_assigned_value",
        r"(?:^|[、。:]|それの|これの|\{進化\})コストを$",
        r"^に(?:する|してプレイする)(?=[。:、）)])",
    ),
    "original_cost_bound": ExplicitRule(
        "original_cost_bound", r"の元のコストが$", r"^(?:以上|以下)?なら"
    ),
    "original_cost_sum_bound": ExplicitRule(
        "original_cost_sum_bound",
        r"元のコストの合計が$",
        r"^(?:以上|以下)(?:になるように|なら)",
    ),
    "pp_capacity_bound": ExplicitRule(
        "pp_capacity_bound",
        r"(?:自分|相手|自分と相手)のPP最大値が$",
        r"^(?:以上|以下)?なら",
    ),
    "pp_remaining_bound": ExplicitRule(
        "pp_remaining_bound",
        r"(?:自分|相手)の残りPPが$",
        r"^(?:以上|以下)?なら",
    ),
    "attack_assignment": ExplicitRule(
        "attack_assigned_value",
        r"(?:これ|それ)の攻撃力を$",
        r"^にする(?=[。:、）)])",
    ),
    "health_assignment": ExplicitRule(
        "health_assigned_value",
        r"(?:これ|それ)の体力を$",
        r"^にする(?=[。:、）)])",
    ),
    "attack_health_assignment": ExplicitRule(
        "attack_health_assigned_value",
        r"(?:これ|それ)の攻撃力と体力を$",
        r"^にする(?=[。:、）)])",
    ),
    "leader_health_assignment": ExplicitRule(
        "leader_health_assigned_value",
        r"(?:自分|相手)のリーダー(?:すべて)?の(?:体力|\{体力\})を$",
        r"^にする(?=[。:、）)])",
    ),
    "leader_health_bound": ExplicitRule(
        "leader_health_bound",
        r"(?:自分|相手)のリーダーの(?:体力が|\{体力\}(?:が)?)$",
        r"^(?:以上|以下)?(?:なら|である限り|になったとき)",
    ),
    "attack_bound": ExplicitRule(
        "attack_bound",
        r"(?:これ|それ)の攻撃力が$",
        r"^(?:以上|以下)?(?:なら|である限り)",
    ),
    "attack_sum_bound": ExplicitRule(
        "attack_sum_bound", r"の攻撃力の合計が$", r"^(?:以上|以下)?なら"
    ),
    "distinct_card_name_count": ExplicitRule(
        "distinct_card_name_count", r"のカード名の種類数が$", r"^種類以上なら"
    ),
    "distinct_original_cost_count": ExplicitRule(
        "distinct_original_cost_count",
        r"カードの元のコストの種類数が$",
        r"^種類以上なら",
    ),
    "damage_count_multiplier": ExplicitRule(
        "damage_count_multiplier",
        r"「(?:[^「」]+の(?:数|枚数)|戻した枚数)」の$",
        r"^倍のダメージ(?=[。:、）」])",
    ),
    "damage_attack_multiplier": ExplicitRule(
        "damage_attack_multiplier",
        r"「[^「」]+の攻撃力」の$",
        r"^倍のダメージ(?=[。:、）」])",
    ),
    "count_formula_multiplier": ExplicitRule(
        "count_formula_multiplier",
        r"Xは「[^「」]+の(?:数|枚数)の$",
        r"^倍」である(?=[。:、）)])",
    ),
    "attack_damage_multiplier": ExplicitRule(
        "attack_damage_multiplier",
        r"が与える「リーダーへの攻撃ダメージ」と「交戦ダメージ」を$",
        r"^倍にする(?=[。:、）)])",
    ),
    "received_damage_lower_bound": ExplicitRule(
        "received_damage_lower_bound",
        r"が受ける$",
        r"^以上のダメージを(?P<companion>N)にする(?=[。:、）」])",
        companion="suffix",
    ),
    "received_damage_assignment": ExplicitRule(
        "received_damage_assigned_value",
        r"が受ける(?P<companion>N)以上のダメージを$",
        r"^にする(?=[。:、）」])",
        companion="prefix",
    ),
}

"""One registry couples sealed N3 source shapes to numeric roles and fixed vocabulary."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

if TYPE_CHECKING:
    from collections.abc import Mapping

DIGIT = r"(?:N(?![A-Za-z])|[0-9０-９]+)"
FUSION_MAX = 2
CHOICE_DOMAIN = "quantity.choice_mode_count.context.v1"
CHOSEN_X_EXPRESSION = "read.parameter.chosen_x.v1"
ABILITY_REPLAY = re.compile(
    r"これの(?:【進化時】|{進化時})能力(?P<n>" + DIGIT + r")つをプレイする。$"
)
EX_COUNTER_SUBJECT = r"(?P<owner>自分)の(?P<zone>EXエリア)のカード(?P<count>N)枚"
EX_COUNTER_READ = re.compile(
    EX_COUNTER_SUBJECT
    + r"の情熱(?P<counter>カウンター)(?P<read>が)(?P<n>N)個(?P<compare>以上)なら、"
)

NumberRule = tuple[str, str | None]


@dataclass(frozen=True)
class Shape:
    pattern: str
    numbers: Mapping[str, NumberRule]
    words: tuple[str, ...]
    role: Literal["body", "reminder"] = "body"
    section_only: bool = False

    def match(self, text: str) -> re.Match[str] | None:
        """Recognize a complete shape before applying its numeric guards or references."""
        return re.fullmatch(self.pattern, text)


SHAPES = {
    "construction": Shape(
        r"『X』は自分のデッキに(?P<n>N)枚まで入れることができる。",
        {"n": ("construction_copy_limit", "枚")},
        ("term:player.self", "term:zone.deck"),
        section_only=True,
    ),
    "construction_fragment": Shape(
        r"カード名に『X』を含まないカードは、メインデッキやエボルヴデッキにそれぞれ(?P<n>N)枚までしか入れられない。",
        {"n": ("construction_copy_limit", "枚")},
        (
            "term:object.card_name",
            "term:object.card",
            "term:zone.deck",
            "term:zone.evolve_deck",
        ),
        section_only=True,
    ),
    "payment": Shape(
        r"これをプレイする際、墓場の元のコスト(?P<costs>N(?:、N){9})のカードを(?P<n>N)枚ずつ消滅:コストを-Nする。",
        {"costs": ("cost_value", None), "n": ("count", "枚")},
        (
            "term:object.card",
            "term:rule.original_cost",
            "term:rule.cost",
            "term:zone.cemetery",
        ),
    ),
    "spell_chain": Shape(
        r"これをプレイする際、【スペルチェイン_N】コストを-Nする。【(?:SC|スペルチェイン)_N】代わりに-Nする。【(?:SC|スペルチェイン)_N】代わりに-Nする。",
        {"replacement": ("cost_delta_magnitude", None)},
        ("term:ability.spell_chain", "term:rule.cost", "term:rule.instead"),
    ),
    "damage": Shape(
        r"これがいる限り、自分のフォロワーが受ける「相手の能力によるダメージ」を-Nする。",
        {"n": ("damage_delta_magnitude", None)},
        (
            "term:player.self",
            "term:player.opponent",
            "type/follower",
            "term:rule.ability",
            "term:rule.damage",
        ),
    ),
    "cost_kinds": Shape(
        r"【進化時】自分の墓場にあるカードの元のコストの種類数がN種類以上なら、[^。]+。N種類以上なら、[^。]+。N種類以上なら、[^。]+。N種類以上なら、[^。]+。",
        {"thresholds": ("threshold", "種類")},
        (
            "term:event.evolve",
            "term:player.self",
            "term:player.opponent",
            "term:zone.cemetery",
            "term:object.card",
            "term:rule.original_cost",
            "term:stat.health",
            "type/follower",
            "type/leader",
            "term:rule.damage",
        ),
    ),
    "start_amulet": Shape(
        r"[（(]【スタートアミュレット】を持つカードはデッキに(?P<n>[1１])種しか入れられない[）)]",
        {"n": ("construction_name_kind_limit", "種")},
        ("term:object.start_amulet", "term:object.card", "term:zone.deck"),
        role="reminder",
    ),
    "lesson": Shape(
        r"[（(]【レッスン_(?P<parameter>"
        + DIGIT
        + r")】とは、自分のEXエリアの『[^』]+』(?P<count>"
        + DIGIT
        + r")枚を消滅させること[）)]",
        {"parameter": ("threshold", None), "count": ("count", "枚")},
        (
            "term:ability.lesson",
            "term:player.self",
            "term:zone.ex",
        ),
        role="reminder",
    ),
    "necrocharge": Shape(
        r"[（(]自分の墓場が(?P<a>"
        + DIGIT
        + r")枚以上なら、【ネクロチャージ_(?P<b>"
        + DIGIT
        + r")】の後の能力が働く。(?P<c>"
        + DIGIT
        + r")枚以上なら、さらに【NC_(?P<d>"
        + DIGIT
        + r")】の後の能力が働く[）)]",
        {"count": ("existence_count", "枚"), "parameter": ("threshold", None)},
        ("term:ability.necrocharge", "term:player.self", "term:zone.cemetery"),
        role="reminder",
    ),
}

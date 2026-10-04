"""Finite lexical roles for explicit counters, people and deck positions."""

from dataclasses import dataclass


@dataclass(frozen=True)
class ExplicitRule:
    role: str
    before: str
    after: str
    minimum: int = 0


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
}

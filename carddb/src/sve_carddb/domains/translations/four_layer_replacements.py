"""Numeric replacements inherit only an adjacent complete source action and condition."""

import re
from dataclasses import dataclass
from typing import Literal

from sve_carddb.domains.translations.four_layer_units import (
    CountContext,
    count_context,
    source_unit,
    written_card_np,
)

_MARKER = re.compile(
    r"(?:【(?:ネクロチャージ|スペルチェイン|コンボ)(?:_| )?N】|【土の秘術】)$"
)
_ACTION = re.compile(
    r"(?:^|[。:：}】])(?P<action>"
    r"N枚引く|"
    r"(?P<np>[^。:：]+?)N(?P<unit>枚|体|つ)(?:を)?墓場に置く|"
    r"(?P<search>[^。:：]+?)N枚(?:を)?探し、EXエリアに置く|"
    r"『X』N枚か『X』N枚をEXエリアに置く)"
    r"[。](?P<condition>[^。:：]*)代わりに$"
)


@dataclass(frozen=True)
class Replacement:
    role: Literal["count", "selection_count"]
    unit: str
    mode: Literal["exact", "up_to"]
    context: CountContext | None


def _condition(text: str) -> bool:
    if _MARKER.fullmatch(text) or text == "【真紅】状態なら、":
        return True
    if match := re.fullmatch(r"(?P<np>.+)を捨てたなら、", text):
        return written_card_np(match["np"])
    if match := re.fullmatch(r"(?P<np>.+?)(?:が)?(?:いる|ある)なら、", text):
        return count_context(match["np"]) is not None
    if match := re.fullmatch(r"(?P<np>.+?)N(?P<unit>枚|体|つ)以上なら、", text):
        context = count_context(match["np"])
        return context is not None and source_unit(context, match["unit"]).merge_allowed
    return False


def replacement_action(before: str, after: str) -> Replacement | None:
    """A new ability or sentence cannot bridge the original action and its replacement."""
    suffix = re.fullmatch(r"(?P<unit>枚|体|つ)(?P<limit>まで|ずつ置く)?[。]", after)
    match = _ACTION.search(before)
    if suffix is None or match is None or not _condition(match["condition"]):
        return None
    unit = suffix["unit"]
    if match["action"] == "N枚引く":
        return (
            Replacement("count", unit, "exact", None)
            if unit == "枚" and not suffix["limit"]
            else None
        )
    if match["action"].startswith("『X』"):
        return (
            Replacement("count", unit, "exact", None)
            if unit == "枚" and suffix["limit"] == "ずつ置く"
            else None
        )
    np = match["search"] or match["np"]
    context = count_context(np)
    original_unit = "枚" if match["search"] else match["unit"]
    if (
        context is None
        or unit != original_unit
        or not source_unit(context, unit).merge_allowed
    ):
        return None
    if suffix["limit"] not in {None, "まで"} or (
        suffix["limit"] == "まで" and not match["search"]
    ):
        return None
    return Replacement(
        "selection_count" if suffix["limit"] else "count",
        unit,
        "up_to" if suffix["limit"] else "exact",
        context,
    )

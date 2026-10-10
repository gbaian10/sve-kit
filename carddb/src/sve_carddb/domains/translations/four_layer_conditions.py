"""Prove a repeated numeric predicate's one explicit counted set in its source ability."""

import re
from typing import TYPE_CHECKING

from sve_carddb.domains.translations.four_layer_units import count_context, source_unit

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_units import CountContext

_HEAD = re.compile(
    r"(?:^|[。\n])\s*(?:\{(?:ファンファーレ|起動|ラストワード|進化|食事|憑依)\}|"
    r"【(?:進化時|攻撃時|超進化時|(?:自分|相手)のターン(?:開始時|終了時)|N)】)"
)
_CONDITION = re.compile(
    r"(?:^|[。:：、\n])(?P<np>[^。:：、\n]+?)N(?P<unit>枚|体|つ|人)"
    r"(?:以上|以下)?なら(?=[、。])"
)


def existence_context(before: str) -> CountContext | None:
    """A bare repeated predicate cannot borrow another ability or an ambiguous antecedent."""
    if not before.endswith(("。", "、")):
        return None
    start = max((head.end() for head in _HEAD.finditer(before)), default=0)
    scope = before[start:]
    depth = scope.count("「") - scope.count("」")
    if depth < 0:
        return None
    evidence: dict[str, CountContext] = {}
    for condition in _CONDITION.finditer(scope):
        prefix = scope[: condition.start("unit")]
        if prefix.count("「") - prefix.count("」") != depth:
            continue
        np = condition["np"]
        context = count_context(np)
        if context is None or not source_unit(context, condition["unit"]).merge_allowed:
            return None
        evidence[np] = context
    return next(iter(evidence.values())) if len(evidence) == 1 else None

"""Prove explicit choice leaves within one complete source ability scope."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from sve_carddb.domains.translations.parameters.models import Hint

_INTRO = re.compile(
    r"(?<![A-Za-z0-9０-９])(?P<count>[0-9０-９]+)つ(?:まで)?チョイス(?:する|して)?[。:：]"
)
_BOUNDARY = re.compile(
    r"\n\s*\n|(?:^|[。\n])\s*(?:\{(?:ファンファーレ|起動|ラストワード|進化|食事|憑依)\}|"
    r"【(?:進化時|攻撃時|超進化時)】)"
)
_MIN_OPTIONS = 2


@dataclass(frozen=True)
class _Choice:
    start: int
    end: int
    intro: re.Match[str]
    labels: tuple[re.Match[str], ...]


def _scope(raw: str, hint: Hint) -> _Choice | None:
    if len(hint.source_segments) != 1:
        return None
    position = hint.source_segments[0].start
    protected = re.sub(r"『[^』]*』", lambda m: " " * len(m[0]), raw)
    protected = re.sub(r"[（(][^（）()]*[）)]", lambda m: " " * len(m[0]), protected)
    boundaries = tuple(_BOUNDARY.finditer(protected))
    start = max((m.end() for m in boundaries if m.end() <= position), default=0)
    end = min((m.start() for m in boundaries if m.start() > position), default=len(raw))
    introductions = tuple(_INTRO.finditer(protected, start, end))
    if len(introductions) != 1:
        return None
    intro = introductions[0]
    labels = tuple(re.finditer(r"【([0-9０-９]+)】", protected[intro.end() : end]))
    if (
        len(labels) < _MIN_OPTIONS
        or not 1 <= int(intro["count"]) <= len(labels)
        or [int(m[1]) for m in labels] != list(range(1, len(labels) + 1))
        or re.search(r"【[0-9０-９]+】", protected[start : intro.start()])
    ):
        return None
    return _Choice(start, end, intro, labels)


def choice_index(raw: str, hint: Hint) -> bool:
    """Independent ability heads delimit reachability without adding cross-frame ports."""
    scope = _scope(raw, hint)
    if scope is None:
        return False
    return any(
        hint.source_segments[0].start == scope.intro.end() + label.start(1)
        and hint.source_segments[0].end == scope.intro.end() + label.end(1)
        for label in scope.labels
    )


def choice_quantity(raw: str, hint: Hint) -> bool:
    """The intro's own source position proves its finite selectable option count."""
    scope = _scope(raw, hint)
    return scope is not None and (
        hint.source_segments[0].start == scope.intro.start("count")
        and hint.source_segments[0].end == scope.intro.end("count")
    )


def choice_alternative(raw: str, hint: Hint) -> bool:
    """An explicit replacement count belongs to the same uniquely introduced options."""
    scope = _scope(raw, hint)
    if scope is None or hint.value is None or not 1 <= hint.value <= len(scope.labels):
        return False
    span = hint.source_segments[0]
    first_label = scope.intro.end() + scope.labels[0].start()
    last_label = scope.intro.end() + scope.labels[-1].end()
    prefix = raw[scope.intro.end() : span.start]
    return (
        (
            scope.intro.end() <= span.start < first_label
            and prefix.endswith("なら、代わりに")
        )
        or (
            last_label <= span.start < scope.end
            and re.search(
                r"(?:^|[。\n])【ネクロチャージ(?:[ 　]*|_)[0-9０-９]+】代わりに$",
                prefix,
            )
            is not None
        )
    ) and (re.match(r"^つまで[。:：]", raw[span.end : scope.end]) is not None)

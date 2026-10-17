"""Closed construction limits and same-ability numeric continuations for N3."""

import re
from typing import TYPE_CHECKING
from unicodedata import normalize

from sve_carddb.contracts.four_layer import FaceRevisionOwner, PrintingFaceOwner
from sve_carddb.domains.translations.four_layer_ability import HEAD
from sve_carddb.domains.translations.four_layer_n3_shapes import DIGIT, SHAPES

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_normalizer import (
        SourceField,
        SourcePart,
    )
    from sve_carddb.domains.translations.recognition.models import Candidate, Hint

_CHAIN_THRESHOLDS = 4
_NC_OCCURRENCES = 4


def is_construction_fragment(part: SourcePart, field: SourceField | None) -> bool:
    """A negated name fragment is not the complete name of a constructed card."""
    return (
        field is not None
        and field.source.field == "section"
        and part.source_span.role == "body"
        and SHAPES["construction_fragment"].match(part.canonical_source) is not None
    )


def repair_candidate(
    raw: str, part: SourcePart, candidate: Candidate, field: SourceField | None
) -> Candidate:
    """Discharge only lexical numeric vetoes proved by a complete N3 construction."""
    repairable = {"numeric_role_requires_review", "signed_numeric_requires_review"}
    hints = tuple(
        hint.model_copy(update={"issues": ()})
        if hint.type == "uint"
        and hint.value is not None
        and hint.issues
        and set(hint.issues) <= repairable
        and number_rule(raw, part, hint, field) is not None
        else hint
        for hint in candidate.slots
    )
    remaining = {issue for hint in hints for issue in hint.issues}
    return candidate.model_copy(
        update={
            "slots": hints,
            "issues": tuple(sorted(set(candidate.issues) - (repairable - remaining))),
        }
    )


def _value(raw: str, part: SourcePart, match: re.Match[str], group: str) -> int:
    positions = sorted(
        {
            i
            for u in part.units[match.start(group) : match.end(group)]
            for s in u.origins
            for i in range(s.start, s.end)
        }
    )
    return int(normalize("NFKC", "".join(raw[i] for i in positions)))


def _anchor(part: SourcePart, field: SourceField | None) -> SourcePart | None:
    index = part.source_span.anchor
    if field is None or index is None or not 0 <= index < len(field.parts):
        return None
    body = field.parts[index]
    return (
        body
        if body.source_span.role == "body" and body.line_ordinal == part.line_ordinal
        else None
    )


def number_rule(
    raw: str, part: SourcePart, hint: Hint, field: SourceField | None
) -> tuple[str, str | None] | None:
    """Return a role only at a printed operand of an entire sealed shape."""
    text = part.canonical_source
    start = hint.occurrence.start
    if (
        field is None
        or field.source.field not in {"effect", "section"}
        or not isinstance(field.source.owner, (FaceRevisionOwner, PrintingFaceOwner))
    ):
        return None
    if part.source_span.role == "reminder":
        return _reminder(raw, part, hint, field)
    if part.source_span.role != "body":
        return None
    return (
        (_construction(text, start) if field.source.field == "section" else None)
        or _payment(raw, part, start)
        or _chains(raw, part, start)
    )


def _construction(text: str, start: int) -> tuple[str, str | None] | None:
    for shape in SHAPES.values():
        if (
            shape.section_only
            and (match := shape.match(text))
            and start == match.start("n")
        ):
            return shape.numbers["n"]
    return None


def _payment(raw: str, part: SourcePart, start: int) -> tuple[str, str | None] | None:
    text = part.canonical_source
    shape = SHAPES["payment"]
    if match := shape.match(text):
        positions = [
            m.start() + match.start("costs") for m in re.finditer(r"N", match["costs"])
        ]
        values = [
            int(
                normalize(
                    "NFKC", "".join(raw[s.start : s.end] for s in part.units[i].origins)
                )
            )
            for i in positions
        ]
        if values == list(range(1, 11)) and _value(raw, part, match, "n") == 1:
            if start in positions:
                return shape.numbers["costs"]
            if start == match.start("n"):
                return shape.numbers["n"]
    return None


def _chains(raw: str, part: SourcePart, start: int) -> tuple[str, str | None] | None:
    text = part.canonical_source
    if SHAPES["spell_chain"].match(text):
        thresholds = list(re.finditer(r"【(?:SC|スペルチェイン)_(?P<n>N)】", text))
        values = [_value(raw, part, t, "n") for t in thresholds]
        if values == sorted(set(values)) and text[max(0, start - 5) : start].endswith(
            "代わりに-"
        ):
            return SHAPES["spell_chain"].numbers["replacement"]
    if SHAPES["damage"].match(text) and text[start - 1 : start] == "-":
        return SHAPES["damage"].numbers["n"]
    chain = SHAPES["cost_kinds"].match(text)
    if chain and len(tuple(HEAD.finditer(text))) == 1:
        thresholds = list(re.finditer(r"(?P<n>N)種類以上なら、", text))
        values = [_value(raw, part, t, "n") for t in thresholds]
        if (
            len(values) == _CHAIN_THRESHOLDS
            and values == sorted(set(values))
            and any(start == t.start("n") for t in thresholds)
        ):
            return SHAPES["cost_kinds"].numbers["thresholds"]
    return None


def _reminder(
    raw: str, part: SourcePart, hint: Hint, field: SourceField | None
) -> tuple[str, str | None] | None:
    body = _anchor(part, field)
    if body is None:
        return None
    text = part.canonical_source
    start = hint.occurrence.start
    if (
        SHAPES["start_amulet"].match(text)
        and "【スタートアミュレット】" in body.canonical_source
    ):
        return SHAPES["start_amulet"].numbers["n"]
    lesson = SHAPES["lesson"].match(text)
    if (
        lesson
        and _value(raw, part, lesson, "parameter")
        == _value(raw, part, lesson, "count")
        == 1
        and _lesson_anchor(raw, body)
    ):
        if start == lesson.start("parameter"):
            return SHAPES["lesson"].numbers["parameter"]
        if start == lesson.start("count"):
            return SHAPES["lesson"].numbers["count"]
    nc = SHAPES["necrocharge"].match(text)
    if nc:
        numbers = list(re.finditer(r"(?P<n>" + DIGIT + ")", text))
        values = [_value(raw, part, n, "n") for n in numbers]
        anchors = list(
            re.finditer(r"【(?:ネクロチャージ|NC)_(?P<n>N)】", body.canonical_source)
        )
        body_values = [_value(raw, body, n, "n") for n in anchors]
        if (
            len(values) == _NC_OCCURRENCES
            and values[0] == values[1] < values[2] == values[3]
            and body_values == [values[0], values[2]]
        ):
            for index, number in enumerate(numbers):
                if start == number.start("n"):
                    return (
                        SHAPES["necrocharge"].numbers["count"]
                        if index % 2 == 0
                        else SHAPES["necrocharge"].numbers["parameter"]
                    )
    return None


def _lesson_anchor(raw: str, body: SourcePart) -> bool:
    matches = tuple(re.finditer(r"【レッスン_(?P<n>N)】", body.canonical_source))
    return len(matches) == 1 and _value(raw, body, matches[0], "n") == 1

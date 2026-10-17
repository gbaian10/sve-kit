"""Shared finite numeric shapes and pool operation anchors for #749."""

import re
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import CardNameReference, Span
from sve_carddb.domains.translations.four_layer_n3_shapes import ABILITY_REPLAY, DIGIT

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_n3_sources import Builder
    from sve_carddb.domains.translations.four_layer_normalizer import SourceField

POOL_ANY = "好きな枚数を場に出す。好きな枚数をEXエリアに置く。"
POOL_FIRST = re.compile(r"好きな枚数を場に出す|N枚を場に出す|場が上限になるまで出す")
POOL_SECOND = re.compile(
    r"好きな枚数をEXエリアに置く|N枚をEXエリアに置く|残りをEXエリアに置く"
)
_NUMBER = "(?P<n>" + DIGIT + ")"
_SEARCH_LIMIT = 10
_GENERATED_CAPACITY_POOL = 3
_SINGLE_POOL = 9
_COUNTER_LIMIT = 2


def _pool(builder: Builder) -> str | None:
    text = builder.text
    expressions = {
        getattr(v, "expression_id", "") for v in builder.found.values.values()
    }
    if (
        POOL_ANY in text
        and "その中から、" in text
        and {"name.selection.distinct.v1", "name.contains.any10.v1"} <= expressions
    ):
        supply = re.search(_NUMBER + r"枚まで探す。", text)
        if (
            supply
            and builder.number(Span(start=supply.start("n"), end=supply.end("n")))
            == _SEARCH_LIMIT
        ):
            builder.numeric(supply, "n", "selection_count", "up_to")
            return "P02"
    if (
        POOL_ANY in text
        and (supply := re.search(r"『X』" + _NUMBER + r"枚の中から", text))
        and builder.number(Span(start=supply.start("n"), end=supply.end("n")))
        == _SINGLE_POOL
    ):
        builder.numeric(supply, "n", "count")
        return "P01"
    if "『X』N枚と『X』N枚の中から" in text and re.search(
        r"N枚を場に出す。N枚をEXエリアに置く。$", text
    ):
        matches = tuple(re.finditer(_NUMBER + r"枚", text))
        if all(
            builder.number(Span(start=m.start("n"), end=m.end("n"))) == 1
            for m in matches
        ):
            for m in matches:
                builder.numeric(m, "n", "count")
            return "P01"
    supply = re.search(
        r"『X』" + _NUMBER + r"枚を、場が上限になるまで出す。残りをEXエリアに置く。$",
        text,
    )
    if (
        supply
        and builder.number(Span(start=supply.start("n"), end=supply.end("n")))
        == _GENERATED_CAPACITY_POOL
    ):
        builder.numeric(supply, "n", "count")
        return "P03"
    return None


def _discard_limit(builder: Builder) -> bool:
    text = builder.text
    discard = re.search(r"手札[^。{}【】]*捨てる", text)
    definition = re.search(r"Xは「[^「」]+」である。", text)
    limit = re.search(r"自分のデッキが" + _NUMBER + r"枚以下", text)
    if discard is None or definition is None or limit is None:
        return False
    if (
        discard.end() <= definition.start() < definition.end() <= limit.start()
        and builder.scope(discard.start())
        == builder.scope(definition.start())
        == builder.scope(limit.start())
        and builder.number(Span(start=limit.start("n"), end=limit.end("n"))) == 1
    ):
        builder.numeric(limit, "n", "existence_count", "up_to")
        return True
    return False


def _body(builder: Builder) -> str | None:
    text = builder.text
    amount = re.search(
        r"自分の場に『X』がいるなら、これに[^。{}【】「」]+カウンター"
        + _NUMBER
        + r"個まで置く。$",
        text,
    )
    if (
        amount
        and builder.number(Span(start=amount.start("n"), end=amount.end("n")))
        == _COUNTER_LIMIT
    ):
        builder.numeric(amount, "n", "counter_amount", "up_to")
        return "H02"
    if _discard_limit(builder):
        return "R01"
    amount = ABILITY_REPLAY.search(text)
    if (
        amount
        and builder.number(Span(start=amount.start("n"), end=amount.end("n"))) == 1
    ):
        builder.numeric(amount, "n", "count")
        return "A01"
    return None


def _stack(builder: Builder, field: SourceField) -> str | None:
    text = builder.text
    if "スタックカウンター" not in text:
        return None
    complete = "".join(p.canonical_source for p in field.parts)
    if not all(
        word in complete for word in ("場を離れる", "代わりに", "スタックカウンター")
    ):
        return None
    matches = tuple(
        re.finditer(
            r"スタックカウンター(?P<state>が)?"
            + _NUMBER
            + r"(?(state)(?:個)?(?=なら|に|の)|(?:個|つ)(?=を))",
            text,
        )
    )
    if not matches or any(
        builder.number(Span(start=m.start("n"), end=m.end("n")))
        != (0 if m["state"] else 1)
        for m in matches
    ):
        return None
    for match in matches:
        builder.numeric(match, "n", "threshold" if match["state"] else "counter_amount")
    return "H03"


def _increment(builder: Builder, field: SourceField) -> str | None:
    index = builder.original.source_span.anchor
    if index is None:
        return None
    body = field.parts[index]
    if body.line_ordinal != builder.original.line_ordinal or not re.search(
        r"【スタック】を\+N", body.canonical_source
    ):
        return None
    text = builder.text
    if not all(word in text for word in ("【スタック】を持つ", "【スタック】を+する")):
        return None
    if not any(
        isinstance(v, CardNameReference)
        and v.term_id == "term:name.earth_magic_fragment"
        for v in builder.found.values.values()
    ):
        return None
    supply = re.search(r"『[^』]+』" + _NUMBER + r"つを場に出し", text)
    if (
        supply
        and builder.number(Span(start=supply.start("n"), end=supply.end("n"))) == 1
    ):
        builder.numeric(supply, "n", "count")
        return "H04"
    return None


def plain_numbers(builder: Builder, field: SourceField) -> str | None:
    """Only complete source shapes discharge lexical numeric vetoes."""
    role = builder.original.source_span.role
    if role == "body":
        return _pool(builder) or _body(builder) or _stack(builder, field)
    if role == "reminder":
        return _stack(builder, field) or _increment(builder, field)
    return None

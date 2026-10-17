"""Finite Fusion declarations and context choice counts, without general arithmetic."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    CapturedValueRef,
    ChoiceContext,
    ContextExpression,
    Expression,
    QuantitySpec,
    ReceiptRef,
    Span,
)
from sve_carddb.domains.translations.four_layer_ability import ABILITY_HEAD
from sve_carddb.domains.translations.four_layer_event_preparation import origins
from sve_carddb.domains.translations.four_layer_n2_read_sources import ReadSources
from sve_carddb.domains.translations.four_layer_n3_context import CHOICE_ZONE_EXPRESSION
from sve_carddb.domains.translations.four_layer_n3_shapes import (
    CHOICE_DOMAIN,
    CHOSEN_X_EXPRESSION,
    FUSION_MAX,
)
from sve_carddb.domains.translations.recognition.provenance import merged

_MODE_COUNT = 3

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_n3_sources import Builder


def _fusion(builder: Builder) -> bool:
    match = re.search(
        r"【(?P<fusion>融合)】元のコスト(?P<cost>N)以上の(?P<trait>[^。{}【】「」:]+)・(?P<card>カード)(?P<material>X)枚:(?P<holder>これ)に融合(?P<counter_word>カウンター)(?P<counter>X)個を置く。(?P<definition>X)は(?P<upper>N)(?P<compare>以下)。$",
        builder.text,
    )
    if (
        match is None
        or builder.number(Span(start=match.start("cost"), end=match.end("cost"))) != 1
        or builder.number(Span(start=match.start("upper"), end=match.end("upper")))
        != FUSION_MAX
    ):
        return False
    if builder.scope(match.start()) != builder.scope(match.end() - 1):
        return False
    for group, category, key, role in (
        ("fusion", "ability", "term:ability.fusion", "ability"),
        ("trait", "trait", None, "trait"),
        ("card", "rule_term", "term:object.card", "rule_term"),
        ("counter_word", "rule_term", "term:object.counter", "rule_term"),
    ):
        if (
            builder.term(
                Span(start=match.start(group), end=match.end(group)),
                category,
                key,
                role,
            )
            is None
        ):
            builder.additions.clear()
            return False
    provider = Span(start=match.start(), end=match.start("holder"))
    port = builder.port(
        "declared_x",
        CapturedValueRef(
            kind="CapturedValue",
            binding_id=builder.identity("fusion-x", provider),
            result_type="Nat",
            measure="declared_x",
        ),
        "play_parameter",
        provider,
        match.start(),
    )
    value = builder.expression(
        CHOSEN_X_EXPRESSION,
        (),
        tuple(
            Span(start=match.start(g), end=match.end(g))
            for g in ("material", "counter", "definition")
        ),
        {"value": port},
        builder.scope(match.start()),
    )
    builder.additions[-1] = replace(builder.additions[-1], abstract=False)
    threshold = builder.numeric(match, "upper", "comparison_threshold")
    builder.expression(
        "condition.quantity.at_most.v1",
        (value, threshold),
        (Span(start=match.start("compare"), end=match.end("compare")),),
        {},
        builder.scope(match.start()),
    )
    builder.port(
        "payment_complete",
        ReceiptRef(
            kind="ReceiptId",
            result="payment_complete",
            receipt_id=builder.identity("fusion-payment", provider),
        ),
        "receipt",
        provider,
        match.start(),
    )
    return True


def _context_choice(builder: Builder) -> bool:
    intro = re.search(r"下記から(?P<x>X)個までチョイスする。", builder.text)
    if intro is None or re.search(ABILITY_HEAD, builder.text[intro.end() :]):
        return False
    labels = tuple(re.finditer(r"【(?P<n>N)】", builder.text[intro.end() :]))
    positions = tuple(
        Span(start=intro.end() + m.start("n"), end=intro.end() + m.end("n"))
        for m in labels
    )
    if len(positions) != _MODE_COUNT or [builder.number(p) for p in positions] != [
        1,
        2,
        3,
    ]:
        return False
    use = Span(start=intro.start("x"), end=intro.end("x"))
    scope = builder.scope(intro.start())
    expr = _choice_provider(builder, use, scope)
    if expr is None:
        return False
    if isinstance(expr, ContextExpression):
        scope = expr.scope
    quantity = QuantitySpec(mode="up_to", expr=expr)
    builder.scalar("QuantitySpec", "choice_mode_count", CHOICE_DOMAIN, quantity, (use,))
    if isinstance(expr, ContextExpression):
        builder.additions[-1] = replace(
            builder.additions[-1],
            slot=builder.additions[-1].slot.model_copy(update={"expression": expr}),
        )
    for span, value in zip(positions, (1, 2, 3), strict=True):
        builder.scalar("Ordinal", "choice_index", "", value, (span,))
    port = builder.port(
        "choice_context",
        ChoiceContext(
            kind="ChoiceContext",
            ability_instance=builder.identity("choice", use),
            turn="CurrentTurn",
            modes=("mode_1", "mode_2", "mode_3"),
        ),
        "play_parameter",
        use,
        use.start,
    )
    builder.ports[-1] = builder.ports[-1].model_copy(update={"scope": scope})
    return bool(port)


def prepare_parameters(builder: Builder) -> str | None:
    """Claim only the two sealed parameter declaration forms."""
    if builder.original.source_span.role != "body":
        return None
    if _fusion(builder):
        return "X01"
    if _context_choice(builder):
        return "X02"
    return None


def _choice_provider(
    builder: Builder, use: Span, scope: str
) -> Expression | ContextExpression | None:
    candidates = [
        (n, v)
        for n, v in builder.found.values.items()
        if isinstance(v, ContextExpression)
        and v.expression_id == CHOSEN_X_EXPRESSION
        and any(
            o.slot == n and o.raw_spans == origins(builder.original, use)
            for o in builder.found.occurrences
        )
    ]
    if len(candidates) == 1:
        old_name, expr = candidates[0]
        other = tuple(
            o
            for o in builder.found.occurrences
            if o.slot == old_name and o.raw_spans != origins(builder.original, use)
        )
        remaining = tuple(
            merged(
                tuple(
                    Span(start=i, end=i + 1)
                    for i, u in enumerate(builder.original.units)
                    if any(
                        a.start < b.end and b.start < a.end
                        for a in u.origins
                        for b in o.raw_spans
                    )
                )
            )
            for o in other
        )
        if not remaining or any(len(p) != 1 for p in remaining):
            return None
        builder.scalar(
            "QuantityExpr",
            "read_value",
            "quantity.read.context.v1",
            expr,
            tuple(p[0] for p in remaining),
        )
        builder.additions[-1] = replace(
            builder.additions[-1],
            abstract=False,
            slot=builder.additions[-1].slot.model_copy(update={"expression": expr}),
        )
        return expr
    if candidates:
        return None
    definition = re.search(
        r"Xは「(?P<operand>自分の場の[^「」。]+・カードの(?:枚数|数))」である。",
        builder.text,
    )
    if definition is None or builder.scope(definition.start()) != scope:
        return None
    reader = ReadSources(
        builder.raw,
        builder.source,
        builder.original,
        builder.original,
        builder.found,
        builder.classifier,
    )
    result = reader.read(
        definition["operand"],
        definition.start("operand"),
        (Span(start=definition.start(), end=definition.start() + 1),),
    )
    zone_expr = next(
        (item.value for item in reader.leaves if item.slot.name == result), None
    )
    if (
        not isinstance(zone_expr, Expression)
        or zone_expr.expression_id != CHOICE_ZONE_EXPRESSION
    ):
        return None
    builder.additions.extend(reader.leaves)
    return zone_expr

"""Source-scoped mode history and conditional cross-line choices for preparation only."""

import re
from dataclasses import dataclass, replace
from hashlib import sha256
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    CapturedValueRef,
    ChoiceContext,
    Constant,
    ContextExpression,
    Domain,
    Expression,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSlot,
    ModeCountRule,
    OccurrenceKey,
    PendingCondition,
    Port,
    PortEvidence,
    PredicateExpr,
    PrintingFaceOwner,
    Projection,
    QuantitySpec,
    ResolvedCondition,
    Scope,
    Span,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.domains.translations.four_layer_context import (
    Descriptor,
    ImportSignature,
    Parameter,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf
from sve_carddb.domains.translations.four_layer_n1_transform import transform
from sve_carddb.domains.translations.four_layer_normalizer import SourceField
from sve_carddb.domains.translations.four_layer_preparation import PreparationFamily

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import ConditionSlot, LeafType
    from sve_carddb.contracts.source_binding import TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.four_layer_preparation import PreparedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

_DIGIT = r"[0-9０-９]+"
_INTRO = re.compile(r"下記から(?P<n>" + _DIGIT + r")つ(?P<limit>まで)?チョイスする。")
_LABEL = re.compile(r"【(" + _DIGIT + r")】")
_ALTERNATIVE = re.compile(r"代わりに(?P<n>" + _DIGIT + r")つ(?P<limit>まで)?。$")
_HISTORY = "このターン、この能力でチョイスした選択肢はチョイスできない。"
_HEAD = re.compile(
    r"\{(?:ファンファーレ|起動|ラストワード)\}|【(?:進化時|攻撃時|超進化時)】"
)
_COST = re.compile(r"これをプレイする際、コストを\+(?P<n>" + _DIGIT + r")してよい。$")
_ROW = "N2-A6-A7.mode_context"
_MIN_MODES = 2


def _descriptor(name: str, parameters: tuple[Parameter, ...] = ()) -> Descriptor:
    return Descriptor(
        "choice.condition." + name + ".v1",
        "predicate.choice.condition.v1",
        "choice_condition",
        "Bool",
        "truth",
        parameters,
        (),
        "A7",
        ("X.predicate", "K.mode"),
    )


_OWNER = Parameter("owner", "Player", "counted_owner", "player.relative.v1")
_LIFE_OWNER = Parameter("owner", "Player", "mode_condition_owner", "player.relative.v1")
_ZONE = Parameter("zone", "ZoneSet", "counted_zone", "zone.single.v1")
_TRAIT = Parameter("trait", "Concept", "trait", "concept.trait.v1")
_KIND = Parameter("kind", "CardKind", "counted_kind", "card_kind.selection.v1")
_CARD = Parameter("card", "Concept", "rule_term", "concept.rule_term.v1")
_BOUND = Parameter("threshold", "Nat", "threshold", "")
_COUNT_BOUND = Parameter(
    "threshold",
    "QuantitySpec",
    "existence_count",
    "quantity.existence_count.constant.v1",
)
DESCRIPTORS = (
    Descriptor(
        "mode.unused_this_turn.v1",
        "predicate.mode.unused_this_turn.v1",
        "mode_reuse_condition",
        "Bool",
        "truth",
        (),
        (ImportSignature("choice", "ChoiceContext", "play_parameter"),),
        "A6",
        ("H.mode-use", "ModeDomain.exclude_used"),
    ),
    Descriptor(
        "choice.optional_cost.paid.v1",
        "predicate.choice.optional_cost.v1",
        "optional_cost_condition",
        "Bool",
        "truth",
        (),
        (
            ImportSignature(
                "payment",
                "CapturedValue",
                "play_parameter",
                result_type="Bool",
                measure="payment_complete",
            ),
        ),
        "A7",
        ("V.binding", "D.play-param", "R-0006"),
    ),
    _descriptor("zone.at_least", (_OWNER, _ZONE, _COUNT_BOUND)),
    _descriptor("trait_card.at_least", (_OWNER, _ZONE, _TRAIT, _CARD, _COUNT_BOUND)),
    _descriptor(
        "trait_follower.at_least", (_OWNER, _ZONE, _TRAIT, _KIND, _COUNT_BOUND)
    ),
    _descriptor(
        "quantity.at_least",
        (
            Parameter(
                "value",
                "QuantityExpr",
                "read_value",
                "",
                alternate_roles=("count_expression", "history_read_value"),
            ),
            _COUNT_BOUND,
        ),
    ),
    _descriptor("life.at_most", (_LIFE_OWNER, _BOUND)),
    *(
        _descriptor("keyword." + name, (_BOUND,))
        for name in ("necrocharge", "spell_chain")
    ),
)


@dataclass(frozen=True)
class ModeSource:
    """One intro and its contiguous options; every offset is in the original field."""

    intro: Span
    count: Span
    base: QuantitySpec
    modes: tuple[Span, ...]
    members: tuple[int, ...]
    condition: Span | None
    alternative: QuantitySpec | None
    alternative_count: Span | None
    restriction: Span | None
    cost: Span | None


def _mask(raw: str) -> str:
    chars = list(raw)
    stack: list[str] = []
    pairs = {"『": "』", "「": "」", "（": "）", "(": ")"}
    for i, char in enumerate(raw):
        if char in pairs:
            stack.append(pairs[char])
        if stack:
            chars[i] = " " if char != "\n" else "\n"
            if char == stack[-1]:
                stack.pop()
    return "".join(chars)


def _quantity(match: re.Match[str]) -> QuantitySpec:
    return QuantitySpec(
        mode="up_to" if match["limit"] else "exact",
        expr=Constant(kind="constant", value=int(match["n"])),
    )


def _ability_bounds(masked: str, position: int) -> tuple[int, int]:
    boundaries = tuple(re.finditer(r"\n\s*\n", masked)) + tuple(_HEAD.finditer(masked))
    start = max((m.end() for m in boundaries if m.end() <= position), default=0)
    end = min(
        (m.start() for m in boundaries if m.start() > position), default=len(masked)
    )
    return start, end


def _cost_candidates(raw: str, position: int, condition: Span) -> tuple[Span, ...]:
    amount = re.search(_DIGIT, raw[condition.start : condition.end])
    if amount is None:
        return ()
    masked = _mask(raw)
    start, _ = _ability_bounds(masked, position)
    return tuple(
        Span(start=line.start(), end=line.end())
        for line in re.compile(r"[^\n]+").finditer(masked, start, position)
        if (payment := _COST.fullmatch(line[0])) is not None
        and int(payment["n"]) == int(amount[0])
    )


def _optional_cost(
    raw: str, condition: Span | None, lines: tuple[re.Match[str], ...], index: int
) -> Span | None:
    if (
        condition is None
        or not index
        or not re.fullmatch(
            r"コストを\+" + _DIGIT + r"したなら、", raw[condition.start : condition.end]
        )
    ):
        return None
    candidates = _cost_candidates(raw, condition.start, condition)
    previous = lines[index - 1]
    if (
        len(candidates) == 1
        and candidates[0] == Span(start=previous.start(), end=previous.end())
        and previous.end() + 1 == lines[index].start()
    ):
        return candidates[0]
    return None


def _labels_valid(
    masked: str,
    line: re.Match[str],
    intro: re.Match[str],
    labels: tuple[re.Match[str], ...],
    end: int,
) -> bool:
    return (
        len(labels) >= _MIN_MODES
        and [int(m[1]) for m in labels] == list(range(1, len(labels) + 1))
        and 1 <= int(intro["n"]) <= len(labels)
        and not (
            _INTRO.search(masked, intro.end(), end)
            or _HEAD.search(masked, intro.end(), end)
        )
        and not _LABEL.search(masked, line.start(), intro.start())
    )


def _unique_intro(masked: str, intro: re.Match[str]) -> bool:
    start, end = _ability_bounds(masked, intro.start())
    return len(tuple(_INTRO.finditer(masked, start, end))) == 1


def _mode_source(
    raw: str,
    masked: str,
    prepared: PreparedField,
    lines: tuple[re.Match[str], ...],
    index: int,
) -> ModeSource | None:
    line = lines[index]
    intros = tuple(_INTRO.finditer(masked, line.start(), line.end()))
    if len(intros) != 1:
        return None
    intro = intros[0]
    if not _unique_intro(masked, intro):
        return None
    end = line.end()
    cursor = index + 1
    while cursor < len(lines) and lines[cursor].start() == end + 1:
        if _LABEL.match(masked, lines[cursor].start()) is None:
            break
        end = lines[cursor].end()
        cursor += 1
    labels = tuple(_LABEL.finditer(masked, intro.end(), end))
    if not _labels_valid(masked, line, intro, labels, end):
        return None
    prefix = raw[intro.end() : labels[0].start()].rstrip()
    alternative = _ALTERNATIVE.search(prefix)
    history = masked[intro.end() : labels[0].start()].find(_HISTORY)
    if not ((labels[0].start() > line.end() and alternative) or history >= 0):
        return None
    if alternative and not 1 <= int(alternative["n"]) <= len(labels):
        return None
    condition = (
        Span(start=intro.end(), end=intro.end() + alternative.start())
        if alternative
        else None
    )
    cost = _optional_cost(raw, condition, lines, index)
    return ModeSource(
        Span(start=intro.start(), end=intro.end()),
        Span(start=intro.start("n"), end=intro.end("n")),
        _quantity(intro),
        tuple(Span(start=m.start(1), end=m.end(1)) for m in labels),
        tuple(
            i
            for i, part in enumerate(prepared.derived.field.parts)
            if part.source_span.role == "body"
            and any(
                (cost.start if cost else line.start()) <= s.start < s.end <= end
                for s in part.source_span.segments
            )
        ),
        condition,
        _quantity(alternative) if alternative else None,
        Span(
            start=intro.end() + alternative.start("n"),
            end=intro.end() + alternative.end("n"),
        )
        if alternative
        else None,
        Span(start=intro.end() + history, end=intro.end() + history + len(_HISTORY))
        if history >= 0
        else None,
        cost,
    )


def mode_sources(raw: str, prepared: PreparedField) -> tuple[ModeSource, ...]:
    """Reject orphan, ambiguous, nested and foreign-ability choices before assigning edges."""
    source = prepared.derived.field.source
    if source.field not in {"effect", "section"} or not isinstance(
        source.owner, (FaceRevisionOwner, PrintingFaceOwner)
    ):
        return ()
    masked = _mask(raw)
    lines = tuple(re.finditer(r"[^\n]+", masked))
    return tuple(
        group
        for i in range(len(lines))
        if (group := _mode_source(raw, masked, prepared, lines, i)) is not None
    )


def _positions(part: SourcePart, span: Span) -> tuple[tuple[Span, ...], ...]:
    positions = [
        i
        for i, unit in enumerate(part.units)
        if any(s.start < span.end and span.start < s.end for s in unit.origins)
    ]
    if not positions:
        return ()
    return ((Span(start=positions[0], end=positions[-1] + 1),),)


def _operand(
    part: SourcePart,
    name: str,
    parameter: Parameter,
    value: TypedValue,
    span: Span,
) -> Leaf:
    positions = _positions(part, span)
    return Leaf(
        LeafSlot(
            name=name,
            type=parameter.type,
            role=parameter.role,
            domain=Domain(values=(parameter.domain,), min=None, max=None)
            if parameter.domain
            else Domain(
                values=(),
                min=1 if parameter.type == "Ordinal" else 0,
                max=9007199254740991,
            ),
            required=True,
            occurrences=tuple(s for group in positions for s in group),
        ),
        value,
        positions,
        _ROW,
        abstract=parameter.type not in {"Nat", "Ordinal", "QuantitySpec"},
    )


def _predicate(part: SourcePart, span: Span, expr: PredicateExpr, name: str) -> Leaf:
    descriptor = next(d for d in DESCRIPTORS if d.id == expr.expression_id)
    item = _operand(
        part,
        name,
        Parameter(name, "PredicateExpr", descriptor.role, descriptor.domain),
        expr,
        span,
    )
    return replace(
        item, slot=item.slot.model_copy(update={"expression": expr}), abstract=False
    )


def _retained(part: SourcePart, found: Recognized) -> list[Leaf]:
    return [
        Leaf(
            slot,
            found.values[slot.name],
            tuple(
                o.canonical_spans
                for o in found.occurrences
                if o.slot == slot.name and o.canonical_spans
            ),
            "N1",
            any(
                u.transformation == "leaf"
                for sp in slot.occurrences
                for u in part.units[sp.start : sp.end]
            ),
            next(o.source_unit for o in found.occurrences if o.slot == slot.name),
            next(o.resolution_rule for o in found.occurrences if o.slot == slot.name),
            part.source_span.segments[0].start,
        )
        for slot in found.schema.slots
    ]


def _pending(raw: str, span: Span, descriptor: str, reason: str) -> PendingCondition:
    return PendingCondition.model_validate(
        {
            "state": "pending",
            "source_descriptor": descriptor,
            "source_span": span,
            "raw_sha256": sha256(raw[span.start : span.end].encode()).hexdigest(),
            "reason": reason,
        }
    )


def _condition(
    raw: str,
    part: SourcePart,
    found: Recognized,
    group: ModeSource,
    classifier: Classifier,
    scope: str,
) -> tuple[ConditionSlot, list[Leaf]]:
    assert group.condition is not None
    span = group.condition
    text = raw[span.start : span.end]
    # Existing providers must prove this exact condition occurrence, never merely equal values.
    providers = [
        slot.name
        for slot in found.schema.slots
        if isinstance((value := found.values[slot.name]), PredicateExpr)
        and value.scope == scope
        and any(
            o.slot == slot.name
            and o.raw_spans
            and all(span.start <= s.start < s.end <= span.end for s in o.raw_spans)
            for o in found.occurrences
        )
    ]
    if len(providers) == 1:
        return ResolvedCondition(state="resolved", predicate=providers[0]), []
    if providers:
        return _pending(
            raw, span, "choice.condition.provider.v1", "ambiguous_source"
        ), []
    if not _positions(part, span):
        return _pending(
            raw, span, "choice.condition.source_edge.v1", "missing_provider"
        ), []
    if group.cost is not None:
        item = _predicate(
            part,
            Span(start=span.end - 3, end=span.end),
            PredicateExpr(
                kind="predicate",
                expression_id="choice.optional_cost.paid.v1",
                leaves=(),
                imports={"payment": "mode_payment"},
                scope=scope,
            ),
            "mode_condition",
        )
        return ResolvedCondition(state="resolved", predicate="mode_condition"), [item]
    finite = _read_condition(raw, part, found, span, scope) or _finite_condition(
        raw, part, span, classifier, scope
    )
    if finite is not None:
        return finite
    descriptor, reason = "choice.condition.unsealed.v1", "unsealed_source"
    if "場に出ていたなら" in text:
        descriptor, reason = "predicate.entry.provenance.v1", "missing_provider"
    elif text.startswith("このターン、"):
        descriptor, reason = "history.evolve.exists.v1", "missing_provider"
    elif "エボルヴデッキの表向き" in text:
        descriptor, reason = "read.faceup_evolve.count.v1", "missing_provider"
    elif text.startswith("コストを+"):
        descriptor = "choice.condition.optional_cost.v1"
        reason = (
            "ambiguous_source"
            if len(_cost_candidates(raw, group.intro.start, span)) > 1
            else "missing_provider"
        )
    return _pending(raw, span, descriptor, reason), []


def _read_condition(
    raw: str, part: SourcePart, found: Recognized, span: Span, scope: str
) -> tuple[ConditionSlot, list[Leaf]] | None:
    providers = [
        name
        for name, value in found.values.items()
        if isinstance(value, (ContextExpression, Expression))
        and any(
            o.slot == name
            and o.raw_spans
            and all(span.start <= s.start < s.end <= span.end for s in o.raw_spans)
            for o in found.occurrences
        )
    ]
    match = re.search(
        r"(?P<n>" + _DIGIT + r")枚以上なら、$", raw[span.start : span.end]
    )
    if len(providers) != 1 or match is None:
        return None
    bound = _operand(
        part,
        "mode_threshold",
        _COUNT_BOUND,
        QuantitySpec(
            mode="at_least", expr=Constant(kind="constant", value=int(match["n"]))
        ),
        Span(start=span.start + match.start("n"), end=span.start + match.end("n")),
    )
    predicate = _predicate(
        part,
        Span(start=span.end - 3, end=span.end),
        PredicateExpr(
            kind="predicate",
            expression_id="choice.condition.quantity.at_least.v1",
            leaves=(providers[0], "mode_threshold"),
            imports={},
            scope=scope,
        ),
        "mode_condition",
    )
    return ResolvedCondition(state="resolved", predicate="mode_condition"), [
        bound,
        predicate,
    ]


def _finite_condition(
    raw: str, part: SourcePart, span: Span, classifier: Classifier, scope: str
) -> tuple[ConditionSlot, list[Leaf]] | None:
    text = raw[span.start : span.end]
    match = re.fullmatch(
        r"(?P<owner>自分|相手)の(?P<zone>場|墓場)(?:の(?P<trait>[^。『』・]+)・(?P<kind>カード|フォロワー))?が(?P<n>"
        + _DIGIT
        + r")(?:枚|体)以上なら、",
        text,
    )
    life = re.fullmatch(
        r"(?P<owner>自分|相手)のリーダーの体力が(?P<n>" + _DIGIT + r")以下なら、", text
    )
    keyword = re.fullmatch(
        r"【(?P<keyword>ネクロチャージ|スペルチェイン)_(?P<n>" + _DIGIT + r")】", text
    )
    selected = match or life or keyword
    additions: list[Leaf] = []
    name = ""
    if selected:

        def argument(
            formal: str,
            type_: LeafType,
            role: str,
            domain: str,
            value: TypedValue,
            capture: str,
        ) -> None:
            additions.append(
                _operand(
                    part,
                    "mode_" + formal,
                    Parameter(formal, type_, role, domain),
                    value,
                    Span(
                        start=span.start + selected.start(capture),
                        end=span.start + selected.end(capture),
                    ),
                )
            )

        if match or life:
            argument(
                "owner",
                "Player",
                "mode_condition_owner" if life else "counted_owner",
                "player.relative.v1",
                "self" if selected["owner"] == "自分" else "opponent",
                "owner",
            )
        if match:
            argument(
                "zone",
                "ZoneSet",
                "counted_zone",
                "zone.single.v1",
                ("graveyard" if match["zone"] == "墓場" else "battlefield",),
                "zone",
            )
            name = "zone.at_least"
            if match["trait"]:
                terms = [
                    t
                    for t in classifier.terms.values()
                    if t.category == "trait" and t.source_ja == match["trait"]
                ]
                if len(terms) != 1:
                    return _pending(
                        raw, span, "choice.condition.trait.v1", "missing_provider"
                    ), []
                argument(
                    "trait",
                    "Concept",
                    "trait",
                    "concept.trait.v1",
                    GlossaryReference(kind="glossary", key=terms[0].id),
                    "trait",
                )
                if match["kind"] == "カード":
                    argument(
                        "card",
                        "Concept",
                        "rule_term",
                        "concept.rule_term.v1",
                        GlossaryReference(kind="glossary", key="term:object.card"),
                        "kind",
                    )
                    name = "trait_card.at_least"
                else:
                    argument(
                        "kind",
                        "CardKind",
                        "counted_kind",
                        "card_kind.selection.v1",
                        VocabularyReference(
                            kind="vocabulary", key=("type", "follower")
                        ),
                        "kind",
                    )
                    name = "trait_follower.at_least"
        elif life:
            name = "life.at_most"
        elif keyword:
            name = "keyword." + (
                "necrocharge"
                if keyword["keyword"] == "ネクロチャージ"
                else "spell_chain"
            )
        argument(
            "threshold",
            "QuantitySpec" if match else "Nat",
            "existence_count" if match else "threshold",
            "quantity.existence_count.constant.v1" if match else "",
            QuantitySpec(
                mode="at_least",
                expr=Constant(kind="constant", value=int(selected["n"])),
            )
            if match
            else int(selected["n"]),
            "n",
        )
        expr = PredicateExpr(
            kind="predicate",
            expression_id="choice.condition." + name + ".v1",
            leaves=tuple(a.slot.name for a in additions),
            imports={},
            scope=scope,
        )
        anchor = Span(start=span.end - (3 if match or life else 1), end=span.end)
        additions.append(_predicate(part, anchor, expr, "mode_condition"))
        return ResolvedCondition(
            state="resolved", predicate="mode_condition"
        ), additions
    return None


def _numbers(
    raw: str, part: SourcePart, found: Recognized, group: ModeSource
) -> list[Leaf]:
    result = []
    if group.cost:
        for source_span in (group.cost, group.condition):
            if source_span is None:
                continue
            number = re.search(_DIGIT, raw[source_span.start : source_span.end])
            if number is None:
                continue
            cost_span = Span(
                start=source_span.start + number.start(),
                end=source_span.start + number.end(),
            )
            if _positions(part, cost_span) and not any(
                o.raw_spans == (cost_span,) for o in found.occurrences
            ):
                result.append(
                    _operand(
                        part,
                        "mode_cost",
                        Parameter("amount", "Nat", "optional_cost_amount", ""),
                        int(number[0]),
                        cost_span,
                    )
                )
    for name, value, span in (
        ("base", group.base, group.count),
        ("alternative", group.alternative, group.alternative_count),
    ):
        if value is not None and span is not None and _positions(part, span):
            result.append(
                replace(
                    _operand(
                        part,
                        "mode_" + name,
                        Parameter(
                            "count",
                            "QuantitySpec",
                            "choice_mode_count",
                            "quantity.choice_mode_count.constant.v1",
                        ),
                        value,
                        span,
                    ),
                    unit="つ",
                )
            )
    for i, span in enumerate(group.modes):
        if _positions(part, span):
            result.append(
                _operand(
                    part,
                    "mode_index_" + str(i),
                    Parameter("index", "Ordinal", "choice_index", ""),
                    i + 1,
                    span,
                )
            )
    return result


def _issues(
    raw: str,
    prepared: PreparedField,
    index: int,
    leaves: list[Leaf],
    classifier: Classifier,
) -> tuple[str, ...]:
    part = prepared.derived.field.parts[index]
    covered = tuple(
        s
        for a in leaves
        for positions in a.positions
        for sp in positions
        for u in part.units[sp.start : sp.end]
        for s in u.origins
    )
    hints = tuple(
        h
        for h in classifier.candidate(raw, prepared.derived.original.parts[index]).slots
        if h.type == "uint"
    )
    if hints and all(
        h.source_segments
        and all(
            any(a.start <= s.start < s.end <= a.end for a in covered)
            for s in h.source_segments
        )
        for h in hints
    ):
        return tuple(
            i
            for i in prepared.derived.recognized[index].issues
            if i
            not in {
                "numeric_role_requires_review",
                "n0_numeric_construction_unresolved",
                *(
                    {"signed_numeric_requires_review"}
                    if any(a.slot.role == "optional_cost_amount" for a in leaves)
                    else set()
                ),
            }
        )
    return prepared.derived.recognized[index].issues


def _updated(
    raw: str,
    prepared: PreparedField,
    index: int,
    additions: list[Leaf],
    classifier: Classifier,
) -> tuple[SourcePart, Recognized, tuple[str, ...], dict[str, str]]:
    part = prepared.derived.field.parts[index]
    found = prepared.derived.recognized[index]
    kept = _retained(part, found)
    # Predicate operands reuse existing occurrences, preserving their public leaf meanings.
    aliases = {}
    for item in additions:
        old = next(
            (
                a
                for a in kept
                if a.positions == item.positions
                and a.value == item.value
                and a.slot.type == item.slot.type
                and a.slot.role == item.slot.role
            ),
            None,
        )
        if old:
            aliases[item.slot.name] = old.slot.name
        else:
            kept.append(item)
    names_before = {id(a): a.slot.name for a in kept}
    updated, result, registry_rows = transform(
        raw,
        part,
        kept,
        (),
        replace(
            found,
            issues=_issues(raw, prepared, index, kept, classifier),
        ),
        separate_occurrences=True,
    )
    names = {names_before[id(a)]: "leaf_" + str(i) for i, a in enumerate(kept)}
    names.update({a: names[b] for a, b in aliases.items()})
    values = {
        name: value.model_copy(
            update={"leaves": tuple(names.get(n, n) for n in value.leaves)}
        )
        if isinstance(value, (PredicateExpr, ContextExpression, Expression))
        else value
        for name, value in result.values.items()
    }
    schema = result.schema.model_copy(
        update={
            "slots": tuple(
                slot.model_copy(update={"expression": values[slot.name]})
                if slot.expression is not None
                else slot
                for slot in result.schema.slots
            )
        }
    )
    return updated, replace(result, values=values, schema=schema), registry_rows, names


def _provider_scope(prepared: PreparedField, group: ModeSource) -> str | None:
    if group.condition is None:
        return None
    scopes = {
        value.scope
        for i in group.members
        for name, value in prepared.derived.recognized[i].values.items()
        if isinstance(value, (PredicateExpr, ContextExpression))
        and any(
            o.slot == name
            and o.raw_spans
            and all(
                group.condition.start <= s.start < s.end <= group.condition.end
                for s in o.raw_spans
            )
            for o in prepared.derived.recognized[i].occurrences
        )
    }
    return next(iter(scopes)) if len(scopes) == 1 else None


def _interface(
    prepared: PreparedField,
    group: ModeSource,
    index: int,
    scope: str,
    occurrence: OccurrenceKey,
    choice: ChoiceContext,
) -> Projection:
    found = prepared.derived.recognized[index]
    projection = (
        prepared.interfaces[index]
        or (found.semantics.projection if found.semantics is not None else None)
        or Projection(
            projection_kind="pending",
            discriminator=None,
            scopes=(),
            imports=(),
            exports=(),
        )
    )
    imports = [
        Port(
            name="mode_choice",
            type="ChoiceContext",
            source_role="choice_context",
            scope=scope,
            evidence=PortEvidence(
                source=occurrence, capture="play_parameter", value=choice
            ),
        )
    ]
    exports = []
    if group.cost is not None:
        part = next(
            p
            for p in prepared.derived.field.parts
            if any(
                s.start == group.cost.start and s.end == group.cost.end
                for s in p.source_span.segments
            )
        )
        cost_source = occurrence.model_copy(
            update={
                "line_ordinal": part.line_ordinal,
                "segments": part.source_span.segments,
            }
        )
        payment = Port(
            name="mode_payment",
            type="CapturedValue",
            source_role="optional_cost_payment",
            scope=scope,
            evidence=PortEvidence(
                source=cost_source,
                capture="play_parameter",
                value=CapturedValueRef(
                    kind="CapturedValue",
                    binding_id="parameter:"
                    + hash_payload(cost_source.model_dump(mode="json")),
                    result_type="Bool",
                    measure="payment_complete",
                ),
            ),
        )
        imports.append(payment)
        if index == prepared.derived.field.parts.index(part):
            exports.append(payment)
    return projection.model_copy(
        update={
            "scopes": (
                *projection.scopes,
                *(
                    (Scope(id=scope, parent=None, kind="ability"),)
                    if scope not in {s.id for s in projection.scopes}
                    else ()
                ),
            ),
            "imports": tuple(
                sorted((*projection.imports, *imports), key=lambda p: p.name)
            ),
            "exports": tuple(
                sorted((*projection.exports, *exports), key=lambda p: p.name)
            ),
        }
    )


def prepare_modes(
    raw: str,
    prepared: PreparedField,
    classifier: Classifier,
    context: CardContext | None,
) -> PreparedField:
    """Attach only proved source edges; runtime history and payment timing remain unresolved."""
    del context
    parts = list(prepared.derived.field.parts)
    recognized = list(prepared.derived.recognized)
    rows = list(prepared.derived.registry_rows)
    interfaces = list(prepared.interfaces)
    rules = list(prepared.mode_rules or (() for _ in parts))
    for group in mode_sources(raw, prepared):
        intro = next(
            parts[i] for i in group.members if _positions(parts[i], group.count)
        )
        occurrence = OccurrenceKey(
            **prepared.derived.field.source.model_dump(
                include={"owner", "field", "ordinal", "source_hash"}
            ),
            line_ordinal=intro.line_ordinal,
            role="body",
            segments=intro.source_span.segments,
        )
        scope = _provider_scope(prepared, group) or "mode_" + str(group.intro.start)
        choice = ChoiceContext(
            kind="ChoiceContext",
            ability_instance="ability:"
            + hash_payload(occurrence.model_dump(mode="json")),
            turn="CurrentTurn",
            modes=tuple("mode_" + str(i + 1) for i in range(len(group.modes))),
        )
        for index in group.members:
            part, found = parts[index], recognized[index]
            additions: list[Leaf] = []
            condition = None
            if group.condition:
                condition, additions = _condition(
                    raw, part, found, group, classifier, scope
                )
            if group.restriction and _positions(part, group.restriction):
                additions.append(
                    _predicate(
                        part,
                        group.restriction,
                        PredicateExpr(
                            kind="predicate",
                            expression_id="mode.unused_this_turn.v1",
                            leaves=(),
                            imports={"choice": "mode_choice"},
                            scope=scope,
                        ),
                        "mode_unused",
                    )
                )
            additions.extend(_numbers(raw, part, found, group))
            parts[index], recognized[index], rows[index], names = _updated(
                raw, prepared, index, additions, classifier
            )
            interfaces[index] = _interface(
                prepared, group, index, scope, occurrence, choice
            )
            if condition and group.alternative:
                if isinstance(condition, ResolvedCondition):
                    condition = condition.model_copy(
                        update={"predicate": names[condition.predicate]}
                    )
                rules[index] = (
                    *rules[index],
                    ModeCountRule(
                        base=group.base,
                        alternative=group.alternative,
                        condition=condition,
                        choice_context="mode_choice",
                        scope=scope,
                    ),
                )
    return replace(
        prepared,
        derived=replace(
            prepared.derived,
            field=SourceField(prepared.derived.field.source, tuple(parts)),
            recognized=tuple(recognized),
            registry_rows=tuple(rows),
        ),
        interfaces=tuple(interfaces),
        mode_rules=tuple(rules),
    )


FAMILY = PreparationFamily("A6_A7", DESCRIPTORS, prepare_modes)

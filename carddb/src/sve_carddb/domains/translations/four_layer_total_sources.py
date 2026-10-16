"""Source-bound A8 totals and the two sealed, disjoint A10 pair signatures."""

import re
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Constant,
    ContextExpression,
    Domain,
    Expression,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSlot,
    PredicateExpr,
    PrintingFaceOwner,
    Projection,
    QuantitySpec,
    Scope,
    Span,
    VocabularyReference,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_n1_transform import transform
from sve_carddb.domains.translations.four_layer_preparation import PreparationFamily
from sve_carddb.domains.translations.four_layer_totals import (
    DESCRIPTORS,
    NOUNS,
    count_id,
    sum_id,
)
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import LeafType
    from sve_carddb.contracts.source_binding import TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.four_layer_preparation import PreparedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

_PAIR_SIZE = 2
_ZONES = {
    "場": "battlefield",
    "墓場": "graveyard",
    "デッキ": "deck",
    "手札": "hand",
    "EXエリア": "ex",
}
_KINDS = {"フォロワー": "follower", "スペル": "spell", "アミュレット": "amulet"}
_SOURCE = re.compile(
    r"(?P<owner>自分|相手)の(?P<zone>場|墓場|デッキ|手札|EXエリア)(?:の|から)"
)
_TOTAL_SOURCE = re.compile(
    r"(?P<owner>自分|相手)の(?P<zone>場|墓場|デッキ|手札|EXエリア)(?:の|(?=と))"
)
_DEFINITION = re.compile(r"Xは「(?P<value>[^「」]+)」である。")
_PROPERTY = re.compile(
    r"(?P<property>元のコスト|攻撃力)の合計が(?P<value>N)(?P<compare>以上|以下|と同じ|)(?P<use>になるように|なら)"
)
_PAIR = re.compile(r"各(?P<cards>N)枚(?P<groups>N)組につき")


def _span(start: int, end: int) -> Span:
    return Span(start=start, end=end)


def _origins(part: SourcePart, span: Span) -> tuple[Span, ...]:
    return merged(
        tuple(s for u in part.units[span.start : span.end] for s in u.origins)
    )


def _raw(raw: str, part: SourcePart, span: Span) -> str:
    return "".join(raw[s.start : s.end] for s in _origins(part, span))


@dataclass
class _Builder:
    raw: str
    original: SourcePart
    classifier: Classifier
    scope: str
    leaves: list[Leaf]

    def add(
        self, name: str, type_: LeafType, domain: str, value: TypedValue, span: Span
    ) -> str:
        """Assign source identity before the shared transform renames slots."""
        name = f"aggregate_{len(self.leaves)}_{name}"
        role = "aggregate_" + name.split("_", 2)[2].removesuffix("_left").removesuffix(
            "_right"
        )
        item = (
            Leaf(
                LeafSlot(
                    name=name,
                    type="Nat",
                    role=role,
                    domain=Domain(values=(), min=0, max=SAFE_INTEGER),
                    required=True,
                    occurrences=(span,),
                ),
                value,
                ((span,),),
                "N2-A8-A10",
                False,
            )
            if type_ == "Nat"
            else leaf(type_, role, domain, value, ((span,),), "N2-A8-A10")
        )
        self.leaves.append(
            replace(
                item, slot=item.slot.model_copy(update={"name": name}), abstract=False
            )
        )
        return name

    def expression(self, id_: str, actuals: list[str], spans: tuple[Span, ...]) -> str:
        """Store the dependency DAG in both binding and source schema."""
        rule = next(d for d in DESCRIPTORS if d.id == id_)
        if id_.startswith("aggregate.count."):
            spans = ()
        predicate = rule.result_type == "Bool"
        expr = (
            PredicateExpr(
                kind="predicate",
                expression_id=id_,
                leaves=tuple(actuals),
                imports={},
                scope=self.scope,
            )
            if predicate
            else ContextExpression(
                kind="context_expression",
                expression_id=id_,
                leaves=tuple(actuals),
                imports={},
                scope=self.scope,
            )
        )
        name = f"aggregate_expr_{len(self.leaves)}"
        item = leaf(
            "PredicateExpr" if predicate else "QuantityExpr",
            rule.role,
            rule.domain,
            expr,
            tuple((s,) for s in spans),
            "N2-" + rule.source_family,
        )
        self.leaves.append(
            replace(
                item,
                slot=item.slot.model_copy(
                    update={"name": name, "expression": expr, "required": True}
                ),
                abstract=False,
                resolution_rule="aggregate.selector.count.v1" if not spans else None,
                use=self.original.source_span.segments[0].start,
            )
        )
        return name

    def reference(
        self, spelling: str, category: str, span: Span, name: str
    ) -> str | None:
        """Missing or ambiguous concepts remain unresolved source evidence."""
        terms = [
            t
            for t in self.classifier.terms.values()
            if t.source_ja == spelling and t.category == category
        ]
        if len(terms) != 1:
            return None
        value = (
            CardNameReference(kind="card_name", term_id=terms[0].id)
            if category == "card_name"
            else GlossaryReference(kind="glossary", key=terms[0].id)
        )
        return self.add(
            name,
            "CardName" if category == "card_name" else "Concept",
            "card_name.any.v1"
            if category == "card_name"
            else "concept." + category + ".v1",
            value,
            span,
        )

    def owner_zone(self, match: re.Match[str]) -> list[str]:
        """Share printed operands by position, never by equal value."""
        return [
            self.add(
                "owner",
                "Player",
                "player.relative.v1",
                "self" if match["owner"] == "自分" else "opponent",
                _span(match.start("owner"), match.end("owner")),
            ),
            self.add(
                "zone",
                "ZoneSet",
                "zone.single.v1",
                (_ZONES[match["zone"]],),
                _span(match.start("zone"), match.end("zone")),
            ),
        ]

    def noun(self, start: int, text: str) -> tuple[str, list[str]] | None:
        """Recognize the finite noun shapes used by aggregate sources."""
        if text == "『X』":
            span = _span(start + 1, start + 2)
            ref = self.reference(
                _raw(self.raw, self.original, span), "card_name", span, "name"
            )
            return ("name", [ref]) if ref else None
        alternative = text.split("や")
        if len(alternative) == _PAIR_SIZE:
            left = self.noun(start, alternative[0])
            right = self.noun(start + len(alternative[0]) + 1, alternative[1])
            if left and right and left[0] == right[0] == "trait_kind":
                return "alternatives", left[1] + right[1]
            return None
        qualifier = ""
        if text.startswith("「これと同名を除く") and text.endswith("」"):
            qualifier = "exclude_name"
            start += len("「これと同名を除く")
            text = text[len("「これと同名を除く") : -1]
        elif text.startswith("「それぞれカード名が異なる") and text.endswith("」"):
            qualifier = "distinct_names"
            start += len("「それぞれカード名が異なる")
            text = text[len("「それぞれカード名が異なる") : -1]
        return self._simple_noun(start, text, qualifier)

    def _simple_noun(
        self, start: int, text: str, qualifier: str
    ) -> tuple[str, list[str]] | None:
        actuals = []
        shape = []
        cost = re.match(r"元のコスト(?P<cost>N)(?P<bound>以下)?の", text)
        if cost:
            span = _span(start + cost.start("cost"), start + cost.end("cost"))
            actuals.append(
                self.add(
                    "cost", "Nat", "", int(_raw(self.raw, self.original, span)), span
                )
            )
            shape.append("class_cost_le" if cost["bound"] else "cost_eq_kind")
            start += cost.end()
            text = text[cost.end() :]
        class_ = re.match(r"\{([^{}]+)\}", text)
        if class_:
            resolution = self.classifier.references.proposed_vocabulary(
                "class", class_[1]
            )
            code = (
                resolution.target.get("vocabulary_code") if resolution.target else None
            )
            if resolution.issues or not isinstance(code, str):
                return None
            actuals.append(
                self.add(
                    "class",
                    "Concept",
                    "vocabulary.class.v1",
                    VocabularyReference(kind="vocabulary", key=("class", code)),
                    _span(start + 1, start + class_.end() - 1),
                )
            )
            start += class_.end()
            text = text[class_.end() :]
        for spelling in text.split("・"):
            span = _span(start, start + len(spelling))
            start = span.end + 1
            atom = self._atom(spelling, span)
            if atom is None:
                return None
            shape.append(atom[0])
            actuals.append(atom[1])
        signature = "_".join(shape)
        if signature == "trait_trait_kind":
            signature = "traits_kind"
        if signature in {"class_cost_le_kind", "cost_eq_kind_kind"}:
            signature = shape[0]
        signature = qualifier or signature
        return (signature, actuals) if signature in NOUNS else None

    def _atom(self, spelling: str, span: Span) -> tuple[str, str] | None:
        if spelling in _KINDS:
            return "kind", self.add(
                "kind",
                "CardKind",
                "card_kind.selection.v1",
                VocabularyReference(kind="vocabulary", key=("type", _KINDS[spelling])),
                span,
            )
        if spelling == "トークン":
            return "token", self.add(
                "token", "TokenStatus", "token_status.token.v1", "token", span
            )
        category = "rule_term" if spelling == "カード" else "trait"
        shape = "card" if spelling == "カード" else "trait"
        ref = self.reference(spelling, category, span, shape)
        return (shape, ref) if ref else None


def _zone_counts(
    builder: _Builder, source: re.Match[str], zones: re.Match[str], first: list[str]
) -> tuple[list[tuple[str, list[str]]], list[list[str]]] | None:
    noun = (
        zones["noun"]
        .removesuffix("の合計")
        .removesuffix("合計")
        .removesuffix("の枚数")
        .removesuffix("の")
    )
    noun_start = source.start("zone") + zones.start("noun")
    left = builder.noun(noun_start, noun)
    if left is None:
        return None
    right_owner = first[0]
    if zones["owner"]:
        p = _span(
            source.start("zone") + zones.start("owner"),
            source.start("zone") + zones.end("owner"),
        )
        right_owner = builder.add(
            "owner",
            "Player",
            "player.relative.v1",
            "self" if zones["owner"] == "自分" else "opponent",
            p,
        )
    p = _span(
        source.start("zone") + zones.start("right"),
        source.start("zone") + zones.end("right"),
    )
    right_zone = builder.add(
        "zone", "ZoneSet", "zone.single.v1", (_ZONES[zones["right"]],), p
    )
    return [left, left], [first, [right_owner, right_zone]]


def _filter_counts(
    builder: _Builder, start: int, tail: str, first: list[str]
) -> tuple[str, list[tuple[str, list[str]]], list[list[str]]] | None:
    if "や" in tail:
        a, b = tail.split("や", 1)
        link = "ya"
    elif tail == r"『X』『X』":
        a, b = "『X』", "『X』"
        link = "adjacent"
    else:
        return None
    left = builder.noun(start, a)
    right = builder.noun(start + len(a) + (1 if link == "ya" else 0), b)
    if left is None or right is None:
        return None
    shapes = [left, right]
    operands = [first, first]
    return link, shapes, operands


def _total_expression(
    builder: _Builder,
    match: re.Match[str],
    link: str,
    offset: str | None,
    shapes: list[tuple[str, list[str]]],
    operands: list[list[str]],
) -> None:
    text = builder.original.canonical_source
    counts = []
    for (shape, args), prefix in zip(shapes, operands, strict=True):
        if count_id(shape) not in {d.id for d in DESCRIPTORS}:
            return
        counts.append(
            builder.expression(
                count_id(shape),
                prefix + args,
                (_span(match.start("value"), match.start("value") + 1),),
            )
        )
    uses = tuple(
        _span(m.start(), m.end())
        for m in re.finditer(r"(?<![A-Za-z])X(?![A-Za-z])", text[: match.start()])
        if not any(
            q.start() <= m.start() < q.end() for q in re.finditer(r"『[^』]*』", text)
        )
    )
    if not uses or re.search(
        r"\{(?:ファンファーレ|起動|ラストワード)\}|【(?:進化時|超進化時|攻撃時|N)】",
        text[uses[0].end : match.start()],
    ):
        return
    builder.expression(
        f"aggregate.total.{link}{'.offset' if offset else ''}.v1",
        counts + ([offset] if offset else []),
        (*uses, _span(match.start(), match.start() + 1)),
    )


def _counts(
    builder: _Builder, source: re.Match[str], end: int, tail: str
) -> tuple[str, list[tuple[str, list[str]]], list[list[str]]] | None:
    text = builder.original.canonical_source
    first = builder.owner_zone(source)
    zones = re.match(
        r"(?P<left>[^と]+)と(?:(?P<owner>自分|相手)の)?(?P<right>場|手札|EXエリア)の(?P<noun>.+)",
        text[source.start("zone") : end],
    )
    if zones:
        parsed = _zone_counts(builder, source, zones, first)
        if parsed is None:
            return None
        link = "to"
        shapes, operands = parsed
    else:
        filtered = _filter_counts(builder, source.end(), tail, first)
        if filtered is None:
            return None
        link, shapes, operands = filtered
    return link, shapes, operands


def _totals(builder: _Builder) -> None:
    text = builder.original.canonical_source
    definitions = tuple(_DEFINITION.finditer(text))
    if len(definitions) != 1 or "合計" not in definitions[0]["value"]:
        return
    match = definitions[0]
    source = _TOTAL_SOURCE.match(text, match.start("value"))
    if source is None:
        return
    start = source.end()
    end = match.end("value")
    tail = text[start:end]
    offset = None
    if tail.endswith("+N"):
        span = _span(end - 1, end)
        offset = builder.add(
            "offset", "Nat", "", int(_raw(builder.raw, builder.original, span)), span
        )
        tail = tail[:-2]
    tail = (
        tail.removesuffix("の合計")
        .removesuffix("合計")
        .removesuffix("の枚数")
        .removesuffix("の")
    )
    parsed = _counts(builder, source, end, tail)
    if parsed is None:
        return
    link, shapes, operands = parsed
    if (link == "adjacent") != (offset is not None):
        return
    _total_expression(builder, match, link, offset, shapes, operands)


def _property_origin(
    builder: _Builder, match: re.Match[str]
) -> tuple[str, list[str], int] | None:
    text = builder.original.canonical_source
    boundary = (
        max(text.rfind("。", 0, match.start()), text.rfind(":", 0, match.start())) + 1
    )
    prefix = text[boundary : match.start()]
    source = list(_SOURCE.finditer(text, boundary, match.start()))
    args = []
    if source:
        src = source[-1]
        args = builder.owner_zone(src)
        start = src.end()
        origin = {"デッキ": "deck", "墓場": "graveyard", "場": "battlefield"}.get(
            src["zone"], ""
        )
    elif "その中から、" in prefix:
        start = boundary + prefix.index("その中から、") + len("その中から、")
        origin = "looked"
    elif "場のフォロワーを" in prefix:
        start = boundary + prefix.index("場の") + len("場の")
        origin = "payment"
    else:
        return None
    return origin, args, start


def _bound(builder: _Builder, match: re.Match[str], value: str, position: str) -> None:
    threshold = _span(match.start("value"), match.end("value"))
    actual = builder.add(
        "threshold",
        "Nat",
        "",
        int(_raw(builder.raw, builder.original, threshold)),
        threshold,
    )
    comparison = {"以上": "ge", "以下": "le", "と同じ": "eq", "": "eq"}[
        match["compare"]
    ]
    builder.expression(
        f"aggregate.bound.{position}.{comparison}.v1",
        [value, actual],
        (_span(match.start("compare"), match.end("use")),),
    )


def _selection_amount(builder: _Builder, match: re.Match[str]) -> None:
    amount = re.match(
        r"(?P<amount>N)枚(?P<mode>まで)?(?:選|探|場に)",
        builder.original.canonical_source[match.end() :],
    )
    if amount is None:
        return
    span = _span(
        match.end() + amount.start("amount"), match.end() + amount.end("amount")
    )
    item = leaf(
        "QuantitySpec",
        "selection_count",
        "quantity.selection_count.constant.v1",
        QuantitySpec(
            mode="up_to" if amount["mode"] else "exact",
            expr=Constant(
                kind="constant", value=int(_raw(builder.raw, builder.original, span))
            ),
        ),
        ((span,),),
        "N2-A8.selection",
    )
    builder.leaves.append(
        replace(
            item,
            slot=item.slot.model_copy(
                update={"name": f"aggregate_amount_{len(builder.leaves)}"}
            ),
            abstract=False,
            unit="枚",
        )
    )


def _properties(builder: _Builder) -> None:
    text = builder.original.canonical_source
    for match in _PROPERTY.finditer(text):
        parsed_origin = _property_origin(builder, match)
        if parsed_origin is None:
            continue
        origin, args, start = parsed_origin
        noun = text[start : match.start()].removesuffix("を").removesuffix("の")
        parsed = builder.noun(start, noun)
        if parsed is None:
            continue
        shape, noun_args = parsed
        prop = "attack" if match["property"] == "攻撃力" else "original_cost"
        id_ = sum_id(origin, shape, prop)
        if id_ not in {d.id for d in DESCRIPTORS}:
            continue
        value = builder.expression(
            id_,
            args + noun_args,
            (_span(match.start("property"), match.end("property") + len("の合計")),),
        )
        position = (
            "condition"
            if match["use"] == "なら"
            else "payment"
            if origin == "payment"
            else "selection"
        )
        _bound(builder, match, value, position)
        if position == "selection":
            _selection_amount(builder, match)


def _pairs(builder: _Builder) -> None:
    text = builder.original.canonical_source
    matches = tuple(_PAIR.finditer(text))
    if len(matches) != 1:
        return
    match = matches[0]
    if any(
        _raw(builder.raw, builder.original, _span(match.start(n), match.end(n)))
        not in {"1", "１"}
        for n in ("cards", "groups")
    ):
        return
    sources = list(_SOURCE.finditer(text, 0, match.start()))
    if not sources:
        return
    src = sources[-1]
    tail = text[src.end() : match.start()]
    operands = tail.split("と")
    if len(operands) != _PAIR_SIZE:
        return
    left = builder.noun(src.end(), operands[0])
    right = builder.noun(src.end() + len(operands[0]) + 1, operands[1])
    if left is None or right is None:
        return
    shape = (
        "named"
        if left[0] == right[0] == "name"
        else "kind"
        if left[0] == right[0] == "trait_kind"
        else ""
    )
    if shape:
        _pair_expression(builder, src, match, shape, left[1], right[1])


def _pair_expression(
    builder: _Builder,
    src: re.Match[str],
    match: re.Match[str],
    shape: str,
    left: list[str],
    right: list[str],
) -> None:
    prefix = builder.owner_zone(src)
    args = prefix + left + prefix + right
    rule = next(d for d in DESCRIPTORS if d.id == f"aggregate.pair.{shape}.v1")
    values = {item.slot.name: item.value for item in builder.leaves}
    if any(
        values[name] not in parameter.values
        for name, parameter in zip(args, rule.parameters, strict=True)
    ):
        return
    builder.expression(rule.id, args, (_span(match.start(), match.end()),))


def _project(
    leaves: list[Leaf], original: SourcePart, current: SourcePart
) -> list[Leaf]:
    roots = {
        item.slot.name
        for item in leaves
        if item.row == "N2-A8.selection"
        or (
            isinstance(item.value, (ContextExpression, PredicateExpr))
            and not item.value.expression_id.startswith(
                ("aggregate.count.", "aggregate.sum.")
            )
        )
    }
    if not roots:
        return []
    dependencies = {
        item.slot.name: item.value.leaves
        for item in leaves
        if isinstance(item.value, (ContextExpression, PredicateExpr))
    }
    required = set(roots)
    pending = list(roots)
    while pending:
        for name in dependencies.get(pending.pop(), ()):
            if name not in required:
                required.add(name)
                pending.append(name)
    additions = []
    for item in leaves:
        if item.slot.name not in required:
            continue
        groups = []
        for group in item.positions:
            origins = merged(
                tuple(s for span in group for s in _origins(original, span))
            )
            projected = merged(
                tuple(
                    _span(i, i + 1)
                    for i, u in enumerate(current.units)
                    if any(
                        a.start < b.end and b.start < a.end
                        for a in u.origins
                        for b in origins
                    )
                )
            )
            groups.append(projected)
        additions.append(
            replace(
                item,
                positions=tuple(groups),
                slot=item.slot.model_copy(
                    update={"occurrences": tuple(s for g in groups for s in g)}
                ),
            )
        )
    return additions


def _retain(
    current: SourcePart, found: Recognized, additions: list[Leaf]
) -> list[Leaf]:
    replaced = tuple(s for item in additions for g in item.positions for s in g)
    kept = []
    for slot in found.schema.slots:
        if any(
            a.start < b.end and b.start < a.end
            for a in slot.occurrences
            for b in replaced
        ):
            continue
        occurrences = tuple(o for o in found.occurrences if o.slot == slot.name)
        kept.append(
            Leaf(
                slot,
                found.values[slot.name],
                tuple(o.canonical_spans for o in occurrences if o.canonical_spans),
                "N1",
                abstract=any(
                    u.transformation == "leaf"
                    for span in slot.occurrences
                    for u in current.units[span.start : span.end]
                ),
                unit=occurrences[0].source_unit,
                resolution_rule=occurrences[0].resolution_rule,
                use=current.source_span.segments[0].start,
            )
        )
    return kept


def _issues(
    builder: _Builder, current: SourcePart, found: Recognized, leaves: list[Leaf]
) -> tuple[str, ...]:
    raw, original, classifier = builder.raw, builder.original, builder.classifier
    covered = merged(
        tuple(
            s
            for item in leaves
            for group in item.positions
            for span in group
            for s in _origins(current, span)
        )
    )
    issues = set(found.issues)
    hints = classifier.candidate(raw, original).slots
    for reason in (
        "numeric_role_requires_review",
        "signed_numeric_requires_review",
        "n0_numeric_construction_unresolved",
    ):
        bad = (
            [h for h in hints if h.type == "uint"]
            if reason == "n0_numeric_construction_unresolved"
            else [h for h in hints if reason in h.issues]
        )
        if bad and all(
            all(
                any(a.start <= b.start < b.end <= a.end for a in covered)
                for b in h.source_segments
            )
            for h in bad
        ):
            issues.discard(reason)
    return tuple(sorted(issues))


def _apply(
    raw: str,
    current: SourcePart,
    found: Recognized,
    original: SourcePart,
    classifier: Classifier,
    scope: str,
) -> tuple[SourcePart, Recognized, tuple[str, ...]] | None:
    builder = _Builder(raw, original, classifier, scope, [])
    _totals(builder)
    _properties(builder)
    _pairs(builder)
    additions = _project(builder.leaves, original, current)
    if not additions:
        return None
    kept = _retain(current, found, additions)
    kept.extend(additions)
    issues = _issues(builder, current, found, kept)
    updated, result, rows = transform(
        raw,
        current,
        kept,
        (),
        replace(found, issues=issues, semantics=None),
        separate_occurrences=True,
    )
    names = {item.slot.name: f"leaf_{i}" for i, item in enumerate(kept)}
    values = {
        name: value.model_copy(
            update={"leaves": tuple(names.get(n, n) for n in value.leaves)}
        )
        if isinstance(value, (Expression, ContextExpression, PredicateExpr))
        else value
        for name, value in result.values.items()
    }
    slots = tuple(
        slot.model_copy(update={"expression": values[slot.name]})
        if isinstance(values[slot.name], (ContextExpression, PredicateExpr))
        else slot
        for slot in result.schema.slots
    )
    return (
        updated,
        replace(
            result,
            values=values,
            schema=result.schema.model_copy(update={"slots": slots}),
        ),
        rows,
    )


def prepare_totals(
    raw: str,
    prepared: PreparedField,
    classifier: Classifier,
    _context: CardContext | None,
) -> PreparedField:
    """Family callbacks reconstruct from raw source; target wording never grants acceptance."""
    derived = prepared.derived
    if derived.field.source.field not in {"effect", "section"} or not isinstance(
        derived.field.source.owner, (FaceRevisionOwner, PrintingFaceOwner)
    ):
        return prepared
    parts = list(derived.field.parts)
    recognized = list(derived.recognized)
    rows = list(derived.registry_rows)
    interfaces = list(prepared.interfaces)
    for i, original in enumerate(derived.original.parts):
        if original.source_span.role != "body" or not any(
            s in original.canonical_source for s in ("合計", "組につき")
        ):
            continue
        semantics = recognized[i].semantics
        existing = interfaces[i] or (semantics.projection if semantics else None)
        scope = (
            next((s.id for s in existing.scopes if s.kind == "ability"), "aggregate")
            if existing
            else "aggregate"
        )
        updated = _apply(raw, parts[i], recognized[i], original, classifier, scope)
        if updated is None:
            continue
        parts[i], recognized[i], rows[i] = updated
        interfaces[i] = (
            existing
            if existing and existing.scopes
            else Projection(
                projection_kind="pending",
                discriminator=None,
                scopes=(Scope(id=scope, parent=None, kind="ability"),),
                imports=(),
                exports=(),
            )
        )
    return replace(
        prepared,
        derived=replace(
            derived,
            field=replace(derived.field, parts=tuple(parts)),
            recognized=tuple(recognized),
            registry_rows=tuple(rows),
        ),
        interfaces=tuple(interfaces),
    )


FAMILY = PreparationFamily("A8-A10", DESCRIPTORS, prepare_totals)

"""Sealed preparation reads require both a local definition and supported use points."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Domain,
    Expression,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSlot,
    PrintingFaceOwner,
    Span,
    VocabularyReference,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.domains.translations.four_layer_expressions import (
    DEFINITIONS,
    DOMAIN_CODE,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_n1_transform import transform
from sve_carddb.domains.translations.four_layer_normalizer import SourceField
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import LeafType
    from sve_carddb.contracts.source_binding import TypedValue
    from sve_carddb.domains.translations.four_layer_analysis import SourceAnalysis
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart

_DEFINITION = re.compile(
    r"Xは「(?P<owner>自分|相手)の(?P<zone>場|手札|デッキ|墓場|EXエリア|消滅領域)の"
    r"(?P<exclude>他の)?(?P<filter>.*?)の?(?P<measure>枚数|数)"
    r"(?P<operator>[+-])?(?P<offset>N)?」である。"
)
_ZONES = {
    "場": "battlefield",
    "手札": "hand",
    "デッキ": "deck",
    "墓場": "graveyard",
    "EXエリア": "ex",
    "消滅領域": "banish",
}
_KINDS = {"フォロワー": "follower", "アミュレット": "amulet", "スペル": "spell"}
_ROW = "N2-B11.current_zone_count"


def prepare_reads(
    raw: str, derived: SourceAnalysis, classifier: Classifier
) -> SourceAnalysis:
    """Change only preparation body parts; pending whole-line semantics stay pending."""
    parts = list(derived.field.parts)
    recognized = list(derived.recognized)
    rows = list(derived.registry_rows)
    for index, part in enumerate(parts):
        if (
            not isinstance(
                derived.field.source.owner, (FaceRevisionOwner, PrintingFaceOwner)
            )
            or part.source_span.role != "body"
            or derived.field.source.field
            not in {
                "effect",
                "section",
            }
        ):
            continue
        found = recognized[index]
        additions = _sep(raw, part)
        count = _count(raw, part, found, classifier)
        if count is not None:
            additions.extend(count)
        if not additions:
            continue
        kept = _retained(part, found, additions)
        issues = _issues(
            raw, part, derived.original.parts[index], found, kept, classifier
        )
        updated, result, registry = transform(
            raw,
            part,
            kept,
            (),
            replace(found, issues=issues, semantics=None),
            separate_occurrences=True,
        )
        names = {item.slot.name: f"leaf_{i}" for i, item in enumerate(kept)}
        values = {
            name: value.model_copy(
                update={"leaves": tuple(names[n] for n in value.leaves)}
            )
            if isinstance(value, Expression)
            else value
            for name, value in result.values.items()
        }
        parts[index], recognized[index], rows[index] = (
            updated,
            replace(result, values=values),
            registry,
        )
    return replace(
        derived,
        field=SourceField(derived.field.source, tuple(parts)),
        recognized=tuple(recognized),
        registry_rows=tuple(rows),
    )


def _raw(raw: str, part: SourcePart, span: Span) -> str:
    origins = merged(
        tuple(s for u in part.units[span.start : span.end] for s in u.origins)
    )
    return "".join(raw[s.start : s.end] for s in origins)


def _nat(role: str, value: int, span: Span, row: str) -> Leaf:
    return Leaf(
        LeafSlot(
            name=role,
            type="Nat",
            role=role,
            domain=Domain(values=(), min=0, max=SAFE_INTEGER),
            required=True,
            occurrences=(span,),
        ),
        value,
        ((span,),),
        row,
        abstract=False,
    )


def _sep(raw: str, part: SourcePart) -> list[Leaf]:
    return [
        _nat(
            "threshold",
            0,
            Span(start=m.start("value"), end=m.end("value")),
            "N2-B13.sep_zero",
        )
        for m in re.finditer(
            r"(?:^|(?<=。)|(?<=}))自分のSEPが(?P<value>N)(?:なら、|である限り、)",
            part.canonical_source,
        )
        if _raw(raw, part, Span(start=m.start("value"), end=m.end("value")))
        in {"0", "０"}
    ]


def _uses(text: str, definition: re.Match[str]) -> tuple[Span, ...]:
    quoted = tuple(re.finditer(r"『[^』]*』", text))
    positions = tuple(
        m
        for m in re.finditer(r"(?<![A-Za-z0-9_])X(?![A-Za-z0-9_])", text)
        if not any(q.start() <= m.start() < q.end() for q in quoted)
        and m.start() != definition.start()
    )
    if not positions or any(m.start() >= definition.start() for m in positions):
        return ()
    between = text[positions[0].start() : definition.start()]
    if re.search(
        r"\{(?:ファンファーレ|起動|ラストワード)\}|【(?:進化時|攻撃時)】|[①②③]|割り振",
        between,
    ):
        return ()
    for match in positions:
        before, after = text[: match.start()], text[match.end() :]
        if (
            re.search(r"\{(?:攻撃力|体力)\}[+-]$|コストを-$|元のコスト$", before)
            or after.startswith(("ダメージ。", "枚引く。"))
            or (
                before.endswith("』")
                and re.match(r"体を(?:\{leaf_\d+\}に)?出す。", after)
            )
        ):
            continue
        return ()
    return tuple(Span(start=m.start(), end=m.end()) for m in positions)


def _read(
    parameter: str,
    type_name: LeafType,
    domain: str,
    value: TypedValue,
    start: int,
    end: int,
) -> Leaf:
    item = leaf(
        type_name,
        "read_" + parameter,
        domain,
        value,
        ((Span(start=start, end=end),),),
        _ROW,
    )
    return replace(
        item,
        slot=item.slot.model_copy(update={"name": parameter}),
        abstract=type_name != "CardName",
    )


def _filters(
    text: str, start: int, found: Recognized, classifier: Classifier
) -> list[Leaf] | None:
    if not text:
        return []
    if text == "『X』":
        span = Span(start=start + 1, end=start + 2)
        name = next(
            (
                found.values[s.name]
                for s in found.schema.slots
                if s.type == "CardName" and s.occurrences == (span,)
            ),
            None,
        )
        return (
            [_read("name", "CardName", "card_name.any.v1", name, span.start, span.end)]
            if isinstance(name, CardNameReference)
            else None
        )
    kind = next((k for k in (*_KINDS, "カード", "トークン") if text.endswith(k)), None)
    if kind is None:
        return None
    end = start + len(text)
    if kind in _KINDS:
        item = _read(
            "kind",
            "CardKind",
            "card_kind.selection.v1",
            VocabularyReference(kind="vocabulary", key=("type", _KINDS[kind])),
            end - len(kind),
            end,
        )
    elif kind == "カード":
        item = _read(
            "card",
            "Concept",
            "concept.rule_term.v1",
            GlossaryReference(kind="glossary", key="term:object.card"),
            end - len(kind),
            end,
        )
    else:
        item = _read(
            "token_status",
            "TokenStatus",
            "token_status.token.v1",
            "token",
            end - len(kind),
            end,
        )
    modifiers = _modifier(text[: -len(kind)], start, classifier)
    return [item, *modifiers] if modifiers is not None else None


def _modifier(text: str, start: int, classifier: Classifier) -> list[Leaf] | None:
    if not text:
        return []
    if text.startswith("{") and text.endswith("}"):
        resolution = classifier.references.proposed_vocabulary("class", text[1:-1])
        target = resolution.target
        code = target.get("vocabulary_code") if target is not None else None
        if resolution.issues or not isinstance(code, str):
            return None
        return [
            _read(
                "class",
                "Concept",
                "vocabulary.class.v1",
                VocabularyReference(kind="vocabulary", key=("class", code)),
                start,
                start + len(text),
            )
        ]
    keyword = text.startswith("【") and text.endswith("】を持つ")
    spelling = text[1:-4] if keyword else text.removesuffix("・")
    category = "keyword" if keyword else "trait"
    terms = [
        term
        for term in classifier.terms.values()
        if term.category == category and term.source_ja == spelling
    ]
    if len(terms) != 1 or (not keyword and not text.endswith("・")):
        return None
    offset = 1 if keyword else 0
    return [
        _read(
            category,
            "Concept",
            f"concept.{category}.v1",
            GlossaryReference(kind="glossary", key=terms[0].id),
            start + offset,
            start + offset + len(spelling),
        )
    ]


def _count(
    raw: str, part: SourcePart, found: Recognized, classifier: Classifier
) -> list[Leaf] | None:
    text = part.canonical_source
    matches = tuple(_DEFINITION.finditer(text))
    if len(matches) != 1:
        return None
    match = matches[0]
    uses = _uses(text, match)
    if not uses or text.count("Xは「") != 1:
        return None
    additions = [
        _read(
            "owner",
            "Player",
            "player.relative.v1",
            "self" if match["owner"] == "自分" else "opponent",
            match.start("owner"),
            match.end("owner"),
        ),
        _read(
            "zone",
            "ZoneSet",
            "zone.single.v1",
            (_ZONES[match["zone"]],),
            match.start("zone"),
            match.end("zone"),
        ),
    ]
    filters = _filters(
        match["filter"].removesuffix("の"), match.start("filter"), found, classifier
    )
    if filters is None:
        return None
    additions.extend(filters)
    qualifiers = (("exclude_this",) if match["exclude"] else ()) + (
        ("token",) if any(a.slot.role == "read_token_status" for a in additions) else ()
    )
    parameters = tuple(
        p
        for p in (
            "owner",
            "zone",
            "class",
            "keyword",
            "token_status",
            "kind",
            "card",
            "trait",
            "name",
        )
        if any(a.slot.name == p for a in additions)
    )
    operator = {"+": "add_offset", "-": "subtract_offset"}.get(match["operator"])
    if bool(operator) != bool(match["offset"]):
        return None
    descriptor = next(
        (
            d
            for d in DEFINITIONS.values()
            if tuple(p for p in d.parameters if p != "offset") == parameters
            and d.qualifiers == qualifiers
            and d.operator == operator
        ),
        None,
    )
    if descriptor is None:
        return None
    if operator:
        span = Span(start=match.start("offset"), end=match.end("offset"))
        value = int(_raw(raw, part, span))
        offset = _nat("arithmetic_offset", value, span, "N2-B03.offset")
        additions.append(
            replace(offset, slot=offset.slot.model_copy(update={"name": "offset"}))
        )
    expr = leaf(
        "QuantityExpr",
        "count_expression",
        DOMAIN_CODE,
        Expression(
            kind="expression", expression_id=descriptor.id, leaves=descriptor.parameters
        ),
        tuple((s,) for s in (*uses, Span(start=match.start(), end=match.start() + 1))),
        _ROW,
    )
    additions.append(
        replace(expr, slot=expr.slot.model_copy(update={"name": "count_expression"}))
    )
    return additions


def _retained(part: SourcePart, found: Recognized, additions: list[Leaf]) -> list[Leaf]:
    replaced = tuple(s for item in additions for group in item.positions for s in group)
    kept = []
    for slot in found.schema.slots:
        occurrences = tuple(o for o in found.occurrences if o.slot == slot.name)
        if any(
            a.start < b.end and b.start < a.end
            for a in slot.occurrences
            for b in replaced
        ):
            continue
        kept.append(
            Leaf(
                slot,
                found.values[slot.name],
                tuple(o.canonical_spans for o in occurrences if o.canonical_spans),
                "N1",
                abstract=any(
                    u.transformation == "leaf"
                    for s in slot.occurrences
                    for u in part.units[s.start : s.end]
                ),
                unit=occurrences[0].source_unit,
                resolution_rule=occurrences[0].resolution_rule,
                use=part.units[0].origins[0].start if part.units else 0,
            )
        )
    return [*kept, *additions]


def _issues(
    raw: str,
    part: SourcePart,
    original: SourcePart,
    found: Recognized,
    leaves: list[Leaf],
    classifier: Classifier,
) -> tuple[str, ...]:
    covered = merged(
        tuple(
            origin
            for item in leaves
            for group in item.positions
            for span in group
            for unit in part.units[span.start : span.end]
            for origin in unit.origins
        )
    )
    candidate = classifier.candidate(raw, original)

    def repaired(spans: tuple[Span, ...]) -> bool:
        return bool(spans) and all(
            any(a.start <= b.start < b.end <= a.end for a in covered) for b in spans
        )

    issues = set(found.issues)
    for reason in issues & {
        "numeric_role_requires_review",
        "signed_numeric_requires_review",
    }:
        hints = tuple(h for h in candidate.slots if reason in h.issues)
        if hints and all(repaired(h.source_segments) for h in hints):
            issues.discard(reason)
    for reason, hints in (
        (
            "n0_numeric_construction_unresolved",
            tuple(h for h in candidate.slots if h.type == "uint"),
        ),
        (
            "n0_vocabulary_construction_unresolved",
            tuple(
                h
                for h in candidate.slots
                if h.target and h.target.get("kind") == "vocabulary"
            ),
        ),
    ):
        if hints and all(repaired(h.source_segments) for h in hints):
            issues.discard(reason)
    return tuple(sorted(issues))

"""N1 descriptors and source evidence, sharing N0 lexical and numeric dependencies."""

import re
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Constant,
    Domain,
    GlossaryReference,
    LeafSlot,
    QuantitySpec,
    Span,
    VocabularyReference,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.contracts.source_binding import CodeDomain, ZoneDomain
from sve_carddb.domains.translations.four_layer_n1 import PLAYERS, Filter, Operand
from sve_carddb.domains.translations.four_layer_units import CountContext, source_unit
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import LeafType
    from sve_carddb.contracts.source_binding import ClosedDomain, TypedValue
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.recognition.models import Hint

ZONE_CODES = tuple(
    sorted({"battlefield", "hand", "deck", "evolve_deck", "graveyard", "ex", "banish"})
)
DOMAINS: Mapping[str, ClosedDomain] = {
    "player.relative.v1": CodeDomain(type="Player", codes=("opponent", "self")),
    "player.self.v1": CodeDomain(type="Player", codes=("self",)),
    "phase.named.v1": CodeDomain(type="Phase", codes=("end", "main", "start")),
    "deck_position.edge.v1": CodeDomain(type="DeckPosition", codes=("bottom", "top")),
    "token_status.token.v1": CodeDomain(type="TokenStatus", codes=("token",)),
    "zone.single.v1": ZoneDomain(
        type="ZoneSet",
        zones=ZONE_CODES,
        combinations=tuple((code,) for code in ZONE_CODES),
    ),
    "zone.hand.v1": ZoneDomain(
        type="ZoneSet", zones=("hand",), combinations=(("hand",),)
    ),
    "zone.selection.battlefield_ex.v1": ZoneDomain(
        type="ZoneSet",
        zones=("battlefield", "ex"),
        combinations=(("battlefield", "ex"),),
    ),
}


@dataclass(frozen=True)
class Leaf:
    slot: LeafSlot
    value: TypedValue
    positions: tuple[tuple[Span, ...], ...]
    row: str
    abstract: bool = True
    unit: str | None = None
    resolution_rule: str | None = None
    use: int = 0


def leaf(
    type_name: LeafType,
    role: str,
    domain: str,
    value: TypedValue,
    positions: tuple[tuple[Span, ...], ...],
    row: str,
) -> Leaf:
    """Keep named descriptors independent of the catalog member observed at this use."""
    spans = tuple(
        sorted(
            {span for group in positions for span in group},
            key=lambda s: (s.start, s.end),
        )
    )
    return Leaf(
        LeafSlot(
            name="leaf",
            type=type_name,
            role=role,
            domain=Domain(values=(domain,), min=None, max=None),
            required=bool(spans),
            occurrences=spans,
        ),
        value,
        positions,
        row,
    )


def structural(operand: Operand, text: str) -> list[Leaf]:
    """Explicit owners always use a relative domain, even when the observed value is self."""
    row = operand.row
    result = []
    if operand.owners:
        role = "phase_owner" if operand.phase is not None else operand.role + "_owner"
        result.append(
            leaf(
                "Player",
                role,
                "player.relative.v1",
                PLAYERS[text[operand.owners[0].start : operand.owners[0].end]],
                tuple((span,) for span in operand.owners),
                row,
            )
        )
    if operand.zones:
        domain = (
            "zone.selection.battlefield_ex.v1"
            if len(operand.zones) > 1
            else "zone.hand.v1"
            if operand.verb in {"add_hand", "return"} and operand.zones == ("hand",)
            else "zone.single.v1"
        )
        result.append(
            leaf(
                "ZoneSet",
                operand.role + "_zone",
                domain,
                operand.zones,
                (operand.zone_spans,),
                row,
            )
        )
    if operand.position is not None:
        assert operand.edge is not None
        result.append(
            leaf(
                "DeckPosition",
                operand.role + "_position",
                "deck_position.edge.v1",
                operand.edge,
                ((operand.position,),),
                row,
            )
        )
    if operand.phase is not None:
        assert operand.phase_code is not None
        result.append(
            leaf(
                "Phase",
                "phase_trigger",
                "phase.named.v1",
                operand.phase_code,
                ((operand.phase,),),
                row,
            )
        )
    return result


def preserve_modifiers(operand: Operand, classifier: Classifier) -> Operand:
    """An unadopted braced class remains a restriction, even if N0 recognizes its ability."""
    noun = operand.noun
    if noun is not None and any(
        item.kind == "braced" and _class_filter(item, classifier) is None
        for item in noun.filters
    ):
        return replace(operand, noun=replace(noun, opaque=True))
    return operand


def _class_filter(item: Filter, classifier: Classifier) -> VocabularyReference | None:
    resolution = classifier.references.proposed_vocabulary("class", item.spelling[1:-1])
    code = resolution.target.get("vocabulary_code") if resolution.target else None
    return (
        VocabularyReference(kind="vocabulary", key=("class", code))
        if not resolution.issues and isinstance(code, str)
        else None
    )


def filters(
    operand: Operand, classifier: Classifier
) -> tuple[list[Leaf], tuple[str, ...]] | None:
    """Noun evidence distinguishes adopted classes, traits, kinds and explicit token filters.

    A trait or generic-card token without exactly one glossary concept leaves the whole
    operand to N0 instead of blocking the line.
    """
    if operand.noun is None:
        return [], ()
    result = []
    issues = set()
    for item in operand.noun.filters:
        type_name: LeafType = "Concept"
        role = item.kind
        domain = "concept.trait.v1"
        value: TypedValue
        if item.kind == "kind":
            resolution = classifier.references.proposed_vocabulary(
                "type", item.spelling
            )
            code = (
                resolution.target.get("vocabulary_code") if resolution.target else None
            )
            if (
                resolution.issues
                or not isinstance(code, str)
                or code not in {"follower", "amulet", "spell"}
            ):
                issues.add("missing_n1_kind_vocabulary")
                continue
            type_name, role, domain = (
                "CardKind",
                "counted_kind",
                "card_kind.selection.v1",
            )
            if operand.zones in {("battlefield",), ("ex",)} and operand.unit in {
                "体",
                "つ",
            }:
                domain = (
                    "card_kind."
                    + ("follower" if operand.unit == "体" else "amulet")
                    + ".v1"
                )
            value = VocabularyReference(kind="vocabulary", key=("type", code))
        elif item.kind == "braced":
            class_value = _class_filter(item, classifier)
            if class_value is None:
                # N0's braced ability is preserved inside an opaque restriction.
                continue
            role, domain, value = "class_filter", "vocabulary.class.v1", class_value
        elif item.kind == "token":
            type_name, role, domain, value = (
                "TokenStatus",
                "token_filter",
                "token_status.token.v1",
                "token",
            )
        else:
            role = "rule_term" if item.kind == "card" else "trait"
            domain = "concept." + role + ".v1"
            reference = _glossary_filter(item, role, classifier)
            if reference is None:
                return None
            value = reference
        result.append(
            leaf(
                type_name,
                role,
                domain,
                value,
                (
                    (
                        Span(start=item.span.start + 1, end=item.span.end - 1)
                        if item.kind == "braced"
                        else item.span,
                    ),
                ),
                "N1-SRC08." + item.kind,
            )
        )
    return result, tuple(sorted(issues))


def _glossary_filter(
    item: Filter, role: str, classifier: Classifier
) -> GlossaryReference | None:
    identifier = "term:object.card" if item.kind == "card" else None
    terms = tuple(
        t
        for t in classifier.terms.values()
        if t.category == role
        and t.source_ja == item.spelling
        and (identifier is None or t.id == identifier)
    )
    return (
        GlossaryReference(kind="glossary", key=terms[0].id) if len(terms) == 1 else None
    )


def valid_unit(operand: Operand) -> bool:
    """The counted source set supplies units; an arriving zone cannot change them."""
    if operand.unit is None:
        return True
    kind = operand.noun.kind if operand.noun else "card"
    zones = (
        operand.zones or ("deck",) if operand.position is not None else operand.zones
    )
    if operand.row == "N1-SRC02.hand_cost":
        return operand.unit == "枚"
    if len(zones) > 1:
        context = CountContext(
            "select.union_unrestricted.v1",
            kind,
            zones,
            frozenset({False, True}),
            "union_cardinality",
        )
    else:
        token = operand.noun is not None and any(
            f.kind == "token" for f in operand.noun.filters
        )
        constructor = (
            "select.card.v1"
            if token
            else "select.ex_unrestricted.v1"
            if zones == ("ex",)
            else "select.unrestricted.v1"
        )
        context = CountContext(
            constructor,
            kind,
            tuple("banished" if z == "banish" else z for z in zones),
            True if token else frozenset({False, True}),
            "cardinality",
        )
    return source_unit(context, operand.unit).merge_allowed


def quantity(operand: Operand, hint: Hint, part: SourcePart, raw: str) -> Leaf | None:
    """Only enabled existing numeric evidence can supply an outer selection value."""
    if (
        operand.number is None
        or hint.occurrence != operand.number
        or hint.issues
        or hint.value is None
        or not valid_unit(operand)
    ):
        return None
    selection = operand.row.startswith(("N1-SRC01", "N1-SRC10"))
    type_name: LeafType = "QuantitySpec" if selection else "Nat"
    role = "selection_count" if selection else "count"
    value: TypedValue = (
        QuantitySpec(
            mode="up_to" if operand.mode == "up_to" else "exact",
            expr=Constant(kind="constant", value=hint.value),
        )
        if selection
        else hint.value
    )
    bounds = (
        Domain(values=("quantity.selection_count.constant.v1",), min=None, max=None)
        if selection
        else Domain(values=(), min=0, max=SAFE_INTEGER)
    )
    unit = None
    if operand.unit:
        first = operand.number.end
        unit = "".join(
            raw[s.start : s.end]
            for s in merged(
                tuple(
                    s
                    for u in part.units[first : first + len(operand.unit)]
                    for s in u.origins
                )
            )
        )
    return Leaf(
        LeafSlot(
            name="leaf",
            type=type_name,
            role=role,
            domain=bounds,
            required=True,
            occurrences=(operand.number,),
        ),
        value,
        ((operand.number,),),
        operand.row,
        False,
        unit,
    )


THRESHOLDS = {
    "コンボ": ("combo", "keyword_threshold_combo"),
    "レッスン": ("lesson", "keyword_threshold_lesson"),
    "ネクロチャージ": ("necrocharge", "keyword_threshold_necrocharge"),
    "スペルチェイン": ("spell_chain", "keyword_threshold_spell_chain"),
    "NC": ("necrocharge", "keyword_alias_nc"),
    "SC": ("spell_chain", "keyword_alias_sc"),
}
_THRESHOLD = re.compile(
    r"【(?P<ability>コンボ|レッスン|ネクロチャージ|スペルチェイン|NC|SC)_(?P<number>N)】"
)
_KEYWORD = re.compile(r"【(?P<keyword>[^【】]+)】")
_KEYWORD_LINE = re.compile(
    r"(?:【(?P<keyword>[^【】]+)】[ 、]*|\{(?P<ability>[^{}]+)\})"
)


def keyword_line(text: str) -> tuple[re.Match[str], ...]:
    """Whole-line failures retain PR 1's treatment; prose tokens do not authorize the line."""
    matches = tuple(_KEYWORD_LINE.finditer(text))
    return matches if matches and "".join(m[0] for m in matches) == text else ()


def _quick(part: SourcePart, classifier: Classifier) -> list[Leaf]:
    if "braced_ability_reference" not in classifier.rules.enabled():
        return []
    terms = tuple(
        t
        for t in classifier.terms.values()
        if t.source_ja == "クイック" and t.category == "ability"
    )
    if len(terms) != 1 or terms[0].id != "term:ability.quick":
        return []
    return [
        replace(
            leaf(
                "Concept",
                "ability",
                "concept.ability.v1",
                GlossaryReference(kind="glossary", key=terms[0].id),
                ((Span(start=m.start("ability"), end=m.end("ability")),),),
                "N1-SRC09.quick",
            ),
            abstract=False,
        )
        for m in re.finditer(r"\{(?P<ability>クイック)\}", part.canonical_source)
    ]


def _thresholds(
    part: SourcePart, hints: tuple[Hint, ...]
) -> tuple[list[Leaf], set[str]]:
    result = []
    dependencies = set()
    for head in _THRESHOLD.finditer(part.canonical_source):
        code, rule = THRESHOLDS[head["ability"]]
        hint = next(
            (h for h in hints if h.occurrence.start == head.start("number")), None
        )
        if hint is None or hint.issues or hint.rule_id != rule:
            continue
        dependencies.add(rule)
        result.append(
            leaf(
                "Concept",
                "ability",
                "concept.ability.v1",
                GlossaryReference(kind="glossary", key="term:ability." + code),
                ((Span(start=head.start("ability"), end=head.end("ability")),),),
                "N1-SRC09.threshold." + code,
            )
        )
    return result, dependencies


def _keyword(
    part: SourcePart, match: re.Match[str], classifier: Classifier
) -> tuple[Leaf | None, str | None]:
    span = Span(start=match.start("keyword"), end=match.end("keyword"))
    if any(u.transformation == "digits" for u in part.units[span.start : span.end]):
        return None, "keyword_source_replacement_unresolved" if match[
            "keyword"
        ] != "N" else None
    terms = tuple(
        t
        for t in classifier.terms.values()
        if t.category == "keyword" and t.source_ja == match["keyword"]
    )
    if not terms:
        other_terms = tuple(
            t for t in classifier.terms.values() if t.source_ja == match["keyword"]
        )
        if len(other_terms) == 1:
            return None, None
        return None, "missing_keyword_concept"
    if len(terms) != 1:
        return None, "ambiguous_keyword_concept"
    return replace(
        leaf(
            "Concept",
            "keyword",
            "concept.keyword.v1",
            GlossaryReference(kind="glossary", key=terms[0].id),
            ((span,),),
            "N1-SRC09.keyword",
        ),
        abstract=False,
    ), None


def lexical(
    part: SourcePart, classifier: Classifier, hints: tuple[Hint, ...]
) -> tuple[list[Leaf], tuple[str, ...], tuple[str, ...]]:
    """Threshold heads are an abstract exception; other keyword spelling stays fixed."""
    text = part.canonical_source
    protected = tuple(re.finditer(r"『X』|「[^「」]*」", text))
    result, dependencies = _thresholds(part, hints)
    quick = _quick(part, classifier)
    result.extend(quick)
    if quick:
        dependencies.add("braced_ability_reference")
    issues = set()
    if "bracket_keyword_reference" in classifier.rules.enabled():
        for match in _KEYWORD.finditer(text):
            if _THRESHOLD.fullmatch(match[0]) or any(
                m.start() <= match.start() < m.end() for m in protected
            ):
                continue
            item, reason = _keyword(part, match, classifier)
            if reason and (
                keyword_line(text)
                or any(
                    t.category == "keyword" and t.source_ja == match["keyword"]
                    for t in classifier.terms.values()
                )
            ):
                issues.add(reason)
            if item:
                result.append(item)
                dependencies.add("bracket_keyword_reference")
    result = [
        item
        for item in result
        if not any(
            m.start() <= item.slot.occurrences[0].start < m.end() for m in protected
        )
    ]
    return result, tuple(sorted(issues)), tuple(sorted(dependencies))

"""Sealed payment, die, header and reminder sources for isolated N2 preparation."""

import re
import unicodedata
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Domain,
    GlossaryReference,
    LeafSlot,
    Span,
    VocabularyReference,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_n1_transform import transform
from sve_carddb.domains.translations.four_layer_normalizer import SourceField
from sve_carddb.domains.translations.four_layer_semantics import classify_semantics
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart

_DIE_SIDES = 6

_EARTH = re.compile(
    r"【(?P<ability>土の秘術)(?:_(?P<amount>N))?】(?=:|相手|自分|これ|それ|下記|『|代わりに)"
)
_DICE = re.compile(
    r"(?:^|(?<=[。、]))(?:出た目が)?(?P<lo>N)(?:(?P<link>から|か)(?P<hi>N))?なら、"
)
_HEADER = re.compile(
    r"『[^』]+』\{[^{}]+\}[^{}]+・(?P<kind>クレスト|イクイップメント)(?:\{コスト[0-9０-９]+\})?"
)
_REMINDER = re.compile(
    r"[（(]このターン、これを\{攻撃力\}(?P<attack>[0-9０-９]+)/\{体力\}(?P<health>[0-9０-９]+)のフォロワーにする[）)]"
)


def _raw_spans(part: SourcePart, span: Span) -> tuple[Span, ...]:
    return merged(
        tuple(s for u in part.units[span.start : span.end] for s in u.origins)
    )


def _number(raw: str, part: SourcePart, span: Span) -> int:
    return int(
        unicodedata.normalize(
            "NFKC", "".join(raw[s.start : s.end] for s in _raw_spans(part, span))
        )
    )


def _nat(
    role: str,
    value: int,
    span: Span | None,
    row: str,
    *,
    minimum: int = 0,
    maximum: int = SAFE_INTEGER,
) -> Leaf:
    return Leaf(
        LeafSlot(
            name="leaf",
            type="Nat",
            role=role,
            domain=Domain(values=(), min=minimum, max=maximum),
            # A pure omitted amount stays optional so a target may keep the printed wording.
            required=span is not None,
            occurrences=(span,) if span else (),
        ),
        value,
        ((span,),) if span else (),
        row,
        abstract=False,
    )


def _earth(raw: str, part: SourcePart, classifier: Classifier) -> list[Leaf]:
    text = part.canonical_source
    result: list[Leaf] = []
    for match in _EARTH.finditer(text):
        ability = Span(start=match.start("ability"), end=match.end("ability"))
        term = classifier.terms.get("term:ability.earth_rite")
        if term is None or term.category != "ability":
            continue
        result.append(
            leaf(
                "Concept",
                "ability",
                "concept.ability.v1",
                GlossaryReference(kind="glossary", key=term.id),
                ((ability,),),
                "N2-B04",
            )
        )
        amount = (
            Span(start=match.start("amount"), end=match.end("amount"))
            if match["amount"]
            else None
        )
        value = _number(raw, part, amount) if amount else 1
        if value < 1:
            result.pop()
            continue
        item = _nat("earth_rite_amount", value, amount, "N2-B04", minimum=1)
        if amount is None:
            item = replace(
                item,
                resolution_rule="earth_rite.omitted_amount.v1",
                use=_raw_spans(part, ability)[0].start,
            )
        result.append(item)
    return result


def _dice(raw: str, part: SourcePart, classifier: Classifier) -> list[Leaf]:
    text = part.canonical_source
    result: list[Leaf] = []
    term = classifier.terms.get("term:object.die")
    if term is None or term.category != "rule_term":
        return []
    rolled = re.search(r"サイコロをふる。|自分がサイコロをふったとき、", text)
    if rolled and (matches := tuple(_DICE.finditer(text, rolled.end()))):
        for match in matches:
            lo = Span(start=match.start("lo"), end=match.end("lo"))
            hi = (
                Span(start=match.start("hi"), end=match.end("hi"))
                if match["hi"]
                else None
            )
            low = _number(raw, part, lo)
            high = _number(raw, part, hi) if hi else low
            if not (1 <= low <= _DIE_SIDES and 1 <= high <= _DIE_SIDES) or (
                match["link"] == "から" and low > high
            ):
                return []
            interval = match["link"] == "から"
            result.append(
                _nat(
                    "dice_lower_bound" if interval else "dice_face",
                    low,
                    lo,
                    "N2-B06",
                    minimum=1,
                    maximum=_DIE_SIDES,
                )
            )
            if hi:
                result.append(
                    _nat(
                        "dice_upper_bound" if interval else "dice_face",
                        high,
                        hi,
                        "N2-B06",
                        minimum=1,
                        maximum=_DIE_SIDES,
                    )
                )
        start = rolled.start()
        start += text[start:].index("サイコロ")
        die = Span(start=start, end=start + len("サイコロ"))
        result.append(
            leaf(
                "Concept",
                "rule_term",
                "concept.rule_term.v1",
                GlossaryReference(kind="glossary", key=term.id),
                ((die,),),
                "N2-B06",
            )
        )
    return result


def _sources(raw: str, part: SourcePart, classifier: Classifier) -> list[Leaf]:
    text = part.canonical_source
    result: list[Leaf] = []
    if part.source_span.role == "body":
        return _earth(raw, part, classifier) + _dice(raw, part, classifier)
    if part.source_span.role == "token_header" and (match := _HEADER.fullmatch(text)):
        code = "crest" if match["kind"] == "クレスト" else "equipment"
        domain = classifier.domains["card_kind.any.v1"]
        value = VocabularyReference(kind="vocabulary", key=("type", code))
        if value in getattr(domain, "references", ()):
            span = Span(start=match.start("kind"), end=match.end("kind"))
            # Other headers keep their declared kind spelling in canonical source.
            result.append(
                replace(
                    leaf(
                        "CardKind",
                        "declared_kind",
                        "card_kind.any.v1",
                        value,
                        ((span,),),
                        "N2-B15",
                    ),
                    abstract=False,
                )
            )
    elif part.source_span.role == "reminder" and (match := _REMINDER.fullmatch(text)):
        for name in ("attack", "health"):
            span = Span(start=match.start(name), end=match.end(name))
            result.append(_nat("stat_value", _number(raw, part, span), span, "N2-B16"))
    return result


def _project(original: SourcePart, current: SourcePart, span: Span) -> tuple[Span, ...]:
    raw = _raw_spans(original, span)
    return merged(
        tuple(
            Span(start=i, end=i + 1)
            for i, u in enumerate(current.units)
            if any(a.start < b.end and b.start < a.end for a in u.origins for b in raw)
        )
    )


def _use(found: Recognized, name: str) -> int:
    index = next(i for i, o in enumerate(found.occurrences) if o.slot == name)
    # N1 omitted owners precede their explicit zone or position anchor in source order.
    return next(o.raw_spans[0].start for o in found.occurrences[index:] if o.raw_spans)


def _update(
    raw: str,
    source: SourceDescriptor,
    original: SourcePart,
    current: SourcePart,
    found: Recognized,
    classifier: Classifier,
) -> tuple[SourcePart, Recognized, tuple[str, ...]] | None:
    additions = _sources(raw, original, classifier)
    if not additions:
        return None
    projected = [
        replace(
            item,
            positions=tuple(
                _project(original, current, span)
                for group in item.positions
                for span in group
            ),
            slot=item.slot.model_copy(
                update={
                    "occurrences": tuple(
                        s
                        for group in item.positions
                        for span in group
                        for s in _project(original, current, span)
                    )
                }
            ),
        )
        for item in additions
    ]
    repaired = tuple(s for item in projected for s in item.slot.occurrences)
    leaves = []
    for slot in found.schema.slots:
        if any(
            a.start < b.end and b.start < a.end
            for a in slot.occurrences
            for b in repaired
        ):
            continue
        occurrences = tuple(o for o in found.occurrences if o.slot == slot.name)
        leaves.append(
            Leaf(
                slot,
                found.values[slot.name],
                tuple(o.canonical_spans for o in occurrences if o.canonical_spans),
                "N1",
                any(
                    current.canonical_source[s.start : s.end].startswith("{leaf_")
                    for s in slot.occurrences
                ),
                occurrences[0].source_unit,
                occurrences[0].resolution_rule,
                _use(found, slot.name),
            )
        )
    leaves.extend(projected)
    covered = tuple(
        s
        for item in leaves
        for group in item.positions
        for span in group
        for s in _raw_spans(current, span)
    )
    candidate = classifier.candidate(raw, original)
    issues = set(found.issues)
    if all(
        h.type != "uint"
        or all(
            any(a.start <= b.start and b.end <= a.end for a in covered)
            for b in h.source_segments
        )
        for h in candidate.slots
    ):
        issues.difference_update(
            {
                "n0_numeric_construction_unresolved",
                "numeric_role_requires_review",
                "numeric_identifier_requires_review",
            }
        )
    if original.source_span.role == "token_header":
        # Only the final bare kind is repaired; unrelated ambiguous hints stay pending.
        kinds = tuple(
            s
            for item in additions
            for group in item.positions
            for span in group
            for s in _raw_spans(original, span)
        )
        if all(
            not h.issues
            or all(
                any(a.start <= b.start and b.end <= a.end for a in kinds)
                for b in h.source_segments
            )
            for h in candidate.slots
        ):
            issues.discard("unknown_or_ambiguous_vocabulary")
    updated, result, rows = transform(
        raw,
        current,
        leaves,
        (),
        replace(found, issues=tuple(sorted(issues)), semantics=None),
    )
    if original.source_span.role == "token_header" and not result.issues:
        # A completed bare kind is the same registered construction as other headers.
        result = replace(
            result,
            semantics=classify_semantics(
                source, updated, result.schema, classifier.terms, None, None
            ),
        )
    return updated, result, rows


def prepare_parameters(
    raw: str, derived: DerivedField, classifier: Classifier
) -> DerivedField:
    """Apply only the sealed source shapes, preserving all unrelated N1 evidence."""
    if derived.field.source.field not in {"effect", "section"}:
        return derived
    parts = list(derived.field.parts)
    recognized = list(derived.recognized)
    rows = list(derived.registry_rows)
    for i, original in enumerate(derived.original.parts):
        if update := _update(
            raw, derived.field.source, original, parts[i], recognized[i], classifier
        ):
            parts[i], recognized[i], rows[i] = update
    return replace(
        derived,
        field=SourceField(derived.field.source, tuple(parts)),
        recognized=tuple(recognized),
        registry_rows=tuple(rows),
    )

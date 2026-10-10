"""Source slot extraction with exact positions, safe values and unmatched reasons."""

import re
import unicodedata
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import Span
from sve_carddb.domains.translations.four_layer_grammar import TOKEN_HEADER, TYPE_WORDS
from sve_carddb.domains.translations.recognition.lexical import DIGITS
from sve_carddb.domains.translations.recognition.models import (
    Candidate,
    Hint,
    LiteralTrace,
    NumericRule,
)
from sve_carddb.domains.translations.recognition.numeric_rules import (
    ASCII_AFTER,
    ASCII_BEFORE,
    NUMERIC_PREFIX,
    NUMERIC_RULE_DISABLED,
    NUMERIC_RULES,
    NUMERIC_SUFFIX,
    RESOURCE_PREFIX_EXCEPTION,
    SIGNS,
    excluded,
)
from sve_carddb.domains.translations.recognition.provenance import Unit, merged
from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.parse.pages.extract_jp import _traits as parse_traits

if TYPE_CHECKING:
    from sve_carddb.domains.translations.recognition.lexical import Part
    from sve_carddb.domains.translations.recognition.references import References

__all__ = ("NUMERIC_PREFIX", "NUMERIC_RULES", "NUMERIC_RULE_DISABLED", "NUMERIC_SUFFIX")

SAFE_INTEGER = 9007199254740991
BRACED = re.compile(r"\{([^{}]+)\}")


@dataclass(frozen=True)
class Position:
    start: int
    end: int
    transformation: str
    semantic_role: str


def prepared(text: str, part: Part) -> tuple[Part, tuple[Unit, ...]]:
    """The source core's immutable Unicode trace is the only normalization authority."""
    if "".join(unit.text for unit in part.units) != part.normalized:
        raise ValueError("Lexical trace differs from the source core")
    if any(s.end > len(text) for u in part.units for s in u.origins):
        raise ValueError("Lexical trace lies outside the exact owner field")
    return part, part.units


def positions(
    text: str, part: Part, units: tuple[Unit, ...], refs: References
) -> tuple[Position, ...]:
    """Identify replacements from provenance; header roles come only from named grammar groups."""
    if part.role == "layout":
        return (Position(0, len(units), "whitespace", "layout"),)
    if part.role == "token_header":
        return header_positions(part.normalized)
    result = [
        Position(
            index,
            index + 1,
            unit.transformation,
            "numeric" if unit.transformation == "digits" else "quoted_reference",
        )
        for index, unit in enumerate(units)
        if unit.transformation != "literal"
    ]
    if part.role == "reminder":
        quoted = tuple(re.finditer(r"『[^』]+』", part.normalized))
        result.extend(
            Position(m.start() + 1, m.end() - 1, "quoted", "quoted_reference")
            for m in quoted
        )
        result.extend(
            Position(m.start(), m.end(), "digits", "numeric")
            for m in DIGITS.finditer(part.normalized)
            if not any(q.start() <= m.start() < q.end() for q in quoted)
        )
    occupied = {i for item in result for i in range(item.start, item.end)}
    for match in BRACED.finditer(part.normalized):
        if any(i in occupied for i in range(match.start(1), match.end(1))):
            continue
        origins = merged(
            tuple(s for u in units[match.start(1) : match.end(1)] for s in u.origins)
        )
        raw = "".join(text[s.start : s.end] for s in origins)
        found = {
            b.kind
            for b in (() if refs.vocabulary is None else refs.vocabulary.bindings)
            if b.region == "jp" and b.raw == raw and b.kind in {"class", "type"}
        }
        if len(found) == 1:
            result.append(
                Position(match.start(1), match.end(1), "braced", next(iter(found)))
            )
        elif raw in refs.terms:
            result.append(Position(match.start(1), match.end(1), "braced", "term"))
    return tuple(sorted(result, key=lambda item: item.start))


def header_positions(normalized: str) -> tuple[Position, ...]:
    """Separate declared traits from the trailing base type; reuse embedded-separator exceptions."""
    match = TOKEN_HEADER.fullmatch(normalized)
    if match is None:
        raise ValueError(
            "Token header candidate must match the complete source header grammar"
        )
    groups = {
        "name": "card_name",
        "cls": "class",
        "cost": "cost",
        "atk": "attack",
        "hp": "health",
    }
    result = [
        Position(match.start(g), match.end(g), "header", role)
        for g, role in groups.items()
        if match[g] is not None and match.start(g) < match.end(g)
    ]
    suffix = re.search("(?:" + TYPE_WORDS + ")$", match["kind"])
    assert suffix is not None
    start = match.start("kind")
    result.append(Position(start + suffix.start(), match.end("kind"), "header", "type"))
    prefix = match["kind"][: suffix.start()]
    if prefix:
        try:
            traits = parse_traits(prefix[:-1]) if prefix.endswith("・") else []
        except ValidationError:
            traits = []
        if not traits:
            result.append(
                Position(start, start + len(prefix), "header", "trait_unclassified")
            )
        for trait in traits:
            result.append(Position(start, start + len(trait), "header", "trait"))
            start += len(trait) + 1
    return tuple(sorted(result, key=lambda p: p.start))


def unsigned(raw: str) -> int | None:
    """Compatibility numerals outside decimal ASCII/fullwidth digits are not authorized values."""
    if DIGITS.fullmatch(raw) is None:
        return None
    value = int(unicodedata.normalize("NFKC", raw))
    return value if value <= SAFE_INTEGER else None


def numeric_role(
    normalized: str, position: Position
) -> tuple[NumericRule | None, tuple[str, ...]]:
    """Unit/prefix grammar excludes signs, ASCII identifiers and undecided bare numbers."""
    before = normalized[: position.start]
    after = normalized[position.end :]
    if before.endswith(SIGNS):
        return None, ("signed_numeric_requires_review",)
    if (
        (before and re.search(ASCII_BEFORE, before)) or re.match(ASCII_AFTER, after)
    ) and not (
        NUMERIC_PREFIX.search(before) or after.startswith(RESOURCE_PREFIX_EXCEPTION)
    ):
        return None, ("numeric_identifier_requires_review",)
    if (reason := excluded(after)) is not None:
        return None, (reason,)
    # A suffix wins when both grammars match, so each position counts exactly once.
    match = NUMERIC_SUFFIX.match(after) or NUMERIC_PREFIX.search(before)
    if match is not None:
        rule = next(rule for rule in NUMERIC_RULES if rule == match.lastgroup)
        return rule, (NUMERIC_RULE_DISABLED,)
    return None, ("numeric_role_requires_review",)


def hint(
    text: str,
    part: Part,
    units: tuple[Unit, ...],
    position: Position,
    index: int,
    refs: References,
) -> Hint:
    """Return source positions, safe values and exact reference targets."""
    selected = units[position.start : position.end]
    origins = merged(tuple(s for unit in selected for s in unit.origins))
    raw = "".join(text[s.start : s.end] for s in origins)
    common = {
        "name": f"slot_{index}",
        "occurrence": Span(start=position.start, end=position.end),
        "source_segments": origins,
        "transformation": position.transformation,
        "semantic_role": position.semantic_role,
        "numeric_rule": None,
    }
    if position.semantic_role == "layout":
        return Hint.model_validate(
            dict(
                common,
                type="literal",
                reference_kind=None,
                value=None,
                target=None,
                issues=() if raw.isspace() else ("literal_requires_source_whitespace",),
            )
        )
    if position.semantic_role in {"numeric", "cost", "attack", "health"}:
        value = unsigned(raw)
        issues: tuple[str, ...] = (
            ("invalid_safe_unsigned_decimal",) if value is None else ()
        )
        if position.semantic_role == "numeric":
            rule, causes = numeric_role(part.normalized, position)
            common["numeric_rule"] = rule
            issues += causes
        return Hint.model_validate(
            dict(
                common,
                type="uint",
                reference_kind=None,
                value=value,
                target=None,
                issues=issues,
            )
        )
    if position.semantic_role in {"class", "type"}:
        resolution = refs.proposed_vocabulary(position.semantic_role, raw)
    elif position.semantic_role == "term":
        resolution = refs.braced_term(raw)
    elif position.semantic_role == "trait":
        resolution = refs.header_trait(raw)
    elif position.semantic_role == "trait_unclassified":
        resolution = refs.unclassified_header()
    else:
        resolution = refs.quoted(raw)
    kind = None if resolution.target is None else resolution.target["kind"]
    return Hint.model_validate(
        dict(
            common,
            type="reference",
            reference_kind=kind,
            value=None,
            target=resolution.target,
            issues=resolution.issues,
        )
    )


def literals(
    units: tuple[Unit, ...], hints: tuple[Hint, ...]
) -> tuple[LiteralTrace, ...]:
    """Fixed text is provenance, never a literal parameter that bypasses translation."""
    occupied = {i for h in hints for i in range(h.occurrence.start, h.occurrence.end)}
    result = []
    start = 0
    while start < len(units):
        if start in occupied:
            start += 1
            continue
        end = start + 1
        while end < len(units) and end not in occupied:
            end += 1
        selected = units[start:end]
        origins = merged(tuple(s for u in selected for s in u.origins))
        result.append(
            LiteralTrace(
                occurrence=Span(start=start, end=end),
                source_segments=origins,
            )
        )
        start = end
    return tuple(result)


def analyze(
    text: str,
    part: Part,
    refs: References,
    *,
    units: tuple[Unit, ...] | None = None,
) -> Candidate:
    """Extract source slots before applying the enabled contextual rules."""
    template_part, units = prepared(text, part) if units is None else (part, units)
    hints = tuple(
        hint(text, template_part, units, position, index, refs)
        for index, position in enumerate(positions(text, template_part, units, refs))
    )
    issues = tuple(sorted({reason for h in hints for reason in h.issues}))
    if part.role == "reminder":
        issues += ("legacy_parenthesis_classification_requires_review",)
    if part.role == "token_header":
        header = TOKEN_HEADER.fullmatch(part.normalized)
        assert header is not None
        if any(
            header[group] is not None and not header[group]
            for group in ("cost", "atk", "hp")
        ):
            issues += ("header_empty_numeric_requires_review",)
    return Candidate(
        slots=hints,
        literal_trace=literals(units, hints),
        issues=issues,
    )

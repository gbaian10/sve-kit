"""Source slot extraction with exact positions, safe values and unmatched reasons."""

import re
import unicodedata
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.template_parameters import Range, Schema, Slot
from sve_carddb.core.json import canonical, digest
from sve_carddb.extract.official_jp import _traits as parse_traits
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.template_parameters.models import (
    Candidate,
    Hint,
    LiteralTrace,
    NumericRule,
)
from sve_carddb.template_parameters.numeric_rules import (
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
from sve_carddb.template_parameters.provenance import Unit, merged, trace
from sve_carddb.template_sources.normalizer import (
    DIGITS,
    TOKEN_HEADER,
    TYPE_WORDS,
    VERSION,
)

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.template_parameters.references import References
    from sve_carddb.template_parameters.spans import Located
    from sve_carddb.template_sources.models import Entry
    from sve_carddb.template_sources.normalizer import Part

__all__ = ("NUMERIC_PREFIX", "NUMERIC_RULES", "NUMERIC_RULE_DISABLED", "NUMERIC_SUFFIX")

VERSION_PARAMETERS = "template-parameters-jp-candidate-v1"
SAFE_INTEGER = 9007199254740991
BRACED = re.compile(r"\{([^{}]+)\}")


@dataclass(frozen=True)
class Position:
    start: int
    end: int
    transformation: str
    semantic_role: str


def prepared(text: str, part: Part) -> tuple[Part, tuple[Unit, ...]]:
    """Keep legacy inventory hashes intact; one new fixed layout payload has a whitespace parameter."""
    if part.role == "layout":
        origins = tuple(Range(start=s.start, end=s.end) for s in part.segments)
        raw = "".join(text[s.start : s.end] for s in origins)
        if not raw.isspace():
            raise ValueError(
                "Fixed layout recipe requires exact nonempty source whitespace"
            )
        return replace(part, normalized="W"), (Unit("W", origins),)
    return part, trace(text, part)


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
            "Token header candidate must match the complete legacy header grammar"
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
        "occurrence": Range(start=position.start, end=position.end),
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


def schema(hints: tuple[Hint, ...]) -> Schema | None:
    """A disabled rule or composite reference cannot masquerade as a complete slot schema."""
    if any(item.issues or item.type is None for item in hints):
        return None
    return Schema(
        slots=tuple(
            Slot.model_validate(
                {
                    "name": h.name,
                    "type": h.type,
                    "occurrences": (h.occurrence,),
                    "reference_kind": h.reference_kind,
                    "min": 0 if h.type == "uint" else None,
                    "max": SAFE_INTEGER if h.type == "uint" else None,
                }
            )
            for h in hints
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
                occurrence=Range(start=start, end=end),
                source_segments=origins,
            )
        )
        start = end
    return tuple(result)


def contract(
    normalized: str, hints: tuple[Hint, ...]
) -> tuple[Schema | None, str, str | None]:
    """Derive diagnostic identities from the final classified slots."""
    shape: JsonValue = [
        [
            h.occurrence.model_dump(mode="json"),
            h.type,
            h.reference_kind,
            h.semantic_role,
            h.numeric_rule,
            list(h.issues),
        ]
        for h in hints
    ]
    parameter_schema = schema(hints)
    payload_hash = None
    if parameter_schema is not None:
        payload_hash = digest(
            canonical(
                {
                    "level": "sentence",
                    "source_lang": "ja",
                    "normalized_text": normalized,
                    "normalizer_version": VERSION_PARAMETERS,
                    "semantic_variant": "default",
                    "parameter_schema": parameter_schema.model_dump(mode="json"),
                }
            )
        )
    return parameter_schema, digest(canonical(shape)), payload_hash


def analyze(
    text: str, part: Part, item: Entry, located: Located, refs: References
) -> Candidate:
    """Extract source slots before applying the enabled contextual rules."""
    template_part, units = prepared(text, part)
    hints = tuple(
        hint(text, template_part, units, position, index, refs)
        for index, position in enumerate(positions(text, template_part, units, refs))
    )
    parameter_schema, signature_hash, payload_hash = contract(
        template_part.normalized, hints
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
        inventory_id=item.id,
        ordinal=located.ordinal,
        line_ordinal=located.line_ordinal,
        source_span=located.source_span,
        normalizer_id=VERSION,
        normalized_hash=part.normalized_hash,
        parameter_normalizer_id=VERSION_PARAMETERS,
        template_normalized_hash=template_part.normalized_hash,
        parameter_schema=parameter_schema,
        slots=hints,
        literal_trace=literals(units, hints),
        issues=issues,
        signature_hash=signature_hash,
        payload_hash=payload_hash,
    )

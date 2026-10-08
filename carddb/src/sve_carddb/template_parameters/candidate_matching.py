"""Classify enabled rules directly from source values and surrounding grammar."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.template_parameter_rules.models import LEGACY_IDS
from sve_carddb.template_parameters.analysis import (
    analyze,
    contract,
    prepared,
    unsigned,
)
from sve_carddb.template_parameters.explicit_rules import EXPLICIT
from sve_carddb.template_parameters.keyword_aliases import KEYWORD_ALIASES
from sve_carddb.template_parameters.models import Range
from sve_carddb.template_parameters.numeric_rules import (
    ASCII_AFTER,
    ASCII_BEFORE,
    SIGNS,
)
from sve_carddb.template_parameters.provenance import merged
from sve_carddb.template_parameters.rule_candidates import (
    BY_ID,
    INTRO_PATTERN,
    KEYWORD_PATTERN,
    LABEL_PATTERN,
    MIN_OPTIONS,
    SIGNED,
    STAT_CATEGORIES,
    SUFFIXES,
    VERSION,
    selection,
)
from sve_carddb.template_parameters.signed_contexts import SIGNED_CONTEXTS
from sve_carddb.template_sources.normalizer import QUOTED, partition

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.template_parameters.models import Candidate, Hint
    from sve_carddb.template_parameters.provenance import Unit
    from sve_carddb.template_parameters.references import References
    from sve_carddb.template_parameters.rule_candidates import Rule
    from sve_carddb.template_parameters.spans import Located
    from sve_carddb.template_sources.models import Entry
    from sve_carddb.template_sources.normalizer import Part

INTRO = re.compile(INTRO_PATTERN)
LABEL = re.compile(LABEL_PATTERN)
ASCII = re.compile(ASCII_AFTER)


@dataclass(frozen=True)
class Match:
    context: tuple[Range, ...] = ()
    target_id: str | None = None


def _raw(text: str, spans: tuple[Range, ...]) -> str:
    return "".join(text[s.start : s.end] for s in spans)


def _exact_target(raw: str, refs: References, targets: tuple[str, ...]) -> str | None:
    found = refs.terms.get(raw, [])
    if len(found) != 1 or found[0][0] not in targets:
        return None
    identifier, category = found[0]
    expected = STAT_CATEGORIES.get(identifier, "ability")
    return identifier if category == expected else None


def _origins(units: tuple[Unit, ...], start: int, end: int) -> tuple[Range, ...]:
    return merged(tuple(s for u in units[start:end] for s in u.origins))


def _choice(text: str, hint: Hint) -> Match | None:
    parts = partition(text)
    body = tuple(s for p in parts if p.role == "body" for s in p.segments)
    quoted = tuple(QUOTED.finditer(text))

    def in_body(start: int, end: int) -> bool:
        return any(s.start <= start < end <= s.end for s in body) and not any(
            q.start() < end and start < q.end() for q in quoted
        )

    introductions = [m for m in INTRO.finditer(text) if in_body(m.start(), m.end())]
    labels = [m for m in LABEL.finditer(text) if in_body(m.start(), m.end())]
    for n, introduction in enumerate(introductions):
        end = introductions[n + 1].start() if n + 1 < len(introductions) else len(text)
        group = [m for m in labels if introduction.end() <= m.start() < end]
        if len(group) < MIN_OPTIONS or [unsigned(m[1]) for m in group] != list(
            range(1, len(group) + 1)
        ):
            continue
        if any(
            hint.source_segments == (Range(start=m.start(1), end=m.end(1)),)
            for m in group
        ):
            return Match(
                context=(
                    Range(start=introduction.start(), end=introduction.end()),
                    *(Range(start=m.start(), end=m.end()) for m in group),
                )
            )
    return None


def _braced(
    rule: Rule, text: str, hint: Hint, refs: References, before: str, after: str
) -> Match | None:
    if not (
        before.endswith("{")
        and after.startswith("}")
        and before.count("{") - before.count("}") == 1
    ):
        return None
    target = _exact_target(_raw(text, hint.source_segments), refs, rule.targets)
    if target is None or hint.target != {
        "kind": "term",
        "id": target,
    }:
        return None
    return Match(target_id=target)


def _suffix(rule: Rule, hint: Hint, before: str, after: str) -> Match | None:
    if before.endswith(SIGNS) or (before and re.search(ASCII_BEFORE, before)):
        return None
    if hint.value == 0 and rule.id.startswith("suffix_ordinal_"):
        return None
    return Match() if re.match(SUFFIXES[rule.id], after) else None


def _signed(
    rule: Rule,
    text: str,
    refs: References,
    units: tuple[Unit, ...],
    before: str,
    after: str,
) -> Match | None:
    match = re.search(SIGNED[rule.id], before)
    if match is None or ASCII.match(after):
        return None
    context = _origins(units, match.start(), match.end())
    if not rule.targets:
        return Match(context=context)
    spans = _origins(units, match.start() + 1, match.end() - len("}+"))
    target = _exact_target(_raw(text, spans), refs, rule.targets)
    return None if target is None else Match(context=context, target_id=target)


def _signed_context(
    rule: Rule,
    text: str,
    hint: Hint,
    refs: References,
    units: tuple[Unit, ...],
    edges: tuple[str, str],
) -> Match | None:
    before, after = edges
    spec = SIGNED_CONTEXTS[rule.id]
    prefix = re.search(spec.before, before)
    suffix = re.match(spec.after, after)
    if prefix is None or suffix is None:
        return None
    target = None
    if spec.target is not None:
        spans = _origins(units, prefix.start("term"), prefix.end("term"))
        target = _exact_target(_raw(text, spans), refs, (spec.target,))
        if target is None:
            return None
    return Match(
        context=_origins(units, prefix.start(), hint.occurrence.end + suffix.end()),
        target_id=target or None,
    )


def _explicit_context(
    rule: Rule, text: str, hint: Hint, units: tuple[Unit, ...], edges: tuple[str, str]
) -> Match | None:
    before, after = edges
    spec = EXPLICIT[rule.id]
    prefix = re.search(spec.before, before)
    suffix = re.match(spec.after, after)
    if (
        prefix is None
        or suffix is None
        or hint.value is None
        or hint.value < spec.minimum
    ):
        return None
    if spec.companion is not None:
        match = prefix if spec.companion == "prefix" else suffix
        offset = 0 if spec.companion == "prefix" else hint.occurrence.end
        spans = _origins(
            units, offset + match.start("companion"), offset + match.end("companion")
        )
        if unsigned(_raw(text, spans)) is None:
            return None
    return Match(
        context=_origins(units, prefix.start(), hint.occurrence.end + suffix.end())
    )


def _alias_threshold(
    rule: Rule,
    text: str,
    hint: Hint,
    refs: References,
    units: tuple[Unit, ...],
    edges: tuple[str, str],
) -> Match | None:
    before, after = edges
    alias = KEYWORD_ALIASES[rule.id]
    head = "【" + alias.spelling + "_"
    start = len(before) - len(head)
    if not before.endswith(head) or not after.startswith("】"):
        return None
    if _raw(text, _origins(units, start, len(before))) != head:
        return None
    target = _exact_target(alias.full_name, refs, (alias.target,))
    if target is None:
        return None
    return Match(
        context=_origins(units, start, hint.occurrence.end + 1),
        target_id=target,
    )


def _threshold(
    rule: Rule,
    text: str,
    hint: Hint,
    refs: References,
    units: tuple[Unit, ...],
    edges: tuple[str, str],
) -> Match | None:
    if rule.id in KEYWORD_ALIASES:
        return _alias_threshold(rule, text, hint, refs, units, edges)
    before, after = edges
    match = re.search(KEYWORD_PATTERN, before)
    if match is None or not after.startswith("】"):
        return None
    spans = _origins(units, match.start(1), match.end(1))
    target = _exact_target(_raw(text, spans), refs, rule.targets)
    return (
        None
        if target is None
        else Match(
            context=_origins(units, match.start(), hint.occurrence.end + 1),
            target_id=target,
        )
    )


def _match(
    rule: Rule,
    text: str,
    traced: tuple[Part, tuple[Unit, ...]],
    hint: Hint,
    refs: References,
) -> Match | None:
    template, units = traced
    before = template.normalized[: hint.occurrence.start]
    after = template.normalized[hint.occurrence.end :]
    if rule.id.startswith("braced_"):
        return _braced(rule, text, hint, refs, before, after)
    if unsigned(_raw(text, hint.source_segments)) != hint.value or hint.value is None:
        return None
    return _numeric_match(rule, text, hint, refs, units, (before, after))


def _numeric_match(
    rule: Rule,
    text: str,
    hint: Hint,
    refs: References,
    units: tuple[Unit, ...],
    edges: tuple[str, str],
) -> Match | None:
    before, after = edges
    if rule.id in EXPLICIT:
        return _explicit_context(rule, text, hint, units, edges)
    if rule.id in SUFFIXES:
        return _suffix(rule, hint, before, after)
    if rule.id in SIGNED or rule.id in SIGNED_CONTEXTS:
        return (
            _signed_context(rule, text, hint, refs, units, edges)
            if rule.id in SIGNED_CONTEXTS
            else _signed(rule, text, refs, units, before, after)
        )
    if rule.id.startswith("keyword_threshold_") or rule.id in KEYWORD_ALIASES:
        return _threshold(rule, text, hint, refs, units, (before, after))
    choice = (
        rule.id == "bracket_choice_index"
        and before.endswith("【")
        and after.startswith("】")
    )
    return _choice(text, hint) if choice else None


def recognize(
    text: str,
    part: Part,
    candidate: Candidate,
    refs: References,
    enabled: tuple[str, ...] = (),
) -> tuple[dict[str, JsonValue], ...]:
    """Return enabled lexical matches with roles, types and exact positions."""
    selected = selection(enabled)
    if part.role not in {"body", "reminder"}:
        return ()
    results: list[dict[str, JsonValue]] = []
    traced = None
    for hint in candidate.slots:
        if (
            hint.numeric_rule is not None
            or "invalid_safe_unsigned_decimal" in hint.issues
        ):
            continue
        for identifier in selected:
            rule = BY_ID[identifier]
            if rule.reason not in hint.issues:
                continue
            # All rule attempts share the same immutable normalization provenance.
            if traced is None:
                traced = prepared(text, part)
            match = _match(rule, text, traced, hint, refs)
            if match is None:
                continue
            if any(r["slot"] == hint.name for r in results):
                raise ValueError("Candidate matchers must not share slot ownership")
            results.append(
                {
                    "inventory_id": candidate.inventory_id,
                    "slot": hint.name,
                    "type": hint.type,
                    "reference_kind": hint.reference_kind,
                    "rule_id": identifier,
                    "matcher_version": VERSION + ":" + identifier,
                    "recognized_role": rule.role,
                    "role": part.role,
                    "normalized_occurrence": hint.occurrence.model_dump(mode="json"),
                    "source_segments": [
                        s.model_dump(mode="json") for s in hint.source_segments
                    ],
                    "value": hint.value,
                    "target_id": match.target_id,
                    "context_segments": [
                        s.model_dump(mode="json") for s in match.context
                    ],
                }
            )
    return tuple(results)


def classify(
    text: str,
    part: Part,
    item: Entry,
    located: Located,
    refs: References,
    enabled: tuple[str, ...] = (),
) -> tuple[Candidate, tuple[dict[str, JsonValue], ...]]:
    """Classify once; disabled or unmatched positions retain their failure reasons."""
    candidate = analyze(text, part, item, located, refs)
    rows = recognize(
        text, part, candidate, refs, tuple(k for k in enabled if k not in LEGACY_IDS)
    )
    matched = {str(row["slot"]): row for row in rows}
    hints = []
    for original in candidate.slots:
        hint = original
        row = matched.get(hint.name)
        if row is not None:
            hint = hint.model_copy(
                update={
                    "issues": tuple(
                        reason
                        for reason in hint.issues
                        if reason != BY_ID[str(row["rule_id"])].reason
                    ),
                    "semantic_role": str(row["recognized_role"]),
                    "rule_id": str(row["rule_id"]),
                }
            )
        elif (
            hint.numeric_rule in enabled
            and part.role in {"body", "reminder"}
            and hint.value is not None
        ):
            hint = hint.model_copy(update={"issues": (), "rule_id": hint.numeric_rule})
        hints.append(hint)
    field_issues = set(candidate.issues) - {
        reason for hint in candidate.slots for reason in hint.issues
    }
    issues = tuple(
        sorted(field_issues | {reason for hint in hints for reason in hint.issues})
    )
    parameter_schema, signature_hash, payload_hash = contract(
        prepared(text, part)[0].normalized, tuple(hints)
    )
    return candidate.model_copy(
        update={
            "slots": tuple(hints),
            "issues": issues,
            "parameter_schema": parameter_schema,
            "signature_hash": signature_hash,
            "payload_hash": payload_hash,
        }
    ), rows

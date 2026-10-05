"""Match only unowned pending slots and preserve their original candidate payloads."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import digest
from sve_carddb.template_parameters.analysis import prepared, unsigned
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
    condition_hash,
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
    from sve_carddb.template_sources.normalizer import Part

INTRO = re.compile(INTRO_PATTERN)
LABEL = re.compile(LABEL_PATTERN)
ASCII = re.compile(ASCII_AFTER)


@dataclass(frozen=True)
class Match:
    context: tuple[Range, ...] = ()
    target_id: str | None = None
    target_hash: str | None = None


def _raw(text: str, spans: tuple[Range, ...]) -> str:
    return "".join(text[s.start : s.end] for s in spans)


def _exact_target(
    raw: str, refs: References, targets: tuple[str, ...]
) -> tuple[str, str] | None:
    found = refs.terms.get(raw, [])
    if len(found) != 1 or found[0][0] not in targets:
        return None
    identifier, category, checksum = found[0]
    expected = STAT_CATEGORIES.get(identifier, "ability")
    return (identifier, checksum) if category == expected else None


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
        "id": target[0],
        "record_hash": target[1],
    }:
        return None
    return Match(target_id=target[0], target_hash=target[1])


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
    return (
        None
        if target is None
        else Match(context=context, target_id=target[0], target_hash=target[1])
    )


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
        target_id=target[0] if target else None,
        target_hash=target[1] if target else None,
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
        target_id=target[0],
        target_hash=target[1],
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
            target_id=target[0],
            target_hash=target[1],
        )
    )


def _match(
    rule: Rule, text: str, part: Part, hint: Hint, refs: References
) -> Match | None:
    template, units = prepared(text, part)
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
    """An opt-in match stays pending; it cannot rewrite issues, schema or old ownership."""
    selected = selection(enabled)
    if part.role not in {"body", "reminder"}:
        return ()
    results: list[dict[str, JsonValue]] = []
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
            match = _match(rule, text, part, hint, refs)
            if match is None:
                continue
            if any(r["slot"] == hint.name for r in results):
                raise ValueError("Candidate matchers must not share slot ownership")
            results.append(
                {
                    "inventory_id": candidate.inventory_id,
                    "slot": hint.name,
                    "rule_id": identifier,
                    "matcher_version": VERSION + ":" + identifier,
                    "condition_hash": condition_hash(rule),
                    "proposed_role": rule.role,
                    "original_reason": rule.reason,
                    "status": "pending_approval",
                    "role": part.role,
                    "normalized_occurrence": hint.occurrence.model_dump(mode="json"),
                    "source_segments": [
                        s.model_dump(mode="json") for s in hint.source_segments
                    ],
                    "raw_hash": hint.raw_hash,
                    "value": hint.value,
                    "target_id": match.target_id,
                    "target_hash": match.target_hash,
                    "context_segments": [
                        s.model_dump(mode="json") for s in match.context
                    ],
                    "context_hash": digest(_raw(text, match.context).encode()),
                }
            )
    return tuple(results)

"""Whole-batch parameter candidate replay and summary counts."""

from collections import Counter, defaultdict
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import array, digest, object_value
from sve_carddb.domains.translations.parameters.analysis import (
    NUMERIC_RULE_DISABLED,
    NUMERIC_RULES,
)
from sve_carddb.domains.translations.parameters.candidate_matching import classify
from sve_carddb.domains.translations.parameters.numeric_rules import configuration
from sve_carddb.domains.translations.parameters.rule_candidates import BY_ID
from sve_carddb.domains.translations.parameters.rules import LEGACY_IDS
from sve_carddb.domains.translations.parameters.spans import locate
from sve_carddb.domains.translations.source_inventory.inventory import (
    entry,
    fields,
    replay,
)
from sve_carddb.domains.translations.source_inventory.normalizer import (
    VERSION,
    partition,
)
from sve_carddb.domains.translations.source_inventory.pins import PARSER
from sve_carddb.domains.translations.sources import project

if TYPE_CHECKING:
    from sve_carddb.domains.translations.parameters.models import Candidate
    from sve_carddb.domains.translations.parameters.references import References
    from sve_carddb.domains.translations.parameters.spans import Located
    from sve_carddb.domains.translations.source_inventory.inventory import Scan
    from sve_carddb.domains.translations.source_inventory.models import Entry
    from sve_carddb.domains.translations.source_inventory.normalizer import Part
    from sve_carddb.ingest.archive.frozen_sources import FrozenSources


@dataclass
class Candidates:
    entries: list[Candidate] = field(default_factory=list)
    mentions: list[dict[str, JsonValue]] = field(default_factory=list)
    field_proofs: list[dict[str, JsonValue]] = field(default_factory=list)
    rule_matches: list[dict[str, JsonValue]] = field(default_factory=list)
    enabled_rules: tuple[str, ...] = ()


@dataclass(frozen=True)
class Field:
    source_id: str
    locator: str
    text: str
    section: int | None
    entries: list[Entry]
    document: JsonValue


def build(
    sources: FrozenSources,
    scan: Scan,
    refs: References,
    *,
    enabled_rules: tuple[str, ...] = (),
) -> Candidates:
    """Every first-checkpoint entry must reappear; null fields are never coerced to empty."""
    if len(enabled_rules) != len(set(enabled_rules)):
        raise ValueError("Candidate rule selection must be unique")
    if any(rule not in BY_ID and rule not in LEGACY_IDS for rule in enabled_rules):
        raise ValueError("Candidate rule selection contains an unknown rule")
    result = Candidates(enabled_rules=tuple(sorted(enabled_rules)))
    by_field: dict[tuple[str, str], list[Entry]] = defaultdict(list)
    for item in scan.entries:
        by_field[item.source_ref.source_version_id, item.source_ref.locator].append(
            item
        )
    for current in sources.inventory.current:
        source, raw, _ = sources.read(current.source_version_id, parser_version=PARSER)
        if source.id in scan.documents:
            document = scan.documents[source.id]
        else:
            _, document = project(raw, source.url, "jp")
        for locator, text, section in fields(document):
            if text is None:
                continue
            _field(
                result,
                Field(
                    source.id,
                    locator,
                    text,
                    section,
                    by_field[source.id, locator],
                    document,
                ),
                refs,
            )
    _coverage(result, scan)
    return result


def _field(result: Candidates, item: Field, refs: References) -> None:
    parts = partition(item.text, section=item.section)
    located = locate(item.text, parts)
    result.field_proofs.append(
        {
            "source_version_id": item.source_id,
            "locator": item.locator,
            "text_hash": digest(item.text.encode()),
            "bindings": len(located),
            "utf8_roundtrip": True,
        }
    )
    for part, position in zip(parts, located, strict=True):
        _candidate(result, item, part, position, refs)


def _candidate(
    result: Candidates, context: Field, part: Part, position: Located, refs: References
) -> None:
    matches = [
        e for e in context.entries if entry(e.source_ref, part, VERSION).id == e.id
    ]
    if len(matches) != 1:
        raise ValueError(
            "Parameter candidates must locate every first-checkpoint entry"
        )
    item = matches[0]
    if replay(item, context.document) != part.normalized:
        raise ValueError(
            "Parameter source recipe must reproduce exact legacy normalized bytes"
        )
    candidate, recognized = classify(
        context.text, part, item, position, refs, result.enabled_rules
    )
    result.entries.append(candidate)
    result.rule_matches.extend(recognized)
    selected = "".join(context.text[s.start : s.end] for s in part.segments)
    mentions = refs.term_mentions(selected)
    if mentions:
        result.mentions.append({"inventory_id": item.id, "mentions": list(mentions)})


def _coverage(result: Candidates, scan: Scan) -> None:
    expected = {item.id for item in scan.entries}
    expected_fields = {
        (str(p["source_version_id"]), str(p["locator"]))
        for p in scan.fields
        if p["state"] in {"empty", "text"}
    }
    actual_fields = [
        (str(p["source_version_id"]), str(p["locator"])) for p in result.field_proofs
    ]
    if (
        len(actual_fields) != len(set(actual_fields))
        or set(actual_fields) != expected_fields
    ):
        raise ValueError(
            "Parameter span proofs must cover every exact first-checkpoint text field"
        )
    actual = [item.inventory_id for item in result.entries]
    if len(actual) != len(set(actual)) or set(actual) != set(expected):
        raise ValueError(
            "Parameter candidates must cover all and only first-checkpoint entries"
        )


def summary(candidates: Candidates) -> dict[str, JsonValue]:
    """Do not equate byte replay, consistent placeholder roles or semantic adoption."""
    roles: dict[str, JsonValue] = {}
    for role in ("body", "reminder", "token_header", "layout"):
        members = [item for item in candidates.entries if item.source_span.role == role]
        roles[role] = {
            "entries": len(members),
            "exact_normalized_templates": len({m.normalized_hash for m in members}),
            "schema_signatures": len({m.signature_hash for m in members}),
            "candidate_payload_templates": len(
                {m.payload_hash for m in members if m.payload_hash is not None}
            ),
            "complete_schemas": sum(m.parameter_schema is not None for m in members),
            "complete_without_disabled_rules": sum(
                m.parameter_schema is not None and not m.issues for m in members
            ),
            "blocked_only_by_disabled_numeric_rules": sum(
                set(m.issues) == {NUMERIC_RULE_DISABLED} for m in members
            ),
            "review_required": sum(bool(m.issues) for m in members),
        }
    return {
        "candidate_only": True,
        "entry_count": len(candidates.entries),
        "field_count": len(candidates.field_proofs),
        "source_span_roundtrip_complete": all(
            p["utf8_roundtrip"] is True for p in candidates.field_proofs
        ),
        "slot_counts": dict(
            Counter(h.semantic_role for m in candidates.entries for h in m.slots)
        ),
        "numeric_rule_counts": {
            rule: sum(
                h.numeric_rule == rule for m in candidates.entries for h in m.slots
            )
            for rule in NUMERIC_RULES
        },
        "inactive_numeric_rule_candidates": inactive_rules(candidates),
        "candidate_rule_counts": {
            rule: {
                "positions": sum(m["rule_id"] == rule for m in candidates.rule_matches),
                "uses": len(
                    {
                        m["inventory_id"]
                        for m in candidates.rule_matches
                        if m["rule_id"] == rule
                    }
                ),
                "body_positions": sum(
                    m["rule_id"] == rule and m["role"] == "body"
                    for m in candidates.rule_matches
                ),
                "body_uses": len(
                    {
                        m["inventory_id"]
                        for m in candidates.rule_matches
                        if m["rule_id"] == rule and m["role"] == "body"
                    }
                ),
            }
            for rule in candidates.enabled_rules
            if rule in BY_ID
        },
        "unresolved_reasons": dict(
            Counter(reason for m in candidates.entries for reason in m.issues)
        ),
        "roles": roles,
        "parameter_complete": not any(m.issues for m in candidates.entries),
        "term_mentions_are_bindings": False,
    }


def inactive_rules(candidates: Candidates) -> list[JsonValue]:
    """List lexical evidence without inserting a disabled ID into a candidate's active rules."""
    result: list[JsonValue] = []
    for value in array(configuration()["proposals"]):
        proposal = object_value(value)
        selected = [
            (candidate, hint)
            for candidate in candidates.entries
            for hint in candidate.slots
            if proposal["reason"] in hint.issues
        ]
        body = [(c, h) for c, h in selected if c.source_span.role == "body"]
        result.append(
            {
                **proposal,
                "positions": len(selected),
                "uses": len({c.inventory_id for c, _ in selected}),
                "body_positions": len(body),
                "body_uses": len({c.inventory_id for c, _ in body}),
                "members": [
                    {
                        "inventory_id": c.inventory_id,
                        "slot": h.name,
                        "source_segments": [
                            s.model_dump(mode="json") for s in h.source_segments
                        ],
                        "normalized_occurrence": h.occurrence.model_dump(mode="json"),
                    }
                    for c, h in sorted(
                        selected, key=lambda pair: (pair[0].inventory_id, pair[1].name)
                    )
                ],
            }
        )
    return result

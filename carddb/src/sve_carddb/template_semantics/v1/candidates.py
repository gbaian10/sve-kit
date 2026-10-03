"""Whole-batch candidate replay and separate mechanical forks versus semantic review."""

from collections import Counter, defaultdict
from dataclasses import dataclass, field
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.template_semantics.v1.inventory import entry, fields, replay
from sve_carddb.template_semantics.v1.parameters.analysis import (
    NUMERIC_RULE_PENDING,
    NUMERIC_RULES,
    analyze,
)
from sve_carddb.template_semantics.v1.parameters.candidate_matching import recognize
from sve_carddb.template_semantics.v1.parameters.numeric_rules import configuration
from sve_carddb.template_semantics.v1.parameters.rule_candidates import selection
from sve_carddb.template_semantics.v1.parameters.spans import locate
from sve_carddb.template_semantics.v1.parameters.verification import verify_candidate
from sve_carddb.template_semantics.v1.projection import project
from sve_carddb.template_sources.normalizer import VERSION, partition

if TYPE_CHECKING:
    from sve_carddb.frozen_sources import FrozenSources
    from sve_carddb.template_parameters.models import Candidate
    from sve_carddb.template_semantics.v1.inventory import Scan
    from sve_carddb.template_semantics.v1.parameters.references import References
    from sve_carddb.template_semantics.v1.parameters.spans import Located
    from sve_carddb.template_sources.models import Entry
    from sve_carddb.template_sources.normalizer import Part

PARSER = "translation-jp-v1"


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
    result = Candidates(enabled_rules=selection(enabled_rules))
    by_field: dict[tuple[str, str], list[Entry]] = defaultdict(list)
    for item in scan.entries:
        by_field[item.source_ref.source_version_id, item.source_ref.locator].append(
            item
        )
    for current in sources.inventory.current:
        source, raw, _ = sources.read(current.source_version_id, parser_version=PARSER)
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
    candidate = analyze(context.text, part, item, position, refs)
    verify_candidate(context.text, part, item, position, refs, candidate)
    result.entries.append(candidate)
    result.rule_matches.extend(
        recognize(context.text, part, candidate, refs, result.enabled_rules)
    )
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


def mechanical(candidate: Candidate) -> str:
    """Literal N/X and actual replacement positions differ even with equal legacy bytes."""
    return digest(
        canonical(
            [
                [h.occurrence.model_dump(mode="json"), h.transformation]
                for h in candidate.slots
            ]
        )
    )


def lineage(candidates: Candidates) -> tuple[dict[str, JsonValue], ...]:
    """No old parent has approved payload here, so DB supersedes stays null."""
    groups: dict[str, list[Candidate]] = defaultdict(list)
    for item in candidates.entries:
        if item.legacy_id is not None:
            groups[item.legacy_id].append(item)
    result: list[dict[str, JsonValue]] = []
    for identifier, members in sorted(groups.items()):
        if len({m.normalized_hash for m in members}) != 1:
            raise ValueError(
                "Legacy fingerprint collision must stop parameter candidate grouping"
            )
        variants: dict[str, list[JsonValue]] = defaultdict(list)
        mechanical_variants: dict[str, list[JsonValue]] = defaultdict(list)
        for member in members:
            variants[member.signature_hash].append(member.inventory_id)
            mechanical_variants[mechanical(member)].append(member.inventory_id)
        result.append(
            {
                "legacy_id": identifier,
                "normalized_hash": members[0].normalized_hash,
                "members": len(members),
                "mechanical_variants": dict(sorted(mechanical_variants.items())),
                "candidate_variants": dict(sorted(variants.items())),
                "requires_provenance_split": len(mechanical_variants) > 1,
                "semantic_review_required": any(m.issues for m in members),
                "supersedes_id": None,
            }
        )
    return tuple(result)


def summary(candidates: Candidates) -> dict[str, JsonValue]:
    """Do not equate byte replay, consistent placeholder roles or semantic adoption."""
    parents = lineage(candidates)
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
            "complete_without_numeric_rule_approval": sum(
                m.parameter_schema is not None and not m.issues for m in members
            ),
            "complete_after_numeric_rule_approval": sum(
                set(m.issues) == {NUMERIC_RULE_PENDING} for m in members
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
        "legacy_template_count": len(parents),
        "legacy_provenance_forks": sum(
            p["requires_provenance_split"] is True for p in parents
        ),
        "legacy_pending_semantics": sum(
            p["semantic_review_required"] is True for p in parents
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

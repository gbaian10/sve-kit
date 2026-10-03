"""Fixed position identity, historical comparison and approved-role resolution."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical
from sve_carddb.template_parameter_rules.models import LEGACY_IDS
from sve_carddb.template_semantics.v1.inventory import fields
from sve_carddb.template_semantics.v1.legacy import assert_restriction, historical_role
from sve_carddb.template_semantics.v1.parameters.analysis import Position, prepared
from sve_carddb.template_semantics.v1.parameters.numeric_rules import (
    NUMERIC_RULE_PENDING,
)
from sve_carddb.template_semantics.v1.parameters.rule_candidates import BY_ID
from sve_carddb.template_semantics.v1.projection import project
from sve_carddb.template_sources.normalizer import partition

if TYPE_CHECKING:
    from sve_carddb.frozen_sources import FrozenSources
    from sve_carddb.template_parameter_rules.loader import Loaded
    from sve_carddb.template_parameters.models import Candidate, Hint
    from sve_carddb.template_semantics.v1.candidates import Candidates
    from sve_carddb.template_semantics.v1.inventory import Scan
PARSER = "translation-jp-v1"


def numeric_identity(candidate: Candidate, hint: Hint, rule: str) -> bytes:
    """Counts cannot prove stability of ownership, coordinates, raw spelling and values."""
    return canonical(
        {
            "inventory_id": candidate.inventory_id,
            "slot": hint.name,
            "source_segments": [
                s.model_dump(mode="json") for s in hint.source_segments
            ],
            "numeric_rule": rule,
            "value": hint.value,
            "raw_hash": hint.raw_hash,
        }
    )


def _historical_positions(
    sources: FrozenSources, scan: Scan, candidates: Candidates
) -> tuple[bytes, ...]:
    by_id = {item.id: item for item in scan.entries}
    by_field: dict[tuple[str, str], list[Candidate]] = {}
    for candidate in candidates.entries:
        ref = by_id[candidate.inventory_id].source_ref
        by_field.setdefault((ref.source_version_id, ref.locator), []).append(candidate)
    old = []
    for current in sources.inventory.current:
        source, raw, _ = sources.read(current.source_version_id, parser_version=PARSER)
        _, document = project(raw, source.url, "jp")
        for locator, text, section in fields(document):
            if text is None:
                continue
            parts = partition(text, section=section)
            for candidate in by_field.get((source.id, locator), ()):
                selected = [
                    p
                    for p in parts
                    if tuple((s.start, s.end) for s in p.segments)
                    == tuple((s.start, s.end) for s in candidate.source_span.segments)
                    and p.role == candidate.source_span.role
                ]
                if len(selected) != 1:
                    raise ValueError(
                        "Recognition historical replay cannot locate an exact source part"
                    )
                template, _units = prepared(text, selected[0])
                for hint in candidate.slots:
                    if hint.semantic_role != "numeric":
                        continue
                    position = Position(
                        hint.occurrence.start,
                        hint.occurrence.end,
                        hint.transformation,
                        hint.semantic_role,
                    )
                    assert_restriction(
                        template.normalized,
                        position,
                        reminder=candidate.source_span.role == "reminder",
                    )
                    rule, _issues = historical_role(template.normalized, position)
                    if rule is not None:
                        old.append(numeric_identity(candidate, hint, rule))
    return tuple(sorted(old))


def _resolve(
    loaded: Loaded | None, candidates: Candidates
) -> tuple[tuple[bytes, ...], tuple[bytes, ...]]:
    rules = {} if loaded is None else {r.rule_id: r for r in loaded.policy.rules}
    roles = () if loaded is None else loaded.policy.scope.roles
    matches = {
        (str(row["inventory_id"]), str(row["slot"])): row
        for row in candidates.rule_matches
    }
    if len(matches) != len(candidates.rule_matches):
        raise ValueError("Recognition matches must have unique slot ownership")
    resolved, remaining = [], []
    for candidate in candidates.entries:
        unresolved = set(candidate.issues) - {
            reason for hint in candidate.slots for reason in hint.issues
        }
        for hint in candidate.slots:
            issues = set(hint.issues)
            key = (candidate.inventory_id, hint.name)
            rule_id: str | None = hint.numeric_rule
            match = matches.get(key)
            if match is not None:
                if rule_id is not None:
                    raise ValueError(
                        "Recognition new rules cannot claim old numeric ownership"
                    )
                rule_id = str(match["rule_id"])
            allowed = rules.get(rule_id) if rule_id is not None else None
            if (
                allowed is not None
                and candidate.source_span.role in roles
                and (hint.value is not None or match is not None)
                and "invalid_safe_unsigned_decimal" not in issues
            ):
                reason = (
                    NUMERIC_RULE_PENDING
                    if rule_id in LEGACY_IDS
                    else BY_ID[str(rule_id)].reason
                )
                if reason not in issues:
                    raise ValueError(
                        "Recognition matched slot lacks its exact pending reason"
                    )
                issues.remove(reason)
                resolved.append(
                    canonical(
                        {
                            "inventory_id": candidate.inventory_id,
                            "slot": hint.name,
                            "rule_id": rule_id,
                            "recognized_role": allowed.recognized_role,
                            "raw_hash": hint.raw_hash,
                            "source_segments": [
                                s.model_dump(mode="json") for s in hint.source_segments
                            ],
                            "remaining_issues": list[JsonValue](sorted(issues)),
                            "match_evidence": match,
                            "value": hint.value,
                            "normalized_occurrence": hint.occurrence.model_dump(
                                mode="json"
                            ),
                        }
                    )
                )
            if issues:
                remaining.append(
                    canonical(
                        {
                            "inventory_id": candidate.inventory_id,
                            "slot": hint.name,
                            "issues": list[JsonValue](sorted(issues)),
                        }
                    )
                )
        if unresolved:
            remaining.append(
                canonical(
                    {
                        "inventory_id": candidate.inventory_id,
                        "slot": None,
                        "issues": list[JsonValue](sorted(unresolved)),
                    }
                )
            )
    return tuple(sorted(resolved)), tuple(sorted(remaining))

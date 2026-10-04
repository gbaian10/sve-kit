"""Resolve current matcher results without policy receipts or producer replay."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical
from sve_carddb.template_parameter_rules.models import LEGACY_IDS
from sve_carddb.template_parameters.numeric_rules import NUMERIC_RULE_PENDING
from sve_carddb.template_parameters.rule_candidates import BY_ID

if TYPE_CHECKING:
    from sve_carddb.template_parameters.inventory import Candidates


def resolve_roles(
    candidates: Candidates, rules: dict[str, str], *, roles: tuple[str, ...]
) -> tuple[tuple[bytes, ...], tuple[bytes, ...]]:
    """Resolve only actual matching positions; enabling rules does not erase other causes."""
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
                            "recognized_role": allowed,
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

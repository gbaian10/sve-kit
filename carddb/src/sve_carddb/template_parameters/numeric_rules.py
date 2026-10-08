"""Lexical vetoes and disabled diagnostic proposals, separate from active numeric roles."""

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.template_parameters.models import NumericRule

VERSION = "numeric-rule-proposals-v3"
RECOVERY_PENDING = "numeric_recovery_amount_requires_review"
ORDINAL_PENDING = "numeric_ordinal_requires_review"
RECOVERY = re.compile(r"^\u56de\u5fa9")
ORDINAL = re.compile(r"^(?:\u679a|\u4f53|\u70b9|\u56de|\u30bf\u30fc\u30f3|PP)\u76ee")

NUMERIC_SUFFIX = re.compile(
    r"^(?:(?P<suffix_unit_cards>枚(?!目))|(?P<suffix_unit_entities>体(?!目))|"
    r"(?P<suffix_unit_points>点(?!目))|(?P<suffix_unit_times>回(?![復目]))|"
    r"(?P<suffix_unit_turns>ターン(?!目))|(?P<suffix_unit_pp>PP(?!目)))(?![A-Za-z0-9_])"
)
NUMERIC_PREFIX = re.compile(
    r"(?:(?P<prefix_field_cost>コスト)|(?P<prefix_field_attack>攻撃力)|"
    r"(?P<prefix_field_health>体力)|(?P<prefix_field_pp>PP)|"
    r"(?P<prefix_field_level>レベル))[=:：]?$"
)
NUMERIC_RULES: tuple[NumericRule, ...] = (
    "suffix_unit_cards",
    "suffix_unit_entities",
    "suffix_unit_points",
    "suffix_unit_times",
    "suffix_unit_turns",
    "suffix_unit_pp",
    "prefix_field_cost",
    "prefix_field_attack",
    "prefix_field_health",
    "prefix_field_pp",
    "prefix_field_level",
)
NUMERIC_RULE_DISABLED = "numeric_rule_disabled"

SIGNS = ("-", "+", "−", "＋", "－")
ASCII_BEFORE = r"[A-Za-z0-9_]$"
ASCII_AFTER = r"^[A-Za-z0-9_]"
RESOURCE_PREFIX_EXCEPTION = "PP"


def excluded(after: str) -> str | None:
    """Veto complete lexical collisions before a suffix rejection could fall back to a prefix."""
    if RECOVERY.match(after):
        return RECOVERY_PENDING
    if ORDINAL.match(after):
        return ORDINAL_PENDING
    return None


def configuration() -> dict[str, JsonValue]:
    """Bind diagnostic definitions to the recipe without authorizing a new slot role."""
    return {
        "version": VERSION,
        "guard_order": ["sign", "ascii_identifier", "lexical_veto", "suffix", "prefix"],
        "context": "normalized body or unchanged reminder; immediately after numeric occurrence",
        "value_validation": "exact ASCII/fullwidth decimal and safe unsigned integer; invalid values stay pending",
        "rules": [definition(rule) for rule in NUMERIC_RULES],
        "proposals": [
            {
                "id": "candidate_recovery_amount",
                "enabled": False,
                "status": "proposal_only",
                "suffix_pattern": RECOVERY.pattern,
                "reason": RECOVERY_PENDING,
                "semantic_role_proposal": "amount recovered, not number of repetitions",
            },
            {
                "id": "candidate_ordinal",
                "enabled": False,
                "status": "proposal_only",
                "suffix_pattern": ORDINAL.pattern,
                "reason": ORDINAL_PENDING,
                "semantic_role_proposal": "ordinal index; unit and semantic lower bound require approval",
            },
        ],
    }


def conditions(rule: NumericRule) -> dict[str, JsonValue]:
    """Bind classifier guards and precedence from the constants used by analysis."""
    return {
        "selected_group": rule,
        "scope": {"region": "jp", "roles": ["body", "reminder"]},
        "context": {"body_nfkc": True, "reminder_nfkc": False},
        "sign_exclusion": list(SIGNS),
        "ascii_before": ASCII_BEFORE,
        "ascii_after": ASCII_AFTER,
        "ascii_guard_exceptions": {
            "prefix_pattern": NUMERIC_PREFIX.pattern,
            "after_starts_with": RESOURCE_PREFIX_EXCEPTION,
        },
        "lexical_vetoes": [RECOVERY.pattern, ORDINAL.pattern],
        "suffix_pattern": NUMERIC_SUFFIX.pattern,
        "prefix_pattern": NUMERIC_PREFIX.pattern,
        "guard_order": ["sign", "ascii_identifier", "lexical_veto", "suffix", "prefix"],
        "suffix_precedes_prefix": True,
        "exact_raw_safe_unsigned_decimal": True,
    }


def definition(rule: NumericRule) -> dict[str, JsonValue]:
    """Export the lexical conditions used by the numeric classifier."""
    match_conditions = conditions(rule)
    return {
        "id": rule,
        "matcher_version": VERSION + ":" + rule,
        "match_conditions": match_conditions,
    }

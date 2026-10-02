"""Lexical vetoes and disabled diagnostic proposals, separate from active numeric roles."""

import re
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

VERSION = "numeric-rule-proposals-v2"
RECOVERY_PENDING = "numeric_recovery_amount_requires_review"
ORDINAL_PENDING = "numeric_ordinal_requires_review"
RECOVERY = re.compile(r"^\u56de\u5fa9")
ORDINAL = re.compile(r"^(?:\u679a|\u4f53|\u70b9|\u56de|\u30bf\u30fc\u30f3|PP)\u76ee")


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

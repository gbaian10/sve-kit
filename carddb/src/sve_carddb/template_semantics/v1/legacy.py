"""Finite historical kernel retained independently of policy/Git orchestration."""

import re
from typing import TYPE_CHECKING

from sve_carddb.template_semantics.v1.parameters.analysis import Position, numeric_role
from sve_carddb.template_semantics.v1.parameters.numeric_rules import (
    NUMERIC_PREFIX,
    NUMERIC_RULE_PENDING,
    NUMERIC_RULES,
    NUMERIC_SUFFIX,
    excluded,
)

if TYPE_CHECKING:
    from sve_carddb.template_parameters.models import NumericRule


def historical_role(
    normalized: str, position: Position
) -> tuple[NumericRule | None, tuple[str, ...]]:
    """Replay the original v2 kernel; only the reminder fullwidth signs differ."""
    before = normalized[: position.start]
    after = normalized[position.end :]
    if before.endswith(("-", "+", "−")):
        return None, ("signed_numeric_requires_review",)
    if (
        (before and re.search(r"[A-Za-z0-9_]$", before))
        or re.match(r"^[A-Za-z0-9_]", after)
    ) and not (NUMERIC_PREFIX.search(before) or after.startswith("PP")):
        return None, ("numeric_identifier_requires_review",)
    if (reason := excluded(after)) is not None:
        return None, (reason,)
    match = NUMERIC_SUFFIX.match(after) or NUMERIC_PREFIX.search(before)
    if match is not None:
        rule = next(rule for rule in NUMERIC_RULES if rule == match.lastgroup)
        return rule, (NUMERIC_RULE_PENDING,)
    return None, ("numeric_role_requires_review",)


def assert_restriction(normalized: str, position: Position, *, reminder: bool) -> None:
    """No additional exclusion or changed role can borrow the original eight approvals."""
    old = historical_role(normalized, position)
    current = numeric_role(normalized, position)
    if old == current:
        return
    before = normalized[: position.start]
    if not (
        reminder
        and before.endswith(("＋", "－"))
        and current == (None, ("signed_numeric_requires_review",))
    ):
        raise ValueError(
            "Recognition historical bridge changes more than the sign restriction"
        )

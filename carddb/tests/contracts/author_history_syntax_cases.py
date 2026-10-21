"""Registered fragments from author syntax examples; these do not execute full effects."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

SYNTAX_HISTORY_CASES: list[tuple[str, str, JsonValue]] = [
    (
        "syntax-section-2",
        "A.cond",
        {
            "history": {
                "event": "stat_changed",
                "where": {"ref": "this", "stat": "hp", "direction": "up"},
                "window": "this_turn",
                "at_least": 1,
            }
        },
    ),
    ("GAP-A-038", "A.expr.numeric", {"event": "value"}),
    (
        "GAP-A-043",
        "A.event",
        {
            "any_of": [
                {"fused": {"who": "self", "material": {"trait": "財宝"}}},
                {"played": {"who": "self", "sel": {"trait": "財宝"}}},
            ]
        },
    ),
]

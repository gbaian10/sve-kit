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
    (
        "syntax-batch",
        "A.event",
        {
            "dealt_damage": {
                "ref": "this",
                "kind": "ability",
                "recipient": {"who": "opp", "zone": "field", "kind": "follower"},
            }
        },
    ),
    (
        "GAP-A-014",
        "A.event",
        {
            "ub_activated": {
                "who": "self",
                "holder": {
                    "who": "self",
                    "zone": "field",
                    "kind": "follower",
                    "other": True,
                },
            }
        },
    ),
    (
        "GAP-A-022",
        "A.event",
        {"stat_changed": {"ref": "this", "stat": ["power", "hp"], "direction": "up"}},
    ),
    ("GAP-A-027", "A.event", {"drive_trigger": {"who": "self"}}),
    (
        "GAP-A-029",
        "A.cond",
        {
            "history": {
                "event": "damaged",
                "where": {"recipient": "item"},
                "window": "this_turn",
                "at_least": 1,
            }
        },
    ),
    (
        "GAP-A-030",
        "A.event",
        {
            "dealt_damage": {
                "ref": "this",
                "kind": "attack",
                "recipient": {"leaders": "opponents"},
            }
        },
    ),
    (
        "GAP-A-033",
        "A.cond",
        {
            "history": {
                "event": "ub_activated",
                "who": "self",
                "window": "this_turn",
                "exclude": "this_execution",
                "at_least": 2,
            }
        },
    ),
    ("GAP-A-038", "A.event", {"dice_rolled": {"who": "self"}}),
    (
        "GAP-A-038",
        "A.cond",
        {"cmp": {"left": {"event": "value"}, "op": "eq", "right": 6}},
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
    ("GAP-A-043", "A.event", {"fused": {"who": "self", "material": {"trait": "財宝"}}}),
    ("GAP-A-043", "A.event", {"played": {"who": "self", "sel": {"trait": "財宝"}}}),
    (
        "GAP-A-046",
        "A.event",
        {
            "discarded": {
                "ref": "this",
                "from": "hand",
                "who": "self",
                "by": {"ability_of": {"who": "self", "trait": "学院"}},
            }
        },
    ),
    (
        "GAP-A-047",
        "A.event",
        {
            "drive_checked": {
                "holder": {"who": "self", "zone": "field", "kind": "follower"}
            }
        },
    ),
    (
        "GAP-A-050",
        "A.event",
        {
            "fused_as_material": {
                "ref": "this",
                "by": {
                    "ability_of": {"who": "self", "kind": "follower", "trait": "八獄"}
                },
            }
        },
    ),
    (
        "GAP-A-052",
        "A.event",
        {
            "moved": {
                "from": {"who": "opp", "zone": "deck"},
                "to": "cemetery",
                "during": {"turn": "self"},
            }
        },
    ),
    ("GAP-A-058", "A.event", {"equipped": {"equipment": "this"}}),
    ("GAP-A-061", "A.event", {"damaged": {"recipient": "this"}}),
    (
        "GAP-A-061",
        "A.cond",
        {"cmp": {"left": {"event": "amount"}, "op": "ge", "right": 5}},
    ),
    ("GAP-A-061", "A.expr.numeric", {"event": "amount"}),
    ("GAP-A-063", "A.event", {"phase": {"which": "end", "whose": "self"}}),
]

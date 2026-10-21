"""Card-labelled fragments from frozen JP text; these do not execute full effects.

Ability-level limits are outside these fragments; a per-turn limit alone does
not establish an event's during filter.
"""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

CARD_HISTORY_CASES: list[tuple[str, str, JsonValue]] = [
    (
        "BP01-033",
        "A.event",
        {
            "enter_field": {
                "who": "self",
                "sel": {"class": "ロイヤル", "kind": "follower"},
            }
        },
    ),
    ("BP01-016", "A.event", {"left_field": {"ref": "this"}}),
    ("CP04-007", "A.event", {"placed_ex": {"ref": "this"}}),
    (
        "BP01-170",
        "A.event",
        {"discarded": {"ref": "this", "from": "hand", "who": "self"}},
    ),
    (
        "BP14-074",
        "A.event",
        {
            "any_of": [
                {"discarded": {"ref": "this", "from": "hand", "who": "self"}},
                {"banished": {"ref": "this", "from": "hand", "who": "self"}},
            ]
        },
    ),
    ("BP01-106", "A.event", {"destroyed": {"who": "opp", "sel": {"kind": "follower"}}}),
    ("CP02-SL10", "A.event", {"damaged": {"recipient": "this"}}),
    (
        "BP04-003",
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
        "CP03-019",
        "A.event",
        {
            "attacked": {
                "who": "self",
                "sel": {"trait": "アクアフォース", "kind": "follower"},
            }
        },
    ),
    ("BP02-016", "A.event", {"evolved": {"who": "self", "sel": {"kind": "follower"}}}),
    ("BP05-006", "A.event", {"phase": {"which": "end", "whose": "self"}}),
    ("CP02-044", "A.event", {"played": {"who": "self", "sel": {"orig_cost_ge": 5}}}),
    ("BP02-095", "A.event", {"life_changed": {"who": "self", "direction": "up"}}),
    (
        "BP11-082",
        "A.event",
        {"stat_changed": {"ref": "this", "stat": ["power", "hp"], "direction": "up"}},
    ),
    ("CP04-102", "A.event", {"acted": {"ref": "this"}}),
    ("CP01-082", "A.event", {"raced": {"who": "self", "sel": {"kind": "follower"}}}),
    (
        "CP04-SL15",
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
    ("BP03-071", "A.event", {"targeted": {"ref": "this"}}),
    ("BP21-SL19", "A.event", {"dice_rolled": {"who": "self"}}),
    (
        "CP03-P27",
        "A.event",
        {
            "drive_checked": {
                "holder": {"who": "self", "zone": "field", "kind": "follower"}
            }
        },
    ),
    ("CP03-P81", "A.event", {"drive_trigger": {"who": "self"}}),
    (
        "BP05-029",
        "A.event",
        {
            "moved": {
                "from": {"who": "opp", "zone": "deck"},
                "to": "cemetery",
                "during": {"turn": "self"},
            }
        },
    ),
    ("BP05-094", "A.event", {"drew": {"who": "self", "not_phase": "start"}}),
    ("CP04-PR22", "A.event", {"equipped": {"equipment": "this"}}),
    (
        "BP19-048",
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
    ("BP05-092", "A.event", {"state": {"leader_life": {"who": "self", "le": 0}}}),
    (
        "BP20-P37",
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
        "BP14-037",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "history_sum": {
                        "event": "payment",
                        "who": "self",
                        "where": {"cost_kind": "earth_rite"},
                        "field": "removed_stack",
                        "window": "this_turn",
                    }
                },
                "op": "ge",
                "right": 2,
            }
        },
    ),
    (
        "BP21-092",
        "A.expr.numeric",
        {
            "history_count": {
                "event": "life_changed",
                "who": "self",
                "where": {"direction": "up"},
                "window": "this_turn",
            }
        },
    ),
    ("BP20-018", "A.cond", {"paid": "optional"}),
    (
        "CP03-SL16",
        "A.cond",
        {
            "did": {
                "result": {"entry_payment": "additional"},
                "where": {"name": "ブラスター・ダーク", "zone": "field"},
            }
        },
    ),
    (
        "BP17-030",
        "A.cond",
        {"chosen": {"ref": "t", "where": {"name_contains": "リオード"}}},
    ),
    (
        "BP07-088",
        "A.expr.numeric",
        {"counter": {"on": {"lki_of": "this"}, "name": "祈り"}},
    ),
    (
        "BP05-057",
        "A.expr.numeric",
        {"power_of": {"objects_of": "sacrificed", "view": "before"}},
    ),
    (
        "BP04-116",
        "A.expr.numeric",
        {"half_up": {"cost_of": {"objects_of": "discarded", "view": "before"}}},
    ),
    ("BP13-053", "A.expr.numeric", {"plus": [{"orig_cost_of": "t"}, 2]}),
    ("BP03-001", "A.expr.numeric", {"x_paid": "removed"}),
    (
        "BP08-103",
        "A.expr.numeric",
        {
            "count_of": {
                "receipt": "milled",
                "where": {"orig_cost_eq": {"bound": "declared"}},
                "view": "after",
            }
        },
    ),
    (
        "CP02-006",
        "A.expr.numeric",
        {
            "at": "event",
            "value": {
                "history_count": {
                    "event": "played",
                    "who": "self",
                    "window": "this_turn",
                }
            },
        },
    ),
    (
        "SP01-039",
        "A.expr.numeric",
        {
            "history_sum": {
                "event": "life_changed",
                "who": "self",
                "where": {"direction": "up"},
                "window": "this_turn",
                "field": "delta",
            }
        },
    ),
    (
        "BP04-116",
        "A.expr.numeric",
        {"cost_of": {"objects_of": "cost", "view": "before"}},
    ),
    ("BP13-053", "A.expr.numeric", {"orig_cost_of": "target"}),
    ("CP02-SL10-amount", "A.expr.numeric", {"event": "amount"}),
]

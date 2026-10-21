"""Card-labelled query fragments from frozen JP text and sections, not full effects."""

from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from pydantic import JsonValue

CARD_QUERIES: list[tuple[str, str, JsonValue]] = [
    (
        "BP09-077",
        "A.sel.object-core",
        {"who": "current", "zone": "field", "kind": "follower", "power_max": True},
    ),
    (
        "BP02-105",
        "A.expr.numeric",
        {
            "count": {
                "who": "self",
                "zone": "field",
                "kind": "follower",
                "keyword": "ward",
                "other": True,
            }
        },
    ),
    (
        "BP12-034",
        "A.expr.numeric",
        {
            "agg": {
                "of": {"who": "self", "zone": "field", "kind": "follower"},
                "attr": "power",
                "fn": "max",
            }
        },
    ),
    (
        "BP10-011",
        "A.expr.numeric",
        {"distinct_orig_costs": {"who": "self", "zone": "ex"}},
    ),
    ("BP13-038", "A.expr.numeric", {"counter": {"on": "this", "name": "魔力"}}),
    ("ECP02-020", "A.expr.numeric", {"hand": {"master_of": "t"}}),
    (
        "BP04-034",
        "A.cond",
        {
            "exists": {
                "who": "self",
                "zone": "field",
                "kind": "follower",
                "not_class": "ロイヤル",
            }
        },
    ),
    (
        "BP09-117",
        "A.cond",
        {
            "cmp": {
                "left": {
                    "count": {
                        "who": "self",
                        "zone": "field",
                        "kind": "follower",
                        "traits_any": ["天使", "堕天使"],
                    }
                },
                "op": "ge",
                "right": 2,
            }
        },
    ),
    ("BP03-028", "A.cond", {"not": {"entered_from": {"ref": "this", "from": "hand"}}}),
    ("BP10-062", "A.cond", {"played_from": {"ref": "this", "from": "ex"}}),
    ("BP17-SL25", "A.cond", {"entered_by": {"ref": "this", "by": "ability"}}),
    ("BP01-SL16", "A.cond", {"is": {"ref": "attack_target", "kind": "follower"}}),
    ("BP18-073", "A.cond", {"on_field": "this"}),
    ("BP07-058", "A.cond", {"in_zone": {"ref": "this", "zone": "ex"}}),
    ("BP15-102", "A.cond", {"standing": "this"}),
    ("CP03-003", "A.cond", {"my_turn": True}),
    ("CP02-T01", "A.cond", {"construction": "アイドルマスター シンデレラガールズ"}),
    ("BP01-014", "A.cond", {"combo": 3}),
    ("ETD01-018", "A.cond", {"awakened": True}),
    ("BP14-072", "A.cond", {"hand": {"who": "self", "eq": 0}}),
    ("BP15-076", "A.cond", {"leader_life": {"who": "self", "eq": 7}}),
    ("BP13-043", "A.cond", {"zone_free": {"who": "current", "zone": "field", "ge": 1}}),
    (
        "BP03-092",
        "A.sel.object-core",
        {
            "any_of": [
                {
                    "who": "self",
                    "zone": "cemetery",
                    "kind": "follower",
                    "cost_le": 3,
                    "keyword": "storm",
                },
                {
                    "who": "self",
                    "zone": "cemetery",
                    "kind": "follower",
                    "cost_le": 3,
                    "keyword": "ward",
                },
            ]
        },
    ),
    (
        "BP01-039",
        "A.sel.object-core",
        {"in": "looked", "kind": "follower", "cost_eq": 1},
    ),
    ("BP01-065", "A.cond", {"spellchain": 10}),
    ("BP02-080", "A.cond", {"necro": 10}),
    ("BP01-112", "A.cond", {"crimson": True}),
    ("BP03-090", "A.cond", {"entered_from": {"ref": "this", "from": "hand"}}),
]

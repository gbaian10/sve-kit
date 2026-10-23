"""Reviewed single-group parent examples and independent decoder correspondence."""

from typing import TYPE_CHECKING

from typing_extensions import TypeForm

from sve_carddb.contracts import author_costs as costs
from sve_carddb.contracts import author_leaves as leaves
from sve_carddb.contracts import author_programs as steps
from sve_carddb.contracts import author_quantities as quantities
from sve_carddb.contracts import author_queries as queries
from sve_carddb.contracts import author_selection_parents as parents

from .author_adapters import adapters, type_names

if TYPE_CHECKING:
    from pydantic import JsonValue

PARENTS_TYPES: dict[str, tuple[TypeForm[object], str]] = {
    "A.target.group-filter": (parents.TargetGroupFilter, "TargetGroupFilter"),
    "A.target.grouped.exact": (parents.TargetGroupedExact, "TargetGroupedExact"),
    "A.target.grouped.up-to": (parents.TargetGroupedUpTo, "TargetGroupedUpTo"),
    "A.target.grouped.any": (parents.TargetGroupedAny, "TargetGroupedAny"),
    "A.targets": (parents.Targets, "Targets"),
    "A.quantity": (queries.Quantity, "Quantity"),
    "A.selection-quantity": (quantities.SelectionQuantity, "SelectionQuantity"),
    "A.deck-slice": (quantities.DeckSlice, "DeckSlice"),
    "CounterName": (leaves.CounterName, "CounterName"),
    "DistinctBy": (parents.DistinctBy, "DistinctBy"),
    "A.target.exact": (parents.TargetExact, "TargetExact"),
    "A.target.up-to": (parents.TargetUpTo, "TargetUpTo"),
    "A.target.any": (parents.TargetAny, "TargetAny"),
    "A.target": (parents.Target, "Target"),
    "A.act.choose.exact": (steps.ChooseExact, "ChooseExact"),
    "A.act.choose.up-to": (steps.ChooseUpTo, "ChooseUpTo"),
    "A.act.choose.any": (steps.ChooseAny, "ChooseAny"),
    "A.act.choose": (steps.Choose, "Choose"),
    "A.act.search.exact": (steps.SearchExact, "SearchExact"),
    "A.act.search.up-to": (steps.SearchUpTo, "SearchUpTo"),
    "A.act.search": (steps.Search, "Search"),
    "A.cost.counter.self.exact": (costs.CounterCostSelfExact, "CounterCostSelfExact"),
    "A.cost.counter.self.up-to": (costs.CounterCostSelfUpTo, "CounterCostSelfUpTo"),
    "A.cost.counter.self.any": (costs.CounterCostSelfAny, "CounterCostSelfAny"),
    "A.cost.counter.self": (costs.CounterCostSelf, "CounterCostSelf"),
    "A.cost.counter.selected.exact": (
        costs.CounterCostSelectedExact,
        "CounterCostSelectedExact",
    ),
    "A.cost.counter.selected.up-to": (
        costs.CounterCostSelectedUpTo,
        "CounterCostSelectedUpTo",
    ),
    "A.cost.counter.selected.any": (
        costs.CounterCostSelectedAny,
        "CounterCostSelectedAny",
    ),
    "A.cost.counter.selected": (costs.CounterCostSelected, "CounterCostSelected"),
    "A.cost.counter": (costs.CounterCost, "CounterCost"),
}

CARD_PARENTS: list[tuple[str, str, dict[str, JsonValue]]] = [
    (
        "BP01-125",
        "A.target.exact",
        {
            "bind": "t",
            "from": {"who": "opp", "kind": "follower", "leader_or": True},
            "n": 1,
        },
    ),
    (
        "BP21-PR13",
        "A.target.up-to",
        {
            "bind": "t",
            "from": {"who": "opp", "zone": "field", "kind": "follower"},
            "upto": 3,
            "roles": {"primary": {"if_nonempty": 1}, "rest": "remaining"},
        },
    ),
    (
        "BP01-001",
        "A.target.any",
        {
            "bind": "t",
            "from": {
                "who": "self",
                "zone": "ex",
                "kind": "follower",
                "trait": "妖精",
            },
            "any": True,
        },
    ),
    (
        "BP19-110",
        "A.act.choose.exact",
        {"bind": "picked", "from": {"in": "looked"}, "n": 1},
    ),
    (
        "BP03-108",
        "A.act.choose.up-to",
        {
            "bind": "picked",
            "from": {"in": "looked", "trait": "童話"},
            "upto": 2,
            "may": True,
        },
    ),
    (
        "BP04-SL08",
        "A.act.choose.any",
        {
            "bind": "picked",
            "from": {"who": "self", "zone": "hand", "kind": "follower"},
            "any": True,
            "may": True,
        },
    ),
    (
        "BP14-055",
        "A.act.search.exact",
        {
            "sel": {"who": "self", "zone": "deck", "name": "竜山温泉"},
            "n": 1,
            "to": "field",
        },
    ),
    (
        "BP18-P10",
        "A.act.search.up-to",
        {
            "sel": {
                "who": "self",
                "zone": "deck",
                "kind": "follower",
                "trait": "透京",
            },
            "upto": 2,
            "cost_sum_le": 3,
            "to": "field",
        },
    ),
    (
        "BP01-062",
        "A.cost.counter.self.exact",
        {"on": "this", "name": "スペル", "n": 3},
    ),
    (
        "CP04-115-section0",
        "A.cost.counter.self.exact",
        {"on": "this", "name": "バッテリー", "n": "X"},
    ),
    (
        "BP03-001",
        "A.cost.counter.self.any",
        {"on": "this", "name": "童話", "any": True, "result": "removed"},
    ),
    (
        "BP14-063",
        "A.cost.counter.selected.exact",
        {
            "on": {"who": "self", "zone": "field", "name": "竜山温泉"},
            "name": "神湯",
            "holders": 1,
            "n": 2,
        },
    ),
]
DECK_CASES: list[tuple[str, dict[str, JsonValue]]] = [
    ("ETD01-005", {"who": "self", "n": 1}),
    ("BP06-028", {"who": "opp", "n": 3}),
    ("ECP01-038", {"who": "opponents", "n": 20}),
    ("BP06-108", {"who": "self", "n": 3}),
    ("CP03-105", {"who": "self", "n": 1}),
    (
        "BP05-020",
        {"who": "self", "n": {"half_up": {"count": {"who": "self", "zone": "deck"}}}},
    ),
]

PARENTS = adapters(PARENTS_TYPES)
PARENT_NAMES = type_names(PARENTS_TYPES)

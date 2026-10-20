"""Reviewed single-group parent examples and independent decoder correspondence."""

from typing import TYPE_CHECKING

from pydantic import TypeAdapter

from sve_carddb.contracts import author_costs as costs
from sve_carddb.contracts import author_leaves as leaves
from sve_carddb.contracts import author_quantities as quantities
from sve_carddb.contracts import author_queries as queries
from sve_carddb.contracts import author_selection_parents as parents

from .author_selector_cases import CONFIG

if TYPE_CHECKING:
    from pydantic import JsonValue

PARENTS: dict[str, TypeAdapter[object]] = {
    "A.target.group-filter": TypeAdapter(parents.TargetGroupFilter, config=CONFIG),
    "A.target.grouped.exact": TypeAdapter(parents.TargetGroupedExact, config=CONFIG),
    "A.target.grouped.up-to": TypeAdapter(parents.TargetGroupedUpTo, config=CONFIG),
    "A.target.grouped.any": TypeAdapter(parents.TargetGroupedAny, config=CONFIG),
    "A.targets": TypeAdapter(parents.Targets, config=CONFIG),
    "A.quantity": TypeAdapter(queries.Quantity, config=CONFIG),
    "A.selection-quantity": TypeAdapter(quantities.SelectionQuantity, config=CONFIG),
    "A.deck-slice": TypeAdapter(quantities.DeckSlice),
    "CounterName": TypeAdapter(leaves.CounterName, config=CONFIG),
    "DistinctBy": TypeAdapter(parents.DistinctBy, config=CONFIG),
    "A.target.exact": TypeAdapter(parents.TargetExact, config=CONFIG),
    "A.target.up-to": TypeAdapter(parents.TargetUpTo, config=CONFIG),
    "A.target.any": TypeAdapter(parents.TargetAny, config=CONFIG),
    "A.target": TypeAdapter(parents.Target, config=CONFIG),
    "A.act.choose.exact": TypeAdapter(parents.ChooseExact, config=CONFIG),
    "A.act.choose.up-to": TypeAdapter(parents.ChooseUpTo, config=CONFIG),
    "A.act.choose.any": TypeAdapter(parents.ChooseAny, config=CONFIG),
    "A.act.choose": TypeAdapter(parents.Choose, config=CONFIG),
    "A.act.search.exact": TypeAdapter(parents.SearchExact, config=CONFIG),
    "A.act.search.up-to": TypeAdapter(parents.SearchUpTo, config=CONFIG),
    "A.act.search": TypeAdapter(parents.Search, config=CONFIG),
    "A.cost.counter.self.exact": TypeAdapter(costs.CounterCostSelfExact, config=CONFIG),
    "A.cost.counter.self.up-to": TypeAdapter(costs.CounterCostSelfUpTo, config=CONFIG),
    "A.cost.counter.self.any": TypeAdapter(costs.CounterCostSelfAny, config=CONFIG),
    "A.cost.counter.self": TypeAdapter(costs.CounterCostSelf, config=CONFIG),
    "A.cost.counter.selected.exact": TypeAdapter(
        costs.CounterCostSelectedExact, config=CONFIG
    ),
    "A.cost.counter.selected.up-to": TypeAdapter(
        costs.CounterCostSelectedUpTo, config=CONFIG
    ),
    "A.cost.counter.selected.any": TypeAdapter(
        costs.CounterCostSelectedAny, config=CONFIG
    ),
    "A.cost.counter.selected": TypeAdapter(costs.CounterCostSelected, config=CONFIG),
    "A.cost.counter": TypeAdapter(costs.CounterCost, config=CONFIG),
}
PARENT_NAMES = {
    "A.target.group-filter": "TargetGroupFilter",
    "A.target.grouped.exact": "TargetGroupedExact",
    "A.target.grouped.up-to": "TargetGroupedUpTo",
    "A.target.grouped.any": "TargetGroupedAny",
    "A.targets": "Targets",
    "A.quantity": "Quantity",
    "A.selection-quantity": "SelectionQuantity",
    "A.deck-slice": "DeckSlice",
    "CounterName": "CounterName",
    "DistinctBy": "DistinctBy",
    "A.target.exact": "TargetExact",
    "A.target.up-to": "TargetUpTo",
    "A.target.any": "TargetAny",
    "A.target": "Target",
    "A.act.choose.exact": "ChooseExact",
    "A.act.choose.up-to": "ChooseUpTo",
    "A.act.choose.any": "ChooseAny",
    "A.act.choose": "Choose",
    "A.act.search.exact": "SearchExact",
    "A.act.search.up-to": "SearchUpTo",
    "A.act.search": "Search",
    "A.cost.counter.self.exact": "CounterCostSelfExact",
    "A.cost.counter.self.up-to": "CounterCostSelfUpTo",
    "A.cost.counter.self.any": "CounterCostSelfAny",
    "A.cost.counter.self": "CounterCostSelf",
    "A.cost.counter.selected.exact": "CounterCostSelectedExact",
    "A.cost.counter.selected.up-to": "CounterCostSelectedUpTo",
    "A.cost.counter.selected.any": "CounterCostSelectedAny",
    "A.cost.counter.selected": "CounterCostSelected",
    "A.cost.counter": "CounterCost",
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
            "on": {"who": "each", "zone": "field", "name": "竜山温泉"},
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

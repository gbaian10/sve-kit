"""Object queries and counted queries retain distinct author positions."""

from typing import TYPE_CHECKING

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_counts as counts
from sve_carddb.contracts import author_selectors as author

if TYPE_CHECKING:
    from pydantic import JsonValue

CONFIG = ConfigDict(regex_engine="python-re")
COUNTS: dict[str, TypeAdapter[object]] = {
    "A.expr.count-objects": TypeAdapter(counts.CountObjects, config=CONFIG),
    "A.expr.count-union": TypeAdapter(counts.CountUnion),
    "A.expr.count-source": TypeAdapter(counts.CountSource, config=CONFIG),
}
SELECTORS: dict[str, TypeAdapter[object]] = {
    "A.sel.zone-objects": TypeAdapter(author.ZoneObjects, config=CONFIG),
    "A.sel.in-objects": TypeAdapter(author.InObjects, config=CONFIG),
    "A.sel.object-atom": TypeAdapter(author.ObjectAtom, config=CONFIG),
    "A.sel.object-union": TypeAdapter(author.ObjectUnion, config=CONFIG),
    "A.sel.object-core": TypeAdapter(author.ObjectSel, config=CONFIG),
}
SELECTOR_NAMES = {
    "A.sel.zone-objects": "ZoneObjects",
    "A.sel.in-objects": "InObjects",
    "A.sel.object-atom": "ObjectAtom",
    "A.sel.object-union": "ObjectUnion",
    "A.sel.object-core": "ObjectSel",
}
OBJECTS = "A.sel.object-core"
COUNTED = "A.expr.count-source"
COUNT_UNION: dict[str, JsonValue] = {
    "any_of": [
        {"who": "self", "zone": "cemetery", "trait": "陰陽師"},
        {"who": "self", "zone": "cemetery", "kind": "spell"},
    ]
}
QUERY_CASES: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.sel.zone-objects", {"who": "each", "zone": "field", "kind": "follower"}),
    ("A.sel.zone-objects", {"who": "self", "trait": "財宝", "orig_cost_ge": 1}),
    ("A.sel.zone-objects", {"who": "self", "zone": ["field", "ex"]}),
    ("A.sel.in-objects", {"in": "shown", "kind": "follower"}),
    ("A.sel.in-objects", {"in": "looked", "except": {"in": "chosen"}}),
    ("A.sel.object-core", {"in": {"objects_of": "moved", "view": "before"}}),
    ("A.sel.object-atom", {"in": "shown", "kind": "follower"}),
    (
        "A.sel.object-union",
        {
            "any_of": [
                {
                    "in": "looked",
                    "kind": "follower",
                    "trait": "童話",
                    "orig_cost_le": 5,
                },
                {"in": "looked", "kind": "amulet", "trait": "童話", "orig_cost_le": 5},
            ]
        },
    ),
    ("A.expr.count-objects", {"who": "each", "zone": "field"}),
    ("A.expr.count-union", COUNT_UNION),
    ("A.expr.count-source", {"who": "self", "zone": "cemetery", "other": True}),
]

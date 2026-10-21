"""Object queries and counted queries retain distinct author positions."""

from typing import TYPE_CHECKING

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.contracts import author_queries as author

from .author_adapters import adapters

if TYPE_CHECKING:
    from pydantic import JsonValue

CONFIG = ConfigDict(regex_engine="python-re")
COUNTS: dict[str, TypeAdapter[object]] = {
    "A.expr.count-objects": TypeAdapter(author.CountObjects, config=CONFIG),
    "A.expr.count-union": TypeAdapter(author.CountUnion),
    "A.expr.count-source": TypeAdapter(author.CountSource, config=CONFIG),
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

COUNTS.update(
    {
        "A.expr.count-atom": TypeAdapter(author.CountAtom, config=CONFIG),
        "A.collection.in": TypeAdapter(author.CollectionIn, config=CONFIG),
        "A.collection.objects": TypeAdapter(author.CollectionObjects, config=CONFIG),
        "A.collection.atom": TypeAdapter(author.CollectionAtom, config=CONFIG),
        "A.collection.union": TypeAdapter(author.CollectionUnion),
        "A.collection": TypeAdapter(author.CollectionSource, config=CONFIG),
    }
)

QUERY_CASES.extend(
    [
        ("A.expr.count-atom", {"in": "shown"}),
        ("A.collection.in", {"in": {"objects_of": "moved", "view": "after"}}),
        ("A.collection.objects", {"who": "self", "zone": "field"}),
        ("A.collection.atom", {"in": "shown"}),
        (
            "A.collection.union",
            {"any_of": [{"in": "shown"}, {"who": "self", "zone": "field"}]},
        ),
        (
            "A.collection",
            {"in": "shown", "except": {"in": "chosen"}, "where": {"standing": "item"}},
        ),
    ]
)

SELECTORS = adapters(
    {key: (getattr(author, name), name) for key, name in SELECTOR_NAMES.items()}
)

SELECTOR_NAMES.update(
    {
        "A.expr.count-objects": "CountObjects",
        "A.expr.count-union": "CountUnion",
        "A.expr.count-source": "CountSource",
        "A.expr.count-atom": "CountAtom",
        "A.collection.in": "CollectionIn",
        "A.collection.objects": "CollectionObjects",
        "A.collection.atom": "CollectionAtom",
        "A.collection.union": "CollectionUnion",
        "A.collection": "CollectionSource",
    }
)

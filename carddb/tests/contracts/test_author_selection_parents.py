"""Quantity modes and their parents retain distinct source and parameter domains."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import TypeAdapter, ValidationError

from sve_carddb.contracts.author_references import Ref
from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import NUMERIC
from .author_parent_cases import CARD_PARENTS, DECK_CASES, PARENTS
from .author_recipient_cases import RECIPIENTS
from .author_selector_cases import CONFIG, SELECTORS

if TYPE_CHECKING:
    from pydantic import JsonValue

NODES = object_value(
    parse(
        (
            Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
        ).read_bytes()
    )
)
ADAPTERS = (
    PARENTS
    | NUMERIC
    | SELECTORS
    | RECIPIENTS
    | {"A.ref": TypeAdapter(Ref, config=CONFIG)}
)
BASES: dict[str, dict[str, JsonValue]] = {
    "A.target": {"bind": "t", "from": {"in": "looked"}},
    "A.act.choose": {"bind": "picked", "from": {"in": "looked"}},
    "A.act.search": {"sel": {"who": "self", "zone": "deck"}, "to": "hand"},
    "A.cost.counter.self": {"on": "this", "name": "童話"},
    "A.cost.counter.selected": {
        "on": {"who": "each", "zone": "field", "name": "竜山温泉"},
        "name": "神湯",
        "holders": 1,
    },
}


def _check(name: str, value: JsonValue, *, accepted: bool) -> None:
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value))
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value))


def _branches(name: str) -> list[str]:
    node = object_value(object_value(NODES["$defs"])[name])
    variants = node.get("anyOf")
    if not isinstance(variants, list):
        return [name]
    result = []
    for branch in variants:
        fields = object_value(branch)
        if (
            fields.get("$dynamicRef")
            == "urn:sve-kit:author-nodes:1.0:authorParam#authorParam"
        ):
            result.append("A.param")
            continue
        reference = fields["$ref"]
        assert isinstance(reference, str)
        result.extend(_branches(reference.removeprefix("#/$defs/")))
    return result


@pytest.mark.parametrize(
    ("name", "value"),
    [(name, value) for _, name, value in CARD_PARENTS],
    ids=[card for card, _, _ in CARD_PARENTS],
)
def test_card_parent_fragments(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)
    root = name.rsplit(".", 1)[0]
    _check(root, value, accepted=True)
    if root.startswith("A.cost.counter"):
        _check("A.cost.counter", value, accepted=True)
    assert not Draft202012Validator(NODES).is_valid(value)


@pytest.mark.parametrize(
    "value", [value for _, value in DECK_CASES], ids=[card for card, _ in DECK_CASES]
)
def test_card_deck_fragments(value: JsonValue) -> None:
    _check("A.deck-slice", value, accepted=True)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize(
    "quantity",
    [
        {},
        {"n": 1, "upto": 1},
        {"n": 1, "any": True},
        {"upto": 1, "any": True},
        {"n": 1, "upto": 1, "any": True},
    ],
)
def test_quantity_modes_are_required_and_exclusive(
    name: str, quantity: dict[str, JsonValue]
) -> None:
    _check(name, BASES[name] | quantity, accepted=False)


@pytest.mark.parametrize("name", BASES)
@pytest.mark.parametrize("key", ["n", "upto"])
def test_selection_parameters_stay_in_whitelisted_parent_positions(
    name: str, key: str
) -> None:
    _check(
        name,
        {**BASES[name], key: {"param": "amount"}},
        accepted=not name.startswith("A.cost"),
    )


@pytest.mark.parametrize(
    ("name", "fields"),
    [
        ("A.target", {"option": 1, "distinct_by": "class"}),
        (
            "A.act.choose",
            {
                "by": {"master_of": "t"},
                "may": False,
                "reveal": True,
                "order": False,
                "distinct_by": "orig_cost",
            },
        ),
        ("A.act.search", {"result": "searched"}),
        ("A.cost.counter.selected", {"result": "paid_counter"}),
    ],
)
def test_optional_parent_fields_preserve_authored_values(
    name: str, fields: dict[str, JsonValue]
) -> None:
    _check(name, {**BASES[name], "n": 1, **fields}, accepted=True)


@pytest.mark.parametrize("key", ["n", "upto", "any"])
def test_target_distinct_spellings_are_exclusive_and_preserved(key: str) -> None:
    base = {**BASES["A.target"], key: True if key == "any" else 1}
    first = {**base, "distinct_names": True}
    second = {**base, "distinct_by": "name"}
    for value in (first, second):
        _check("A.target", value, accepted=True)
    assert canonical(first) != canonical(second)
    _check("A.target", {**first, "distinct_by": "name"}, accepted=False)
    _check("A.target", {**first, "distinct_by": "orig_cost"}, accepted=False)


def test_search_destination_parameters_do_not_replace_structure() -> None:
    _check(
        "A.act.search",
        {**BASES["A.act.search"], "n": 1, "to": {"param": "destination"}},
        accepted=True,
    )
    _check(
        "A.act.search",
        {
            **BASES["A.act.search"],
            "n": 1,
            "to": {"order": "printed", "partition": [{"zone": "hand", "n": 1}]},
        },
        accepted=False,
    )

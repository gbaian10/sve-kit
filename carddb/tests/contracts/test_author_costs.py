"""Validate payment shapes, ordered author bytes and their precise context boundaries."""

import json
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, parse

from .author_cost_cases import CARD_COSTS, CARD_TARGETS, CARD_WRAPPERS, COSTS
from .author_parent_cases import PARENTS
from .test_author_schema_drift import NODES

if TYPE_CHECKING:
    from pydantic import JsonValue

ADAPTERS = COSTS | PARENTS


def _validator(name: str) -> Draft202012Validator:
    return Draft202012Validator(
        {"$id": NODES["$id"], "$defs": NODES["$defs"], "$ref": "#/$defs/" + name}
    )


def _check(name: str, value: JsonValue, *, accepted: bool) -> None:
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value))
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value))


@pytest.mark.parametrize(
    "value", [v for _, v in CARD_COSTS], ids=[c for c, _ in CARD_COSTS]
)
def test_card_payment_fragments(value: JsonValue) -> None:
    _check("A.cost", value, accepted=True)
    assert not Draft202012Validator(NODES).is_valid(value)


@pytest.mark.parametrize(
    ("name", "value"),
    [(n, v) for _, n, v in CARD_WRAPPERS],
    ids=[c for c, _, _ in CARD_WRAPPERS],
)
def test_card_wrapper_fragments(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)


@pytest.mark.parametrize(
    "value",
    [
        {"ep": 1},
        {"one_of": [{"steps": [{"pp": 1}, {"act_self": True}]}, {"ep": 1}]},
        {"steps": [{"one_of": [{"pp": 1}, {"ep": 1}]}, {"pp": 1}]},
    ],
)
def test_synthetic_composite_payments(value: JsonValue) -> None:
    _check("A.cost", value, accepted=True)


@pytest.mark.parametrize(
    "mode",
    [
        {"n": 1},
        {"upto": "X"},
        {"any": True},
        {"groups": [{"sel": {"in": "this"}, "n": 1}, {"sel": {"who": "self"}, "n": 1}]},
    ],
)
@pytest.mark.parametrize(
    "op",
    [
        "discard",
        "banish",
        "act",
        "to_cemetery",
        "return_to_hand",
        "reveal",
        "flip_down",
        "flip_up",
        "to_ex",
        "to_deck_bottom",
        "to_deck",
    ],
)
def test_synthetic_material_modes(op: str, mode: dict[str, JsonValue]) -> None:
    value = dict(mode)
    if "groups" not in value:
        value["sel"] = {"who": "self", "zone": "hand"}
    value.update(result="materials", distinct_by="name", cost_sum_ge=1)
    if op == "to_deck":
        value["shuffle"] = True
    _check("A.cost", {op: value}, accepted=True)


@pytest.mark.parametrize(
    "value",
    [{"sel": {"who": "self"}}, {"sel": {"leader": "self"}, "n": 1}],
)
def test_material_rejects_context_and_mode_mixups(value: JsonValue) -> None:
    _check("A.cost.material", value, accepted=False)


@pytest.mark.parametrize("name", ["A.cost.optional", "A.cost.additional"])
def test_wrapper_and_material_receipts_have_separate_positions(name: str) -> None:
    value: JsonValue = {
        "discard": {"sel": {"who": "self"}, "n": 1, "result": "material"},
        "result": "whole",
    }
    _check(name, value, accepted=True)
    _check("A.cost", value, accepted=False)
    _check(name, {"pp": 1, "option": 1}, accepted=name == "A.cost.optional")
    _check(name, {"pp": 1, "result": "this"}, accepted=False)


def _bytes(name: str, value: JsonValue) -> bytes:
    adapter = ADAPTERS[name]
    decoded = adapter.validate_json(canonical(value))
    return canonical(parse(adapter.dump_json(decoded)))


def test_decoded_cost_bytes_preserve_order_repetition_and_mapping_independence() -> (
    None
):
    first: JsonValue = {"discard": {"sel": {"zone": "hand", "who": "self"}, "n": 1}}
    reordered: JsonValue = {"discard": {"n": 1, "sel": {"who": "self", "zone": "hand"}}}
    assert _bytes("A.cost", first) == _bytes("A.cost", reordered)
    forward: JsonValue = {"steps": [first, {"pp": 1}, first]}
    assert parse(_bytes("A.cost", forward)) == forward
    assert _bytes("A.cost", forward) != _bytes(
        "A.cost", {"steps": [{"pp": 1}, first, first]}
    )
    assert _bytes("A.cost", forward) != _bytes("A.cost", {"steps": [first, {"pp": 1}]})


@pytest.mark.parametrize(
    "value",
    [value for _, value in CARD_TARGETS],
    ids=[card for card, _ in CARD_TARGETS],
)
def test_card_grouped_and_distinct_targets(value: JsonValue) -> None:
    _check("A.target", value, accepted=True)


@pytest.mark.parametrize("mode", [{"n": 2}, {"upto": 2}, {"any": True}])
def test_grouped_target_synthetic_modes_match_one_branch(
    mode: dict[str, JsonValue],
) -> None:
    value: JsonValue = {
        "bind": "t",
        "from": {"who": "self"},
        "groups": [{"orig_cost_le": 5}, {"orig_cost_le": 3}],
        **mode,
    }
    _check("A.target", value, accepted=True)
    names = [
        "A.target." + suffix
        for suffix in [
            "exact",
            "up-to",
            "any",
            "grouped.exact",
            "grouped.up-to",
            "grouped.any",
        ]
    ]
    assert sum(_validator(name).is_valid(value) for name in names) == 1

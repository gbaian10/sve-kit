"""Recipient sums distinguish object-only unions from unions containing leaders."""

import json
from copy import deepcopy
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import TypeAdapter, ValidationError

from sve_carddb.contracts.author_references import PlayerRef, Ref
from sve_carddb.core.json import canonical, object_value, parse

from .author_numeric_cases import NUMERIC
from .author_recipient_cases import (
    CARD_QUERIES,
    LEADER_OR,
    LEADER_UNION,
    OBJECT_UNION,
    RECIPIENT,
    RECIPIENT_UNION,
    RECIPIENTS,
)
from .author_selector_cases import CONFIG, COUNTED, COUNTS, OBJECTS, SELECTORS

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
    RECIPIENTS
    | SELECTORS
    | COUNTS
    | NUMERIC
    | {
        "A.ref": TypeAdapter(Ref, config=CONFIG),
        "A.player-ref": TypeAdapter(PlayerRef, config=CONFIG),
    }
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
    "value", [v for _, v in CARD_QUERIES], ids=[c for c, _ in CARD_QUERIES]
)
def test_card_sources_round_trip(value: JsonValue) -> None:
    _check(RECIPIENT, value, accepted=True)
    assert not Draft202012Validator(NODES).is_valid(value)


@pytest.mark.parametrize(
    ("name", "value"),
    [
        ("A.sel.leader-or", LEADER_OR),
        ("A.sel.recipient-atom", {"leader_of": "this"}),
        (RECIPIENT_UNION, LEADER_UNION),
        (RECIPIENT, LEADER_UNION),
    ],
)
def test_named_recipient_variants(name: str, value: JsonValue) -> None:
    _check(name, value, accepted=True)


@pytest.mark.parametrize(
    "limbs", [[{"in": "shown"}], [{"who": "self"}, {"in": "shown"}]]
)
def test_all_object_union_is_not_a_recipient_union(limbs: list[JsonValue]) -> None:
    value: dict[str, JsonValue] = {"any_of": limbs}
    _check(OBJECT_UNION, value, accepted=True)
    _check(RECIPIENT_UNION, value, accepted=False)
    _check(RECIPIENT, value, accepted=True)


@pytest.mark.parametrize("key", ["who", "kind", "leader_or"])
def test_leader_or_required_fields(key: str) -> None:
    value = LEADER_OR.copy()
    value.pop(key)
    _check("A.sel.leader-or", value, accepted=False)
    # A selector without leader_or may be a valid object-only query.
    if key != "leader_or":
        _check(RECIPIENT, value, accepted=False)


def test_equivalent_spellings_preserve_distinct_author_bytes() -> None:
    variants: list[JsonValue] = [
        LEADER_OR,
        {**LEADER_OR, "zone": "field"},
        {
            "any_of": [
                {"leader": "opp"},
                {"who": "opp", "zone": "field", "kind": "follower"},
            ]
        },
        {
            "any_of": [
                {"who": "opp", "zone": "field", "kind": "follower"},
                {"leader": "opp"},
            ]
        },
    ]
    for value in variants:
        _check(RECIPIENT, value, accepted=True)
    assert len({canonical(value) for value in variants}) == len(variants)
    repeated = deepcopy(variants[-1])
    limbs = object_value(repeated)["any_of"]
    assert isinstance(limbs, list)
    limbs.append(deepcopy(limbs[0]))
    _check(RECIPIENT, repeated, accepted=True)
    assert canonical(repeated) != canonical(variants[-1])


@pytest.mark.parametrize("name", [OBJECTS, COUNTED])
@pytest.mark.parametrize(
    ("combined", "limbs"),
    [
        (
            {"who": "self", "zone": ["field", "ex"]},
            [{"who": "self", "zone": "field"}, {"who": "self", "zone": "ex"}],
        ),
        (
            {"who": "self", "zone": "deck", "names": ["アマテラス", "ツクヨミ"]},
            [
                {"who": "self", "zone": "deck", "name": "アマテラス"},
                {"who": "self", "zone": "deck", "name": "ツクヨミ"},
            ],
        ),
    ],
)
def test_collection_fields_and_union_do_not_rewrite_each_other(
    name: str, combined: JsonValue, limbs: list[JsonValue]
) -> None:
    union: dict[str, JsonValue] = {"any_of": limbs}
    for value in (combined, union):
        _check(name, value, accepted=True)
    assert canonical(combined) != canonical(union)

"""Event roles, history narrowing and capture boundaries from author contracts."""

import json
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.core.json import canonical

from .author_history_card_cases import CARD_HISTORY_CASES
from .author_history_cases import HISTORY
from .author_history_syntax_cases import SYNTAX_HISTORY_CASES
from .author_numeric_cases import NUMERIC, C, E
from .author_receipt_cases import RECEIPTS

if TYPE_CHECKING:
    from pydantic import JsonValue

ADAPTERS = HISTORY | NUMERIC | RECEIPTS
QUERY = "A.history-query"
EVENT_CASES: list[tuple[str, dict[str, JsonValue]]] = [
    (
        "enter_field",
        {"who": "self", "sel": {"kind": "follower"}, "during": {"turn": "opp"}},
    ),
    ("left_field", {"ref": "this", "who": "self"}),
    ("placed_ex", {"sel": {"trait": "プリンセス"}, "who": "self"}),
    ("discarded", {"who": "self", "from": "hand", "sel": {"kind": "follower"}}),
    ("banished", {"who": "self", "from": "hand"}),
    ("destroyed", {"sel": {"kind": "amulet"}, "who": "self"}),
    ("damaged", {"recipient": "this", "kind": "ability", "during": {"turn": "opp"}}),
    (
        "dealt_damage",
        {
            "who": "self",
            "sel": {"trait": "かげろう"},
            "kind": "ability",
            "zone": "field",
        },
    ),
    ("attacked", {"ref": "this"}),
    ("attack_target", {"ref": "this"}),
    ("evolved", {"who": "self", "sel": {"other": True}}),
    ("super_evolved", {"who": "self", "ref": "this"}),
    ("phase", {"which": "end", "whose": "self"}),
    ("played", {"who": "self", "sel": {"trait": "財宝"}}),
    ("ability_played", {"who": "self", "holder": {"who": "self", "kind": "follower"}}),
    ("life_changed", {"who": "self", "direction": "up"}),
    ("stat_changed", {"ref": "this", "stat": ["power", "hp"], "direction": "up"}),
    ("counter_changed", {"ref": "this", "name": "stack", "after": {"ge": 2}}),
    ("acted", {"ref": "this"}),
    ("stood", {"ref": "this"}),
    ("raced", {"ref": "this"}),
    ("drive_gained", {"ref": "this"}),
    ("ub_activated", {"who": "self", "holder": "this"}),
    ("fused", {"who": "self", "material": {"trait": "ドラゴニュート"}}),
    ("targeted", {"ref": "this", "by": {"ability_of": {"who": "opp"}}}),
    ("dice_rolled", {"who": "self"}),
    ("drive_checked", {"holder": {"who": "self", "trait": "ロイヤルパラディン"}}),
    ("drive_trigger", {"who": "self"}),
    (
        "moved",
        {"ref": "this", "from": {"who": "opp", "zone": "deck"}, "to": "cemetery"},
    ),
    ("drew", {"who": "self", "not_phase": "start"}),
    ("equipped", {"equipment": "this"}),
    ("fused_as_material", {"ref": "this"}),
    ("state", {"cmp": {"left": {"leader_life": "self"}, "op": "le", "right": 10}}),
]


def _check(name: str, value: JsonValue, *, accepted: bool = True) -> None:
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value), strict=True)
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value), strict=True)


@pytest.mark.parametrize(("event", "payload"), EVENT_CASES[:-1])
def test_history_promotes_only_registered_roles(
    event: str, payload: dict[str, JsonValue]
) -> None:
    lifted = {
        key: value
        for key, value in payload.items()
        if key in {"who", "zone", "from", "to", "sel"}
    }
    remaining = {key: value for key, value in payload.items() if key not in lifted}
    query: dict[str, JsonValue] = {"event": event, "window": "this_turn", **lifted}
    if remaining:
        query["where"] = remaining
    _check(QUERY, query)
    _check(E, {"history_count": query})
    _check(C, {"history": {**query, "at_least": {"plus": [1, 1]}, "at": "event"}})
    _check(E, {"history_count": {**query, "at_least": 1}}, accepted=False)
    _check(E, {"history_count": {**query, "at": "event"}}, accepted=False)
    _check(C, {"history": {**query, "at": "live"}}, accepted=False)
    _check(
        QUERY,
        {key: value for key, value in query.items() if key != "window"},
        accepted=False,
    )
    _check(QUERY, {**query, "where": {}}, accepted=False)
    for key, value in lifted.items():
        _check(QUERY, {**query, "where": {**remaining, key: value}}, accepted=False)
    for key, value in remaining.items():
        _check(QUERY, {**query, key: value}, accepted=False)


@pytest.mark.parametrize(
    "value",
    [
        "additional",
        "optional",
        {"receipt": "additional"},
        {"receipt": "optional"},
        {"receipt": "paid_cost"},
        {"receipt": {"entry_payment": "additional"}},
    ],
)
def test_paid_keywords_and_named_receipts_are_disjoint(value: JsonValue) -> None:
    _check(C, {"paid": value})
    _check(E, {"paid": value}, accepted=False)


STATE_READS: list[JsonValue] = [
    {"count": {"who": "self", "zone": "hand"}},
    {"hand": "self"},
    {"leader_life": "self"},
    {"resource": {"who": "self", "kind": "pp"}},
    {"turn_number": {"who": "self"}},
    {"zone_free": {"who": "self", "zone": "ex"}},
    {"distinct_names": {"in": "shown"}},
    {"distinct_orig_costs": {"in": "shown"}},
    {"agg": {"of": {"in": "shown"}, "attr": "hp", "fn": "sum"}},
    {"power_of": "t"},
    {"hp_of": "t"},
    {"cost_of": "t"},
    {"orig_cost_of": "t"},
    {"history_count": {"event": "played", "window": "this_turn"}},
    {
        "history_sum": {
            "event": "payment",
            "window": "this_turn",
            "where": {"cost_kind": "earth_rite"},
            "field": "removed_stack",
        }
    },
]


@pytest.mark.parametrize("read", STATE_READS)
@pytest.mark.parametrize("at", ["live", "play", "resolution", "event", "selection"])
def test_capture_read_matrix(read: JsonValue, at: str) -> None:
    capture: dict[str, JsonValue] = {"at": at, "value": read}
    if at == "selection":
        capture["selection"] = "target"
        _check(E, {"at": at, "value": read}, accepted=False)
    else:
        _check(E, {**capture, "selection": "target"}, accepted=False)
    _check(E, capture)
    _check(C, capture, accepted=False)
    _check(E, {**capture, "value": capture}, accepted=False)
    _check(E, {**capture, "at": "unknown"}, accepted=False)
    _check(E, {"plus": [capture, 1]})


@pytest.mark.parametrize(
    "read",
    [0, "X"],
)
def test_capture_rejects_frozen_or_unregistered_reads(read: JsonValue) -> None:
    _check(E, {"at": "event", "value": read}, accepted=False)


@pytest.mark.parametrize(
    ("event", "field", "where"),
    [
        ("damaged", "amount", {"recipient": "item"}),
        ("dealt_damage", "amount", {"kind": "ability"}),
        ("life_changed", "delta", {"direction": "up"}),
        ("payment", "removed_stack", {"cost_kind": "earth_rite"}),
    ],
)
def test_history_sum_is_narrowed(
    event: str, field: str, where: dict[str, JsonValue]
) -> None:
    query: dict[str, JsonValue] = {
        "event": event,
        "window": "this_turn",
        "where": where,
        "field": field,
    }
    _check(E, {"history_sum": query})
    _check(E, {"history_sum": {**query, "distinct": "facts"}})
    _check(E, {"history_sum": {**query, "distinct": "subjects"}}, accepted=False)
    _check(E, {"history_sum": {**query, "field": "removed"}}, accepted=False)
    _check(E, {"history_sum": {**query, "at_least": 1}}, accepted=False)
    if event in {"life_changed", "payment"}:
        _check(
            E,
            {
                "history_sum": {
                    key: value for key, value in query.items() if key != "where"
                }
            },
            accepted=False,
        )


@pytest.mark.parametrize(
    ("source", "name", "value"),
    CARD_HISTORY_CASES + SYNTAX_HISTORY_CASES,
    ids=[source for source, _, _ in CARD_HISTORY_CASES + SYNTAX_HISTORY_CASES],
)
def test_card_and_syntax_fragments_round_trip(
    source: str, name: str, value: JsonValue
) -> None:
    assert source
    _check(name, value)

"""Event roles, history narrowing and capture boundaries from author contracts."""

import json
from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value

from .author_history_card_cases import CARD_HISTORY_CASES
from .author_history_cases import HISTORY
from .author_history_syntax_cases import SYNTAX_HISTORY_CASES
from .author_numeric_cases import NUMERIC, C, E
from .author_receipt_cases import RECEIPTS
from .test_author_schema_drift import NODES, assert_selector_definition

if TYPE_CHECKING:
    from pydantic import JsonValue

ADAPTERS = HISTORY | NUMERIC | RECEIPTS
EVENT = "A.event"
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


def _values(*values: JsonValue) -> tuple[JsonValue, ...]:
    return values


def _check(name: str, value: JsonValue, *, accepted: bool = True) -> None:
    validator = Draft202012Validator(
        {"$id": NODES["$id"], "$defs": NODES["$defs"], "$ref": "#/$defs/" + name}
    )
    assert validator.is_valid(value) == accepted
    adapter = ADAPTERS[name]
    if accepted:
        decoded = adapter.validate_json(canonical(value), strict=True)
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(json.dumps(value), strict=True)


@pytest.mark.parametrize(("event", "payload"), EVENT_CASES)
def test_each_direct_event_is_closed(event: str, payload: dict[str, JsonValue]) -> None:
    _check(EVENT, {event: payload})
    _check(EVENT, {event: {**payload, "unknown": True}}, accepted=False)
    _check(EVENT, {event: payload, "extra": {}}, accepted=False)
    _check(EVENT, {event: None}, accepted=False)
    _check(E, {event: payload}, accepted=False)
    if event != "state":
        _check(EVENT, {"any_of": [{event: payload}]})
        _check(EVENT, {"any_of": [{"any_of": [{event: payload}]}]}, accepted=False)
    else:
        _check(EVENT, {"any_of": [{event: payload}]}, accepted=False)


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


@pytest.mark.parametrize(
    "value",
    [
        "paid_cost",
        {"entry_payment": "additional"},
        {"receipt": {"event": "move_receipt"}},
        {"receipt": "this"},
        None,
    ],
)
def test_paid_rejects_unregistered_payloads(value: JsonValue) -> None:
    _check(C, {"paid": value}, accepted=False)


@pytest.mark.parametrize("op", ["eq", "ne"])
def test_player_comparisons_are_same_type(op: str) -> None:
    value: dict[str, JsonValue] = {
        "left": {"player": {"owner_of": "t"}},
        "op": op,
        "right": {"player": "self"},
    }
    _check(C, {"cmp": value})
    for wrong in _values("self", 1, {"bound": "p"}, {"leader": "self"}):
        _check(C, {"cmp": {**value, "right": wrong}}, accepted=False)
    for ordered in ["lt", "le", "gt", "ge"]:
        _check(C, {"cmp": {**value, "op": ordered}}, accepted=False)


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
    [
        0,
        "X",
        {"bound": "n"},
        {"plus": [1, 2]},
        {"dice": "d"},
        {"x_paid": "cost"},
        {"event": "value"},
        {"paid": "additional"},
        {"counter": {"on": "this", "name": "stack"}},
        {"lki_of": "this"},
        {"objects_of": "cost", "view": "before"},
    ],
)
def test_capture_rejects_frozen_or_unregistered_reads(read: JsonValue) -> None:
    _check(E, {"at": "event", "value": read}, accepted=False)


@pytest.mark.parametrize(
    "property_name", ["power_of", "hp_of", "cost_of", "orig_cost_of"]
)
@pytest.mark.parametrize("view", ["before", "after"])
def test_singleton_material_properties_are_not_recaptured(
    property_name: str, view: str
) -> None:
    value: dict[str, JsonValue] = {property_name: {"objects_of": "cost", "view": view}}
    _check(E, value)
    _check(E, {"at": "event", "value": value}, accepted=False)
    _check(E, {property_name: {"lki_of": "t"}}, accepted=False)


def test_lki_is_only_an_explicit_counter_operand() -> None:
    _check(E, {"counter": {"on": {"lki_of": "this"}, "name": "prayer"}})
    _check(E, {"lki_of": "this"}, accepted=False)
    _check(C, {"standing": {"lki_of": "this"}}, accepted=False)
    _check(E, {"count": {"in": {"lki_of": "this"}}}, accepted=False)


@pytest.mark.parametrize("key", ["dice", "x_paid"])
def test_receipt_and_die_reads_require_names(key: str) -> None:
    _check(E, {key: "result"})
    for value in _values("this", 1, {"bound": "result"}, None):
        _check(E, {key: value}, accepted=False)


@pytest.mark.parametrize("field", ["amount", "value"])
def test_event_numeric_read_is_closed(field: str) -> None:
    _check(E, {"event": field})
    _check(C, {"event": field}, accepted=False)
    _check(E, {"event": {"kind": "damaged", "field": field}}, accepted=False)


@pytest.mark.parametrize(
    "field", ["holder", "move_receipt", "delta", "from", "to", "unknown"]
)
def test_non_numeric_event_fields_do_not_enter_numeric_reads(field: str) -> None:
    _check(E, {"event": field}, accepted=False)


@pytest.mark.parametrize("op", ["eq", "le", "ge"])
def test_history_die_values_are_bounded_and_exclusive(op: str) -> None:
    for n in _values(1, 6):
        query: dict[str, JsonValue] = {
            "event": "dice_rolled",
            "window": "this_turn",
            "where": {"value": {op: n}},
        }
        _check(QUERY, query)
        _check(EVENT, {"dice_rolled": {"value": {op: n}}}, accepted=False)
    for wrong in _values(0, 7, True, "X", {"param": "n"}):
        _check(
            QUERY,
            {
                "event": "dice_rolled",
                "window": "this_turn",
                "where": {"value": {op: wrong}},
            },
            accepted=False,
        )
    _check(
        QUERY,
        {
            "event": "dice_rolled",
            "window": "this_turn",
            "where": {"value": {"eq": 1, "ge": 1}},
        },
        accepted=False,
    )


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


def test_history_only_payment_and_structured_windows() -> None:
    query: dict[str, JsonValue] = {
        "event": "payment",
        "who": "self",
        "where": {"cost_kind": "earth_rite"},
        "window": {"since": "anchor"},
    }
    _check(QUERY, query)
    _check(C, {"history": query})
    _check(EVENT, {"payment": {}}, accepted=False)
    _check(QUERY, {**query, "window": "since:anchor"}, accepted=False)
    _check(
        QUERY,
        {**query, "where": {"cost_kind": "earth_rite", "phase": "main"}},
        accepted=False,
    )
    for event in ["state", "any_of", "counter_reached", "unknown"]:
        _check(QUERY, {"event": event, "window": "game"}, accepted=False)


def test_phase_parameter_permission_is_local_to_on() -> None:
    _check(EVENT, {"phase": {"which": {"param": "phase"}, "whose": "self"}})
    _check(
        QUERY,
        {
            "event": "phase",
            "window": "game",
            "where": {"which": {"param": "phase"}, "whose": "self"},
        },
        accepted=False,
    )
    for payload in _values({}, {"which": "end"}, {"whose": "self"}):
        _check(EVENT, {"phase": payload}, accepted=False)


def test_subject_filters_have_no_selection_source_permissions() -> None:
    cases: list[tuple[str, JsonValue]] = [
        ("who", "self"),
        ("zone", "field"),
        ("in", "t"),
        ("except", {"in": "t"}),
        ("power_max", True),
        ("any_of", [{"kind": "follower"}]),
    ]
    for key, value in cases:
        _check("A.event-subject-filter", {key: value}, accepted=False)
    for key in ["kind", "name", "trait", "class", "cost_ge", "orig_cost_eq", "hp_le"]:
        _check("A.event-subject-filter", {key: {"param": "v"}}, accepted=False)
        _check(
            "A.event-subject-filter",
            {key: {"same_as": {"objects_of": "r", "view": "before"}}},
            accepted=False,
        )
    _check(
        "A.event-subject-filter",
        {
            "where": {
                "cmp": {
                    "left": {"count": {"who": {"param": "side"}, "zone": "field"}},
                    "op": "ge",
                    "right": {"plus": [1, {"param": "offset"}]},
                }
            }
        },
    )
    for wrong in _values(
        {},
        {"as": "subject"},
        {"token": True, "not_token": True},
        {"name": "a", "names": ["b"]},
    ):
        _check("A.event-subject-filter", wrong, accepted=False)
    _check(
        "A.event-subject-filter", {"as": "subject", "where": {"standing": "subject"}}
    )


def test_subject_reference_and_filter_are_exclusive_across_history_levels() -> None:
    _check(
        EVENT, {"moved": {"ref": "this", "sel": {"kind": "follower"}}}, accepted=False
    )
    _check(
        QUERY,
        {
            "event": "moved",
            "window": "game",
            "sel": {"kind": "follower"},
            "where": {"ref": "this"},
        },
        accepted=False,
    )
    _check(EVENT, {"left_field": {"to": "cemetery"}}, accepted=False)
    _check(EVENT, {"damaged": {"ref": "this"}}, accepted=False)
    _check(EVENT, {"played": {"trait": "財宝"}}, accepted=False)


def test_event_recipient_unions_require_a_leader_and_use_event_roles() -> None:
    _check(
        "A.event-recipient",
        {"any_of": [{"leader": "self"}, {"who": "opp", "kind": "follower"}]},
    )
    for value in _values(
        {"any_of": [{"who": "self"}]},
        {"any_of": ["this", {"leader": "self"}]},
        {"who": {"param": "side"}},
        {"in": "t"},
    ):
        _check("A.event-recipient", value, accepted=False)


def test_chosen_and_material_filters_keep_their_own_contexts() -> None:
    _check(C, {"chosen": {"ref": "target", "where": {"hp_le": 3}}})
    _check(C, {"chosen": {"ref": "target"}})
    for wrong in _values(
        {"ref": "this"},
        {"ref": "target", "where": {}},
        {"ref": "target", "at": "selection"},
    ):
        _check(C, {"chosen": wrong}, accepted=False)
    _check(
        C,
        {"did": {"result": "moved", "where": {"not_class": "エルフ"}, "view": "after"}},
    )
    _check(
        C,
        {
            "did": {
                "result": "cost",
                "where": {"kind": "follower", "trait": "ウマ娘", "orig_cost_ge": 7},
            }
        },
    )
    _check(
        E,
        {
            "count_of": {
                "receipt": "shown",
                "where": {"orig_cost_eq": {"bound": "declared"}},
                "view": "before",
            }
        },
    )


@pytest.mark.parametrize("name", HISTORY)
def test_new_definition_drift_detects_constraint_removal(name: str) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    if "anyOf" in node:
        branches = node["anyOf"]
        assert isinstance(branches, list)
        branches.append({"type": "null"})
    elif "properties" in node:
        node["additionalProperties"] = True
    else:
        node["type"] = "null"
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


@pytest.mark.parametrize(
    ("name", "field", "replacement"),
    [
        ("A.event.phase-payload", "which", {"type": "null"}),
        ("A.history.phase-where", "which", {"$ref": "#/$defs/A.param"}),
        ("A.history.life-changed-sum", "field", {"const": "amount"}),
        ("A.capture.selection", "selection", {"$ref": "#/$defs/A.ref"}),
        ("A.capture.power-of", "power_of", {"$ref": "#/$defs/A.snapshot-read"}),
        ("A.die-eq", "eq", {"$ref": "#/$defs/Nat"}),
        ("A.cond.player-cmp-payload", "op", {"enum": ["eq", "ne", "lt"]}),
    ],
)
def test_new_role_drift_detects_type_widening(
    name: str, field: str, replacement: JsonValue
) -> None:
    changed = deepcopy(NODES)
    node = object_value(object_value(changed["$defs"])[name])
    object_value(node["properties"])[field] = replacement
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


def _union_leaves(node: JsonValue) -> list[dict[str, JsonValue]]:
    body = object_value(node)
    reference = body.get("$ref")
    if isinstance(reference, str):
        return _union_leaves(
            object_value(NODES["$defs"])[reference.removeprefix("#/$defs/")]
        )
    variants = body.get("anyOf")
    if isinstance(variants, list):
        return [leaf for variant in variants for leaf in _union_leaves(variant)]
    return [body]


@pytest.mark.parametrize(
    ("name", "value"),
    [
        *[(EVENT, {event: payload}) for event, payload in EVENT_CASES],
        *[("A.state-numeric-read", read) for read in STATE_READS],
        ("A.expr.capture", {"at": "event", "value": {"hand": "self"}}),
        (
            "A.expr.capture",
            {"at": "selection", "selection": "t", "value": {"hand": "self"}},
        ),
        ("A.event-recipient", "this"),
        ("A.event-recipient", {"leader": "self"}),
        ("A.event-recipient", {"who": "self", "kind": "follower"}),
        ("A.event-recipient", {"who": "self", "kind": "follower", "leader_or": True}),
        ("A.event-recipient", {"any_of": [{"leader": "self"}, {"kind": "follower"}]}),
        ("A.cond.cmp-payload", {"left": -1, "op": "eq", "right": 0}),
    ],
)
def test_new_unions_match_exactly_one_leaf(name: str, value: JsonValue) -> None:
    leaves = _union_leaves({"$ref": "#/$defs/" + name})
    assert (
        sum(
            Draft202012Validator(
                {"$id": NODES["$id"], "$defs": NODES["$defs"], **leaf}
            ).is_valid(value)
            for leaf in leaves
        )
        == 1
    )


@pytest.mark.parametrize(
    ("name", "keyword"),
    [
        ("A.event.enter-field-payload", "not"),
        ("A.history.moved", "not"),
        ("A.event-subject-filter", "dependentRequired"),
        ("A.event-subject-filter", "minProperties"),
        ("A.history.phase", "required"),
        ("A.history.life-changed-sum", "required"),
        ("A.history.life-sum-where", "required"),
        ("A.capture.selection", "required"),
        ("A.event.phase-payload", "required"),
    ],
)
def test_new_drift_detects_required_and_exclusive_constraint_removal(
    name: str, keyword: str
) -> None:
    changed = deepcopy(NODES)
    object_value(object_value(changed["$defs"])[name]).pop(keyword)
    with pytest.raises(AssertionError):
        assert_selector_definition(changed, name)


def test_counter_event_thresholds_are_natural_literals() -> None:
    _check(EVENT, {"counter_reached": {"ref": "this", "name": "stack", "value": 2}})
    for wrong in _values(-1, True, "X", {"plus": [1, 1]}):
        _check(
            EVENT,
            {"counter_reached": {"ref": "this", "name": "stack", "value": wrong}},
            accepted=False,
        )
        _check(EVENT, {"counter_changed": {"after": {"ge": wrong}}}, accepted=False)
    for payload in _values(
        {"ref": "this", "name": "stack"},
        {"ref": "this", "value": 1},
        {"name": "stack", "value": 1},
    ):
        _check(EVENT, {"counter_reached": payload}, accepted=False)


@pytest.mark.parametrize(
    ("name", "field", "value"),
    [
        ("A.cond.paid", "paid", "additional"),
        ("A.cond.paid", "paid", "optional"),
        ("A.cond.paid", "paid", {"receipt": "additional"}),
        ("A.cond.paid", "paid", {"receipt": "optional"}),
        ("A.cond.paid", "paid", {"receipt": {"entry_payment": "additional"}}),
        (
            "A.cond.cmp",
            "cmp",
            {"left": {"player": "self"}, "op": "eq", "right": {"player": "opp"}},
        ),
        ("A.cond.cmp", "cmp", {"left": 1, "op": "eq", "right": 2}),
    ],
)
def test_new_operand_unions_have_one_matching_branch(
    name: str, field: str, value: JsonValue
) -> None:
    node = object_value(object_value(NODES["$defs"])[name])
    leaves = _union_leaves(object_value(node["properties"])[field])
    assert (
        sum(
            Draft202012Validator(
                {"$id": NODES["$id"], "$defs": NODES["$defs"], **leaf}
            ).is_valid(value)
            for leaf in leaves
        )
        == 1
    )


@pytest.mark.parametrize("quantity", ["X", {"bound": "cost"}, {"plus": [1, 1]}])
def test_material_cost_comparisons_accept_numeric_operations(
    quantity: JsonValue,
) -> None:
    _check("A.material-filter", {"orig_cost_eq": quantity})
    _check("A.material-filter", {"orig_cost_ge": quantity})


@pytest.mark.parametrize(
    ("event", "field", "values", "wrong"),
    [
        ("damaged", "kind", ["attack", "combat", "ability"], "any"),
        ("dealt_damage", "kind", ["attack", "combat", "ability"], "effect"),
        ("enter_field", "entered_by", ["ability", "play"], "rule"),
        ("life_changed", "direction", ["up", "down"], "set"),
        ("stat_changed", "direction", ["up", "down"], "change"),
        ("counter_changed", "direction", ["up", "down"], "set"),
        ("stat_changed", "stat", ["power", "hp"], "attack"),
        ("drew", "not_phase", ["start", "main", "end"], "draw"),
    ],
)
def test_event_and_history_filter_enums_match_the_per_event_matrix(
    event: str, field: str, values: list[str], wrong: str
) -> None:
    for value in [*values, wrong]:
        accepted = value != wrong
        _check(EVENT, {event: {field: value}}, accepted=accepted)
        _check(
            QUERY,
            {"event": event, "window": "game", "where": {field: value}},
            accepted=accepted,
        )


@pytest.mark.parametrize(
    ("event", "payload"),
    [
        ("damaged", {"recipient": {"in": "t"}}),
        ("attacked", {"who": "opponents"}),
        ("played", {"sel": {"who": "self"}}),
        ("stat_changed", {"stat": []}),
        ("moved", {"from": {"zone": "hand"}}),
        ("moved", {"from": {"who": "self"}}),
        ("moved", {"from": {"who": "self", "zone": "hand", "unknown": True}}),
        ("discarded", {"from": "ex"}),
        ("drive_trigger", {"holder": "this"}),
        ("equipped", {"equipment": {"leader": "self"}}),
        ("fused", {"material": {"in": "t"}}),
    ],
)
def test_direct_events_reject_other_roles_and_incomplete_origins(
    event: str, payload: JsonValue
) -> None:
    _check(EVENT, {event: payload}, accepted=False)


@pytest.mark.parametrize(
    ("source", "name", "value"),
    CARD_HISTORY_CASES + SYNTAX_HISTORY_CASES,
    ids=[source for source, _, _ in CARD_HISTORY_CASES + SYNTAX_HISTORY_CASES],
)
def test_card_and_syntax_fragments_round_trip_in_one_leaf(
    source: str, name: str, value: JsonValue
) -> None:
    assert source
    _check(name, value)
    test_new_unions_match_exactly_one_leaf(name, value)

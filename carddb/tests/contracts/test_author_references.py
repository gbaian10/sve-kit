"""Synthetic reference fragments from author syntax §§6, 9, 13 and 14."""

from copy import deepcopy
from itertools import combinations, product
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ConfigDict, TypeAdapter, ValidationError

from sve_carddb.contracts import author_references as author
from sve_carddb.core.json import canonical, object_value, parse

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMA_PATH = Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"
CONFIG = ConfigDict(regex_engine="python-re")
ADAPTERS: dict[str, TypeAdapter[object]] = {
    "BindingName": TypeAdapter(author.BindingName, config=CONFIG),
    "RoleName": TypeAdapter(author.RoleName, config=CONFIG),
    "A.follow-source": TypeAdapter(author.FollowSource, config=CONFIG),
    "A.receipt-ref": TypeAdapter(author.ReceiptRef, config=CONFIG),
    "A.ref": TypeAdapter(author.Ref, config=CONFIG),
    "A.player-ref": TypeAdapter(author.PlayerRef, config=CONFIG),
    "A.player-set": TypeAdapter(author.PlayerSet, config=CONFIG),
    "A.leader-recipient": TypeAdapter(author.LeaderRecipient, config=CONFIG),
    "A.until": TypeAdapter(author.Until, config=CONFIG),
}

# These are contract examples, not values extracted from either implementation.
RESERVED = [
    ("this", True, False, False),
    ("it", True, False, False),
    ("item", True, True, False),
    ("affected", True, False, False),
    ("found", True, False, False),
    ("entering", True, False, False),
    ("attack_target", True, False, False),
    ("host", True, False, False),
    ("cost1", True, False, False),
    ("self", False, True, False),
    ("opp", False, True, False),
    ("turn", False, True, False),
    ("non_turn", False, True, False),
    ("current", False, True, False),
    ("opponents", False, False, True),
    ("each", False, False, True),
    ("other_players", False, False, True),
]
OBJECTS: list[tuple[str, dict[str, JsonValue]]] = [
    ("A.follow-source", {"event": "move_receipt"}),
    ("A.receipt-ref", {"event": "move_receipt"}),
    ("A.receipt-ref", {"entry_payment": "additional"}),
    ("A.ref", {"follow": "moved_group"}),
    ("A.ref", {"follow": {"event": "move_receipt"}}),
    ("A.ref", {"role": {"of": "chosen_group", "name": "primary"}}),
    ("A.ref", {"role": {"of": "chosen_group", "name": "rest"}}),
    ("A.ref", {"event": "holder"}),
    ("A.player-ref", {"owner_of": "chosen_object"}),
    ("A.player-ref", {"master_of": "chosen_object"}),
    ("A.player-ref", {"master_of": {"follow": "moved_group"}}),
    ("A.player-ref", {"owner_of": {"event": "holder"}}),
    ("A.leader-recipient", {"leader": "self"}),
    ("A.leader-recipient", {"leader": "opp"}),
    ("A.leader-recipient", {"leaders": "opponents"}),
    ("A.leader-recipient", {"leaders": "all"}),
    ("A.until", {"next_phase": {"who": "self", "phase": "start", "edge": "begin"}}),
    ("A.until", {"next_turn": {"who": "opp", "edge": "end"}}),
    ("A.until", {"during_next_phase": {"who": "opp", "phase": "main"}}),
    (
        "A.until",
        {"next_turn": {"who": {"master_of": "chosen_object"}, "edge": "end"}},
    ),
]


def _validator(schema: dict[str, JsonValue], definition: str) -> Draft202012Validator:
    return Draft202012Validator(
        {"$defs": schema["$defs"], "$ref": "#/$defs/" + definition}
    )


@pytest.fixture(scope="module")
def schema() -> dict[str, JsonValue]:
    result = object_value(parse(SCHEMA_PATH.read_bytes()))
    Draft202012Validator.check_schema(result)
    return result


def _accept(schema: dict[str, JsonValue], definition: str, value: JsonValue) -> None:
    assert _validator(schema, definition).is_valid(value)
    adapter = ADAPTERS[definition]
    decoded = adapter.validate_json(canonical(value), strict=True)
    assert adapter.dump_python(decoded, mode="json") == value


def _reject(schema: dict[str, JsonValue], definition: str, value: JsonValue) -> None:
    assert not _validator(schema, definition).is_valid(value)
    with pytest.raises(ValidationError):
        ADAPTERS[definition].validate_json(canonical(value), strict=True)


def _reserved_roles(
    schema: dict[str, JsonValue],
    name: str,
    object_ref: bool,
    player: bool,
    players: bool,
) -> None:
    for definition, expected in (
        ("A.ref", object_ref),
        ("A.player-ref", player),
        ("A.player-set", players),
        ("BindingName", False),
        ("A.follow-source", False),
        ("A.receipt-ref", False),
    ):
        assert _validator(schema, definition).is_valid(name) == expected
        if expected:
            _accept(schema, definition, name)
        else:
            _reject(schema, definition, name)


@pytest.mark.parametrize(("name", "object_ref", "player", "players"), RESERVED)
def test_reserved_names_keep_their_roles(
    schema: dict[str, JsonValue],
    name: str,
    object_ref: bool,
    player: bool,
    players: bool,
) -> None:
    _reserved_roles(schema, name, object_ref, player, players)


@pytest.mark.parametrize("name", ["chosen_group", "moved_all", "Alias_2", "X", "on"])
@pytest.mark.parametrize(
    "definition",
    [
        "BindingName",
        "RoleName",
        "A.ref",
        "A.player-ref",
        "A.receipt-ref",
        "A.follow-source",
    ],
)
def test_names_preserve_spelling(
    schema: dict[str, JsonValue], definition: str, name: str
) -> None:
    _accept(schema, definition, name)


@pytest.mark.parametrize("name", ["rest", "primary", "item", "opp"])
def test_role_namespace_is_separate(schema: dict[str, JsonValue], name: str) -> None:
    _accept(schema, "RoleName", name)
    _accept(schema, "A.ref", {"role": {"of": "chosen_group", "name": name}})


@pytest.mark.parametrize(("definition", "value"), OBJECTS)
def test_closed_reference_variants(
    schema: dict[str, JsonValue], definition: str, value: dict[str, JsonValue]
) -> None:
    _accept(schema, definition, value)


@pytest.mark.parametrize(
    "duration", ["turn", "next_opp_turn", "this_and_next_opp_turn", "game"]
)
def test_short_durations(schema: dict[str, JsonValue], duration: str) -> None:
    _accept(schema, "A.until", duration)


@pytest.mark.parametrize(
    ("phase", "edge"), product(["start", "main", "end"], ["begin", "end"])
)
@pytest.mark.parametrize(
    "who",
    ["self", "opp", "turn", "non_turn", "current", "item", "player_alias"],
)
def test_phase_and_turn_boundaries(
    schema: dict[str, JsonValue], phase: str, edge: str, who: str
) -> None:
    _accept(
        schema, "A.until", {"next_phase": {"who": who, "phase": phase, "edge": edge}}
    )
    _accept(schema, "A.until", {"next_turn": {"who": who, "edge": edge}})
    _accept(schema, "A.until", {"during_next_phase": {"who": who, "phase": phase}})


def _objects(value: dict[str, JsonValue]) -> list[dict[str, JsonValue]]:
    result = [value]
    for child in value.values():
        if isinstance(child, dict):
            result.extend(_objects(child))
    return result


@pytest.mark.parametrize(("definition", "value"), OBJECTS)
def test_every_mapping_is_closed_and_required(
    schema: dict[str, JsonValue], definition: str, value: dict[str, JsonValue]
) -> None:
    for offset, node in enumerate(_objects(value)):
        for key in node:
            changed = deepcopy(value)
            del _objects(changed)[offset][key]
            _reject(schema, definition, changed)
        changed = deepcopy(value)
        _objects(changed)[offset]["unexpected"] = "ignored"
        _reject(schema, definition, changed)


@pytest.mark.parametrize(("definition", "value"), OBJECTS)
@pytest.mark.parametrize("invalid", [None, False, 3, [], {"param": "p"}])
def test_fields_reject_wrong_types_and_parameters(
    schema: dict[str, JsonValue],
    definition: str,
    value: dict[str, JsonValue],
    invalid: JsonValue,
) -> None:
    for offset, node in enumerate(_objects(value)):
        for key in node:
            changed = deepcopy(value)
            _objects(changed)[offset][key] = invalid
            _reject(schema, definition, changed)


@pytest.mark.parametrize(
    "definition",
    ["A.receipt-ref", "A.ref", "A.player-ref", "A.leader-recipient", "A.until"],
)
def test_variants_cannot_be_combined(
    schema: dict[str, JsonValue], definition: str
) -> None:
    samples = [value for name, value in OBJECTS if name == definition]
    for left, right in combinations(samples, 2):
        if left.keys().isdisjoint(right):
            _reject(schema, definition, left | right)


@pytest.mark.parametrize("definition", ADAPTERS)
@pytest.mark.parametrize(
    "value", [None, False, 1, [], {}, {"param": "p"}, {"generated_by": {}}]
)
def test_roots_reject_wrong_types_and_expansion_nodes(
    schema: dict[str, JsonValue], definition: str, value: JsonValue
) -> None:
    _reject(schema, definition, value)


@pytest.mark.parametrize(
    "name",
    [
        "",
        "_t",
        "1t",
        "t-name",
        "t.name",
        "t/name",
        " t",
        "t ",
        "t\n",
        "t\r\n",
        "t\u0000",
        "名字",
    ],
)
@pytest.mark.parametrize(
    "definition", ["BindingName", "RoleName", "A.ref", "A.player-ref", "A.receipt-ref"]
)
def test_identifier_boundaries(
    schema: dict[str, JsonValue], definition: str, name: str
) -> None:
    _reject(schema, definition, name)


@pytest.mark.parametrize(
    ("definition", "value"),
    [
        ("A.ref", {"event": "move_receipt"}),
        ("A.ref", {"event": "amount"}),
        ("A.ref", {"event": "value"}),
        ("A.ref", {"event": "equipment"}),
        ("A.ref", {"entry_payment": "additional"}),
        ("A.ref", {"follow": "this"}),
        ("A.ref", {"follow": {"entry_payment": "additional"}}),
        ("A.ref", {"role": {"of": "this", "name": "primary"}}),
        ("A.ref", {"role": {"of": {"follow": "moved_group"}, "name": "rest"}}),
        ("A.ref", {"leader": "self"}),
        ("A.ref", {"in": "chosen_group", "kind": "follower"}),
        ("A.ref", "first:chosen_group"),
        ("A.ref", {"first": "chosen_group"}),
        ("A.receipt-ref", {"event": "holder"}),
        ("A.receipt-ref", {"entry_payment": "optional"}),
        ("A.receipt-ref", {"follow": "moved_group"}),
        ("A.follow-source", {"entry_payment": "additional"}),
        ("A.player-ref", {"owner_of": {"leader": "self"}}),
        ("A.player-ref", {"master_of": "opponents"}),
        ("A.player-ref", {"event": "holder"}),
        ("A.player-set", "self"),
        ("A.player-set", "all"),
        ("A.leader-recipient", {"leader": "current"}),
        ("A.leader-recipient", {"leader": "opponents"}),
        ("A.leader-recipient", {"leader": {"master_of": "chosen_group"}}),
        ("A.leader-recipient", {"leaders": "opp"}),
        ("A.until", {"next_turn": {"who": "self", "edge": "end", "phase": "end"}}),
        (
            "A.until",
            {"during_next_phase": {"who": "self", "phase": "start", "edge": "begin"}},
        ),
        (
            "A.until",
            {"next_phase": {"who": "opponents", "phase": "start", "edge": "begin"}},
        ),
        (
            "A.until",
            {"next_phase": {"who": "self", "phase": "combat", "edge": "begin"}},
        ),
        ("A.until", {"next_turn": {"who": "self", "edge": "after"}}),
        ("A.until", "next_start_phase"),
        ("A.until", "next_main_phase"),
        ("A.until", "next_owner_turn_end"),
    ],
)
def test_wrong_context_and_unregistered_shapes(
    schema: dict[str, JsonValue], definition: str, value: JsonValue
) -> None:
    _reject(schema, definition, value)


def test_reserved_name_probe_detects_overlapping_union(
    schema: dict[str, JsonValue],
) -> None:
    changed = deepcopy(schema)
    binding = object_value(object_value(changed["$defs"])["BindingName"])
    pattern = binding["pattern"]
    assert isinstance(pattern, str)
    binding["pattern"] = pattern.replace("|opponents", "")
    with pytest.raises(AssertionError):
        _reserved_roles(changed, "opponents", False, False, True)


@pytest.mark.parametrize(
    "value",
    ["this", {"follow": "moved_group"}, {"next_turn": {"who": "self", "edge": "end"}}],
)
def test_library_root_is_not_a_document_entry(
    schema: dict[str, JsonValue], value: JsonValue
) -> None:
    assert not Draft202012Validator(schema).is_valid(value)

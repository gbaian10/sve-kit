"""Target role shapes keep declarations separate from binding admission."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical, object_value, parse

from .author_selection_cases import ROLES, SELECTIONS

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMA_PATH = Path(__file__).resolve().parents[3] / "dsl" / "author-nodes.schema.json"


@pytest.fixture(scope="module")
def schema() -> dict[str, JsonValue]:
    result = object_value(parse(SCHEMA_PATH.read_bytes()))
    Draft202012Validator.check_schema(result)
    return result


def _check(
    schema: dict[str, JsonValue],
    definition: str,
    value: JsonValue,
    *,
    accepted: bool,
) -> None:
    validator = Draft202012Validator(
        {
            "$id": schema["$id"],
            "$defs": schema["$defs"],
            "$ref": "#/$defs/" + definition,
        }
    )
    assert validator.is_valid(value) == accepted
    adapter = SELECTIONS[definition]
    if accepted:
        decoded = adapter.validate_json(canonical(value), strict=True)
        assert adapter.dump_python(decoded, mode="json") == value
    else:
        with pytest.raises(ValidationError):
            adapter.validate_json(canonical(value), strict=True)


@pytest.mark.parametrize(("definition", "value"), ROLES)
def test_role_variants_round_trip(
    schema: dict[str, JsonValue], definition: str, value: JsonValue
) -> None:
    _check(schema, definition, value, accepted=True)


@pytest.mark.parametrize(
    "value",
    [None, False, True, 0, 2, -1, "1", {}, [], {"param": "n"}, "remaining"],
)
def test_role_one_requires_the_exact_integer(
    schema: dict[str, JsonValue], value: JsonValue
) -> None:
    cases: list[tuple[str, JsonValue]] = [
        ("A.target.role-one", {"if_nonempty": value}),
        ("A.target.role-value", {"if_nonempty": value}),
        ("A.target.roles", {"primary": {"if_nonempty": value}}),
    ]
    for definition, sample in cases:
        _check(schema, definition, sample, accepted=False)


@pytest.mark.parametrize(
    "value",
    [
        {},
        {"if_nonempty": 1, "remaining": True},
        {"if_nonempty": 1, "param": "n"},
        {"n": 1},
        {"upto": 1},
        {"any": True},
        {"role": {"of": "t", "name": "primary"}},
    ],
)
def test_role_values_reject_missing_discriminator_and_mixed_context(
    schema: dict[str, JsonValue], value: JsonValue
) -> None:
    _check(schema, "A.target.role-value", value, accepted=False)
    _check(schema, "A.target.roles", {"primary": value}, accepted=False)


@pytest.mark.parametrize(
    "name", ["", "_t", "1t", "t-name", "t.name", "t/name", "t\n", "t\r\n", "角色"]
)
def test_role_names_are_complete_identifiers(
    schema: dict[str, JsonValue], name: str
) -> None:
    _check(schema, "A.target.roles", {name: "remaining"}, accepted=False)


@pytest.mark.parametrize("definition", SELECTIONS)
@pytest.mark.parametrize(
    "value", [None, False, True, 1, [], {}, {"param": "n"}, {"leader_of": "this"}]
)
def test_role_roots_reject_other_contexts(
    schema: dict[str, JsonValue], definition: str, value: JsonValue
) -> None:
    _check(schema, definition, value, accepted=False)


@pytest.mark.parametrize(
    ("definition", "raw"),
    [
        ("A.target.role-one", b'{"if_nonempty":1.0}'),
        ("A.target.role-value", b'{"if_nonempty":1.0}'),
        ("A.target.roles", b'{"primary":{"if_nonempty":1.0}}'),
    ],
    ids=["one", "value", "roles"],
)
def test_float_literals_do_not_bypass_strict_entry(definition: str, raw: bytes) -> None:
    with pytest.raises(ValueError, match="Floating point"):
        parse(raw)
    with pytest.raises(ValidationError):
        SELECTIONS[definition].validate_json(raw, strict=True)


def test_role_map_order_does_not_create_an_ordered_view(
    schema: dict[str, JsonValue],
) -> None:
    left: dict[str, JsonValue] = {"primary": {"if_nonempty": 1}, "rest": "remaining"}
    right = dict(reversed(left.items()))
    for value in (left, right):
        _check(schema, "A.target.roles", value, accepted=True)
    assert canonical(left) == canonical(right)


@pytest.mark.parametrize(("definition", "value"), ROLES)
def test_role_nodes_are_not_document_entries(
    schema: dict[str, JsonValue], definition: str, value: JsonValue
) -> None:
    _check(schema, definition, value, accepted=True)
    assert not Draft202012Validator(schema).is_valid(value)

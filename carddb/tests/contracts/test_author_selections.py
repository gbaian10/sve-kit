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
    [{}, {"if_nonempty": 1, "remaining": True}],
)
def test_role_values_reject_missing_discriminator_and_mixed_context(
    schema: dict[str, JsonValue], value: JsonValue
) -> None:
    _check(schema, "A.target.role-value", value, accepted=False)
    _check(schema, "A.target.roles", {"primary": value}, accepted=False)


def test_role_map_order_does_not_create_an_ordered_view(
    schema: dict[str, JsonValue],
) -> None:
    left: dict[str, JsonValue] = {"primary": {"if_nonempty": 1}, "rest": "remaining"}
    right = dict(reversed(left.items()))
    for value in (left, right):
        _check(schema, "A.target.roles", value, accepted=True)
    assert canonical(left) == canonical(right)

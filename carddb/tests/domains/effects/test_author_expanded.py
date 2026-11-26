"""The expanded profile shares syntax while independently excluding macro leaves."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from ...contracts.test_author_abilities import HEADS
from .test_author_entries import NESTED, ability, document

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"
NODES = "urn:sve-kit:author-nodes:1.0"


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


def profile_validator(schema: dict[str, JsonValue]) -> Draft202012Validator:
    registry = Registry().with_resource(NODES, Resource.from_contents(schema))
    return Draft202012Validator({"$ref": NODES + "#/$defs/expanded"}, registry=registry)


@pytest.mark.parametrize(("key", "payload"), HEADS.items())
def test_expanded_accepts_each_concrete_head(
    schemas: AuthorSchemas, key: str, payload: JsonValue
) -> None:
    value = document(ability({key: payload}))
    assert schemas.expanded(canonical(value)).model_dump(mode="json") == value


@pytest.mark.parametrize("head", [head for head, _ in NESTED])
def test_recursive_profile_rejects_parameters(
    schemas: AuthorSchemas, head: dict[str, JsonValue]
) -> None:
    value = document(ability(head))
    schema = object_value(parse((SCHEMAS / "author-nodes.schema.json").read_bytes()))
    assert not profile_validator(schema).is_valid(value)
    with pytest.raises(AuthorInputError) as error:
        schemas.expanded(canonical(value))
    assert error.value.diagnostic.stage == "schema"

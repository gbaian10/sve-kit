"""The expanded profile shares syntax while independently excluding macro leaves."""

from copy import deepcopy
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from referencing import Registry, Resource

from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from .author_entry_cases import source_use
from .test_author_entries import HEADS, NESTED, ability, companion, document, shard

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


def generated() -> dict[str, JsonValue]:
    return {
        "line": 1,
        "sources": [source_use()],
        "generated_by": {
            "macro": {
                "frame_id": "frame:" + "1" * 64,
                "ability_scope": "ability_0",
                "applicability_scope": "fixture",
            },
            "version": "effect-body-v1:sha256:" + "2" * 64,
            "params": {},
        },
    }


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


@pytest.mark.parametrize("face", [False, True])
def test_generated_branch_is_only_available_before_expansion(
    schemas: AuthorSchemas, face: bool
) -> None:
    value = document(generated())
    if face:
        card = object_value(object_value(value["cards"])["FIX-001"])
        card["faces"] = {"f:" + "1" * 32: {"abilities": card.pop("abilities")}}
    schemas.author_effect_set(canonical(value))
    with pytest.raises(AuthorInputError) as error:
        schemas.expanded(canonical(value))
    assert error.value.diagnostic.stage == "schema"


@pytest.mark.parametrize("residual", ["authorParam", "authorGenerated"])
def test_decoder_detects_broken_dynamic_override(tmp_path: Path, residual: str) -> None:
    for filename in (
        "author-nodes.schema.json",
        "effects-index.schema.json",
        "author.schema.json",
        "effect-bodies.schema.json",
        "effects.schema.json",
    ):
        (tmp_path / filename).write_bytes((SCHEMAS / filename).read_bytes())
    nodes_path = tmp_path / "author-nodes.schema.json"
    schema = object_value(parse(nodes_path.read_bytes()))
    profile = object_value(object_value(schema["$defs"])["expanded"])
    override = object_value(object_value(profile["$defs"])[residual])
    del override["not"]
    nodes_path.write_bytes(canonical(schema))
    value = document(
        ability(NESTED[-2][0]) if residual == "authorParam" else generated()
    )
    assert profile_validator(schema).is_valid(value)
    with pytest.raises(AuthorInputError) as error:
        AuthorSchemas(tmp_path).expanded(canonical(value))
    assert error.value.diagnostic.reason == (
        "expanded_param_reference"
        if residual == "authorParam"
        else "expanded_generated_by"
    )


def test_metadata_spellings_are_not_syntax(schemas: AuthorSchemas) -> None:
    value = document(ability({"keywords": ["param", "generated_by"]}))
    card = object_value(object_value(value["cards"])["FIX-001"])
    metadata = object_value(card["meta"])
    metadata["written_by"] = [
        {"actor": {"who": "param", "model": None}, "date": "2026-10-21"}
    ]
    assert schemas.expanded(canonical(value)).model_dump(mode="json") == value


@pytest.mark.parametrize("body", [False, True])
def test_original_trees_preserve_optional_fields(
    schemas: AuthorSchemas, body: bool
) -> None:
    record = companion({"spell": {"do": []}})
    original = shard(record) if body else document(ability({"spell": {"do": []}}))
    reader = schemas.body_tree if body else schemas.author_tree
    assert reader(canonical(original)) == original
    changed = deepcopy(original)
    if body:
        records = changed["records"]
        assert isinstance(records, list)
        head = object_value(object_value(records[0])["body"])
        object_value(head["spell"])["define"] = {"X": 1}
    else:
        card = object_value(object_value(changed["cards"])["FIX-001"])
        abilities = card["abilities"]
        assert isinstance(abilities, list)
        object_value(abilities[0])["section"] = 0
    assert reader(canonical(changed)) == changed
    assert canonical(reader(canonical(original))) != canonical(
        reader(canonical(changed))
    )

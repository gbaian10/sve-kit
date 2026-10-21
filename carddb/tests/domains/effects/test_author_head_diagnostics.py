"""Failed ability unions report source locations without broadening acceptance."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from .test_author_entries import HEADS, ability, companion, document, shard

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"
ABILITY_PATH = ("cards", "FIX-001", "abilities", 0)


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


@pytest.mark.parametrize(("head", "payload"), HEADS.items())
def test_registered_heads_conflict_before_unknown_keys(
    schemas: AuthorSchemas, head: str, payload: JsonValue
) -> None:
    other = "spell" if head == "quick" else "quick"
    entry = ability({head: payload, other: HEADS[other], "unknown": "private_value"})
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(canonical(document(entry)))
    diagnostic = caught.value.diagnostic
    assert diagnostic.reason == "ability_head_conflict"
    assert diagnostic.path == ABILITY_PATH
    assert diagnostic.source is not None
    assert "private_value" not in str(caught.value)


@pytest.mark.parametrize(
    ("head", "reason", "suffix"),
    [
        ({"fanfar": {"do": []}}, "additionalProperties", ("fanfar",)),
        ({}, "ability_head_missing", ()),
        ({"quick": True, "unknown": None}, "additionalProperties", ("unknown",)),
        ({"quick": True, "generated_by": {}}, "ability_head_conflict", ()),
    ],
)
@pytest.mark.parametrize("face", [False, True])
def test_ability_header_failures_keep_exact_container_paths(
    schemas: AuthorSchemas,
    head: dict[str, JsonValue],
    reason: str,
    suffix: tuple[str, ...],
    face: bool,
) -> None:
    entry = ability(head)
    value = document(entry)
    path: tuple[str | int, ...] = ABILITY_PATH
    if face:
        card = object_value(object_value(value["cards"])["FIX-001"])
        del card["abilities"]
        card["faces"] = {"f:" + "1" * 32: {"abilities": [entry]}}
        path = ("cards", "FIX-001", "faces", "f:" + "1" * 32, "abilities", 0)
    with pytest.raises(AuthorInputError) as caught:
        schemas.document(json.dumps(value, indent=2).encode())
    diagnostic = caught.value.diagnostic
    assert diagnostic.reason == reason
    assert diagnostic.path == (*path, *suffix)
    assert diagnostic.line is not None
    assert diagnostic.column is not None
    assert diagnostic.card_no == "FIX-001"


@pytest.mark.parametrize(
    ("body", "reason", "suffix"),
    [
        (
            {"quick": True, "spell": {"do": []}, "typo": None},
            "ability_head_conflict",
            (),
        ),
        ({"fanfar": {}}, "additionalProperties", ("fanfar",)),
        ({}, "ability_head_missing", ()),
    ],
)
def test_companion_uses_the_same_head_diagnostics(
    schemas: AuthorSchemas,
    body: dict[str, JsonValue],
    reason: str,
    suffix: tuple[str, ...],
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.effect_body_shard(canonical(shard(companion(body))))
    assert caught.value.diagnostic.reason == reason
    assert caught.value.diagnostic.path == ("records", 0, "body", *suffix)


def test_nested_granted_head_conflict_is_not_reported_as_an_outer_typo(
    schemas: AuthorSchemas,
) -> None:
    head: dict[str, JsonValue] = {
        "spell": {
            "do": [
                {
                    "grant": {
                        "on": "this",
                        "abilities": [{"fanfare": {"do": []}, "lastword": {"do": []}}],
                    }
                }
            ]
        }
    }
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(canonical(document(ability(head))))
    assert caught.value.diagnostic.reason == "ability_head_conflict"
    assert caught.value.diagnostic.path == (
        *ABILITY_PATH,
        "spell",
        "do",
        0,
        "grant",
        "abilities",
        0,
    )


def test_generated_payload_errors_stay_inside_authored_generated_branch(
    schemas: AuthorSchemas,
) -> None:
    entry = ability({"generated_by": {"unknown": "private_value"}})
    del entry["ports"]
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(canonical(document(entry)))
    assert caught.value.diagnostic.reason == "additionalProperties"
    assert caught.value.diagnostic.path == (*ABILITY_PATH, "generated_by", "unknown")
    assert "private_value" not in str(caught.value)


def test_schema_derived_heads_do_not_bypass_the_typed_reader(tmp_path: Path) -> None:
    for source in SCHEMAS.glob("*.schema.json"):
        (tmp_path / source.name).write_bytes(source.read_bytes())
    path = tmp_path / "author-nodes.schema.json"
    schema = json.loads(path.read_bytes())
    variant = schema["$defs"]["A.ability.fanfare"]
    variant["properties"]["future_head"] = variant["properties"].pop("fanfare")
    variant["required"] = [
        "future_head" if name == "fanfare" else name for name in variant["required"]
    ]
    path.write_text(json.dumps(schema))
    schemas = AuthorSchemas(tmp_path)
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(
            canonical(document(ability({"future_head": {"do": []}, "quick": True})))
        )
    assert caught.value.diagnostic.reason == "ability_head_conflict"
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(
            canonical(document(ability({"future_head": {"do": []}})))
        )
    assert caught.value.diagnostic.reason == "decoder_constraint"

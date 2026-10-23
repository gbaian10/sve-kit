"""Failed ability unions report source locations without broadening acceptance."""

import json
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from ...contracts.test_author_abilities import HEADS
from .test_author_entries import ability, document

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
    other = "spell" if head == "keywords" else "keywords"
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
        (
            {"keywords": ["quick"], "unknown": None},
            "additionalProperties",
            ("unknown",),
        ),
        ({"keywords": ["quick"], "generated_by": {}}, "ability_head_conflict", ()),
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

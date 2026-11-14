"""Both author decoders enforce the conditional construction domain."""

from typing import TYPE_CHECKING

import pytest
from jsonschema import Draft202012Validator
from pydantic import ValidationError

from sve_carddb.core.json import canonical

from .author_entry_types import ENTRIES
from .test_author_schema_drift import NODES

if TYPE_CHECKING:
    from pydantic import JsonValue

RULE: dict[str, JsonValue] = {
    "when": "self_in_main",
    "decks": ["main", "evolve"],
    "exclude_name_contains": "Synthetic",
    "same_name_max": 1,
}


@pytest.mark.parametrize(
    "decks", [["main"], ["evolve"], ["main", "evolve"], ["evolve", "main"]]
)
def test_each_nonempty_unique_section_selection(decks: list[JsonValue]) -> None:
    value = RULE | {"decks": decks}
    decoded = ENTRIES["CardDeckRule"].validate_json(canonical(value), strict=True)
    assert ENTRIES["CardDeckRule"].dump_python(decoded, mode="json") == value
    assert Draft202012Validator(
        {"$defs": NODES["$defs"], "$ref": "#/$defs/CardDeckRule"}
    ).is_valid(value)


@pytest.mark.parametrize(
    "change",
    [
        {"decks": []},
        {"decks": ["main", "main"]},
        {"decks": ["side"]},
        {"when": "self_in_evolve"},
        {"exclude_name_contains": ""},
        {"same_name_max": -1},
        {"deck": "main"},
    ],
)
def test_unsupported_or_ambiguous_rule_is_rejected(
    change: dict[str, JsonValue],
) -> None:
    value = RULE | change
    with pytest.raises(ValidationError):
        ENTRIES["CardDeckRule"].validate_json(canonical(value), strict=True)
    assert not Draft202012Validator(
        {"$defs": NODES["$defs"], "$ref": "#/$defs/CardDeckRule"}
    ).is_valid(value)

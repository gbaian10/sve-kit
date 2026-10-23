"""Inventory reads preserve strict syntax without admitting unsupported programs."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from .test_author_entries import ability, document

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


@pytest.mark.parametrize(
    "cards", [{}, {"FIX-001": None}, {"FIX-001": [False, "opaque"]}]
)
def test_en_inventory_only_exposes_keys_without_authorizing_values(
    schemas: AuthorSchemas, cards: dict[str, JsonValue]
) -> None:
    value: JsonValue = {
        "format": 1,
        "kind": "author_effect_set",
        "scope": "en_override",
        "cards": cards,
    }
    raw = canonical(value)
    assert schemas.author_inventory_tree(raw) == value
    for read in (schemas.author_tree, schemas.author_effect_set, schemas.document):
        with pytest.raises(AuthorInputError) as caught:
            read(raw)
        assert caught.value.diagnostic.reason == "en_source_contract_unavailable"


@pytest.mark.parametrize(
    ("changes", "path"),
    [({"format": True}, ("format",)), ({"format": 2}, ("format",))],
)
def test_inventory_rejects_bad_envelopes_at_exact_locations(
    schemas: AuthorSchemas, changes: dict[str, JsonValue], path: tuple[str, ...]
) -> None:
    value: JsonValue = {
        "format": 1,
        "kind": "author_effect_set",
        "scope": "en_override",
        "cards": {"FIX-001": None},
        **changes,
    }
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_inventory_tree(canonical(value))
    assert caught.value.diagnostic.path == path
    assert "private_value" not in str(caught.value)


def test_prototype_requires_its_own_explicit_entry(schemas: AuthorSchemas) -> None:
    value: JsonValue = {
        "format": 1,
        "kind": "effect_set",
        "cards": {"FIX-001": {"status": "unsupported", "abilities": []}},
    }
    raw = canonical(value)
    assert schemas.prototype_tree(raw) == value
    for read in (schemas.document, schemas.author_inventory_tree, schemas.author_tree):
        with pytest.raises(AuthorInputError):
            read(raw)
    with pytest.raises(AuthorInputError):
        schemas.prototype_tree(canonical(document(ability({"keywords": ["quick"]}))))

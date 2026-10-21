"""Inventory reads preserve strict syntax without admitting unsupported programs."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from .test_author_entries import ability, companion, document, shard

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


def test_shared_inventory_keeps_full_validation_and_original_tree(
    schemas: AuthorSchemas,
) -> None:
    value = document(ability({"spell": {"do": []}}))
    raw = canonical(value)
    assert schemas.author_inventory_tree(raw) == schemas.author_tree(raw) == value
    card = object_value(object_value(value["cards"])["FIX-001"])
    card["abilities"] = [ability({"spell": {"do": [{"draw": {"n": {"param": "N"}}}]}})]
    for read in (schemas.author_inventory_tree, schemas.author_tree):
        with pytest.raises(AuthorInputError) as caught:
            read(canonical(value))
        assert caught.value.diagnostic.reason == "inline_param_reference"


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
    [
        ({"format": True}, ("format",)),
        ({"format": 2}, ("format",)),
        ({"kind": "effect_set"}, ("kind",)),
        ({"scope": "jp"}, ("scope",)),
        ({"extra": "private_value"}, ("extra",)),
        ({"cards": []}, ("cards",)),
        ({"cards": {"": None}}, ("cards", "")),
    ],
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


@pytest.mark.parametrize("field", ["format", "kind", "scope", "cards"])
def test_inventory_requires_complete_envelope(
    schemas: AuthorSchemas, field: str
) -> None:
    value: dict[str, JsonValue] = {
        "format": 1,
        "kind": "author_effect_set",
        "scope": "en_override",
        "cards": {},
    }
    del value[field]
    with pytest.raises(AuthorInputError):
        schemas.author_inventory_tree(canonical(value))


@pytest.mark.parametrize(
    "raw",
    [
        b"{format: 1, kind: author_effect_set, scope: en_override, cards: {X: 1, X: 2}}",
        b"{format: 1, kind: author_effect_set, scope: en_override, cards: {1: null}}",
        b"{format: 1, kind: author_effect_set, scope: en_override, cards: {X: .nan}}",
    ],
    ids=repr,
)
def test_opaque_inventory_still_requires_strict_json(
    schemas: AuthorSchemas, raw: bytes
) -> None:
    with pytest.raises(AuthorInputError):
        schemas.author_inventory_tree(raw)


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
        schemas.prototype_tree(canonical(document(ability({"quick": True}))))


@pytest.mark.parametrize(
    "raw",
    [
        b"{format: true, kind: effect_set, cards: {X: {status: complete, abilities: []}}}",
        b"{format: 1, kind: effect_set, cards: {X: {status: future, abilities: []}}}",
        b"{format: 1, kind: effect_set, cards: {X: {status: complete}}}",
        b"{format: 1, kind: effect_set, cards: {}, scope: shared}",
        b"{format: 1, kind: effect_set, cards: {}, cards: {}}",
    ],
    ids=repr,
)
def test_prototype_inventory_is_strict(schemas: AuthorSchemas, raw: bytes) -> None:
    with pytest.raises(AuthorInputError):
        schemas.prototype_tree(raw)


@pytest.mark.parametrize("reader", ["author_inventory_tree", "prototype_tree"])
def test_inventory_errors_keep_index_context(
    schemas: AuthorSchemas, reader: str
) -> None:
    context = AuthorIndexRecord(
        card_id="c:" + "a" * 32,
        record_key="FIX-001",
        file="rules/author-effects/FIX/001.yaml",
        scope="shared",
        kind="author_effect_set",
    )
    with pytest.raises(AuthorInputError) as caught:
        getattr(schemas, reader)(b"{format: private_value}", context=context)
    diagnostic = caught.value.diagnostic
    assert diagnostic.card_id == context.card_id
    assert diagnostic.card_no == context.record_key
    assert diagnostic.scope == context.scope
    assert "private_value" not in str(caught.value)


@pytest.mark.parametrize(
    "kind", ["author_effect_set", "effect_body_shard", "effects_index"]
)
@pytest.mark.parametrize("valid", [False, True])
def test_document_matches_specific_reader(
    schemas: AuthorSchemas, kind: str, valid: bool
) -> None:
    values: dict[str, JsonValue] = {
        "author_effect_set": document(ability({"quick": True})),
        "effect_body_shard": shard(companion({"spell": {"do": []}})),
        "effects_index": {"format": 1, "kind": "effects_index", "records": []},
    }
    value = object_value(values[kind])
    if not valid:
        value["extra"] = "private_value"
    raw = canonical(value)
    read = getattr(schemas, kind)
    if valid:
        assert schemas.document(raw) == read(raw)
    else:
        with pytest.raises(AuthorInputError) as generic:
            schemas.document(raw)
        with pytest.raises(AuthorInputError) as specific:
            read(raw)
        assert generic.value.diagnostic == specific.value.diagnostic
        assert "private_value" not in str(generic.value)


def test_document_dispatch_preserves_selected_context(schemas: AuthorSchemas) -> None:
    context = AuthorIndexRecord(
        card_id="c:" + "b" * 32,
        record_key="FIX-001",
        file="rules/author-effects/FIX/001.yaml",
        scope="shared",
        kind="author_effect_set",
    )
    raw = canonical(document(ability({"fanfar": {"do": []}})))
    with pytest.raises(AuthorInputError) as generic:
        schemas.document(raw, context=context)
    with pytest.raises(AuthorInputError) as specific:
        schemas.author_effect_set(raw, context=context)
    assert generic.value.diagnostic == specific.value.diagnostic
    assert generic.value.diagnostic.card_id == context.card_id

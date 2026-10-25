"""Keyword declarations have exact scope-free sources, without current admission."""

from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from pydantic import TypeAdapter, ValidationError

from sve_carddb.contracts.author_entries import KeywordsAbility
from sve_carddb.contracts.author_evidence import CardKeywordSourceUse, SourceUse
from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_current import CardRejected
from sve_carddb.domains.effects.author_expansion import AuthorExpander, ExpansionError
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from ...support.author_current_fixtures import current_source_fixture
from .author_entry_cases import source_use
from .author_source_cases import synthetic_offset
from .test_author_current import author, current, inputs, validate_files
from .test_author_entries import SCHEMAS, ability, companion, document, shard

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


def test_keyword_source_is_not_an_ability_source(schemas: AuthorSchemas) -> None:
    entry = ability({"keywords": ["ward", "aura"]})
    reference = object_value(array(entry["sources"])[0])
    declared = CardKeywordSourceUse.model_validate_json(canonical(reference))
    assert declared.model_dump(mode="json") == reference
    with pytest.raises(ValidationError):
        SourceUse.model_validate_json(canonical(reference))
    for face in (False, True):
        value = document(deepcopy(entry))
        if face:
            card = object_value(object_value(value["cards"])["FIX-001"])
            card["faces"] = {"f:" + "1" * 32: {"abilities": card.pop("abilities")}}
        assert schemas.author_tree(canonical(value)) == value
        assert schemas.expanded(canonical(value)).model_dump(mode="json") == value
    expanded = AuthorExpander(SCHEMAS).expand(canonical(document(entry)), ())
    assert expanded.sidecar == ()
    assert expanded.declarations[0].source_proof == "none"


@pytest.mark.parametrize(
    "change",
    ["old-source", "imports", "export-port", "import-port", "duplicate", "generated"],
)
def test_keyword_declaration_has_one_closed_shape(
    schemas: AuthorSchemas, change: str
) -> None:
    entry = ability({"keywords": ["ward", "aura"]})
    if change == "old-source":
        entry["sources"] = [source_use()]
    elif change == "imports":
        entry["imports"] = {
            "external": {
                "binding_id": "binding:fixture",
                "ability_scope": "ability_0",
                "port": "p",
            }
        }
    elif change == "export-port":
        object_value(entry["ports"])["exports"] = {"p": {"path": [], "bind": "x"}}
    elif change == "import-port":
        object_value(entry["ports"])["imports"] = {"p": {"path": [], "bind": "x"}}
    elif change == "duplicate":
        entry["keywords"] = ["ward", "ward"]
    else:
        entry["generated_by"] = {}
    with pytest.raises(ValidationError):
        TypeAdapter(KeywordsAbility).validate_json(canonical(entry), strict=True)
    for reader in (schemas.author_tree, schemas.expanded):
        with pytest.raises(AuthorInputError):
            reader(canonical(document(entry)))


def test_plain_ability_still_requires_its_real_scope(schemas: AuthorSchemas) -> None:
    entry = ability({"spell": {"do": []}})
    object_value(array(entry["sources"])[0]).pop("ability_scope")
    for reader in (schemas.author_tree, schemas.expanded):
        with pytest.raises(AuthorInputError):
            reader(canonical(document(entry)))


def test_unreferenced_keyword_companion_is_rejected(schemas: AuthorSchemas) -> None:
    value = shard(companion({"keywords": ["ward"]}))
    with pytest.raises(AuthorInputError):
        schemas.body_tree(canonical(value))
    with pytest.raises(AuthorInputError):
        AuthorExpander(SCHEMAS).expand(
            canonical(document(ability({"spell": {"do": []}}))),
            (canonical(value),),
        )


@pytest.mark.parametrize("status", ["reviewed", "verified", "disputed"])
def test_keyword_status_cannot_claim_unsupported_evidence(
    schemas: AuthorSchemas, status: str
) -> None:
    value = document(ability({"keywords": ["ward"]}))
    card = object_value(object_value(value["cards"])["FIX-001"])
    object_value(card["meta"])["status"] = status
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_tree(canonical(value))
    assert caught.value.diagnostic.reason == "unsupported_keyword_review_evidence"


def test_body_review_cannot_qualify_a_card_with_keyword_declarations(
    schemas: AuthorSchemas,
) -> None:
    value = document(ability({"spell": {"do": []}}))
    card = object_value(object_value(value["cards"])["FIX-001"])
    metadata = object_value(card["meta"])
    recipe: dict[str, JsonValue] = {"id": "fixture", "hash": "sha256:" + "1" * 64}
    metadata["reviewed_by"] = [
        {
            "kind": "use",
            "reviewer": {"who": "synthetic reviewer", "model": None},
            "date": "2026-10-26",
            "verdict": "ok",
            "reason": None,
            "dependencies": {
                "sources": [source_use()],
                "frame_id": "frame:" + "1" * 64,
                "ability_scope": "ability_0",
                "semantic_variant": {
                    "state": "resolved",
                    "key": "fixture.v1",
                    "scope": None,
                },
                "body_version": "effect-body-v1:sha256:" + "1" * 64,
                "interfaces": [],
                "source": metadata["source"],
                "rulings": [],
                "expansion_recipe": recipe,
                "adapter_recipe": recipe,
                "imports": {},
                "applicability_scope": None,
                "domain_hash": None,
                "params": {},
            },
        }
    ]
    schemas.author_tree(canonical(value))
    card["abilities"] = [ability({"keywords": ["ward"]})]
    for reader in (schemas.author_tree, schemas.expanded):
        with pytest.raises(AuthorInputError) as caught:
            reader(canonical(value))
        assert caught.value.diagnostic.reason == "unsupported_keyword_review_evidence"


def test_keyword_structure_does_not_grant_source_mapping() -> None:
    raw = canonical(document(ability({"keywords": ["ward", "aura"]})))
    expander = AuthorExpander(SCHEMAS)
    with pytest.raises(ExpansionError, match="unsupported_keyword_source_mapping"):
        expander.expand_sources(raw, (), {})
    with pytest.raises(ExpansionError, match="source_declaration_candidates_mismatch"):
        expander.expand_contexts(raw, (), {})
    with pytest.raises(ExpansionError, match="source_candidates_mismatch"):
        expander.expand_contexts(
            raw,
            (),
            {("cards", "FIX-001", "abilities", 0): synthetic_offset().compatible()},
        )


def test_keyword_inline_parameter_diagnostic_keeps_its_source(
    schemas: AuthorSchemas,
) -> None:
    entry = ability({"keywords": [{"param": "K"}]})
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_tree(canonical(document(entry)))
    assert caught.value.diagnostic.reason == "inline_param_reference"
    assert isinstance(caught.value.diagnostic.source, CardKeywordSourceUse)


def test_current_keyword_cannot_borrow_a_real_body_source(tmp_path: Path) -> None:
    fixture = current_source_fixture(tmp_path / "sources")
    with current(fixture) as sources:
        card = author(sources)
        node = object_value(array(card["abilities"])[0])
        node.pop("evolve")
        node["keywords"] = ["ward"]
        object_value(array(node["sources"])[0]).pop("ability_scope")
        result = validate_files(
            sources, inputs({"BP01-009": card}), AuthorExpander(SCHEMAS)
        )[0]
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason == "source_keyword_occurrence_mismatch"

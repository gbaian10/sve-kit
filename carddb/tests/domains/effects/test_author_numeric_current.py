"""Public adopted Fanfare buff sources prove complete current-card admission."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.contracts.four_layer import FaceRevisionOwner
from sve_carddb.core.json import array, object_value, parse
from sve_carddb.domains.effects.author_current import CardAccepted, CardRejected
from sve_carddb.domains.effects.author_set import AuthorInputSet
from sve_carddb.domains.effects.author_source_hash import source_version
from sve_carddb.domains.effects.author_sources import SourceContext

from ...support.author_numeric_fixtures import FRAME
from ...support.author_semantic_inputs import load_semantic_inputs
from ..translations.test_four_layer_fixed_actions import NUMERIC_RAW as RAW
from .author_current_synthetic import effect, token_section
from .author_entry_cases import meta
from .test_author_current import current, input_file, validate_files, write_inputs
from .test_author_entries import shard
from .test_author_expansion import generated
from .test_author_numeric_sources import CHANGES, changed_head, companion, head

CARD = "c:5dc2ea15a3a55a559c27a4f630c2e333"

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources
    from sve_carddb.domains.effects.author_expansion import AuthorExpander

    from ...support.author_current_fixtures import CurrentSourceFixture


def inputs(number: str, card: JsonValue) -> AuthorInputSet:
    file = input_file(
        "rules/author-effects/BP01/000.yaml",
        {
            "format": 1,
            "kind": "author_effect_set",
            "scope": "shared",
            "cards": {number: card},
        },
    )
    index = input_file(
        "rules/effects-index.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [
                {
                    "card_id": CARD,
                    "scope": "shared",
                    "file": file.path,
                    "kind": "author_effect_set",
                    "record_key": number,
                }
            ],
        },
    )
    return AuthorInputSet(
        EffectsIndex.model_validate_json(index.raw), index, (file,), (), ()
    )


def test_real_numeric_current_boundary(
    numeric_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    tmp_path: Path,
) -> None:
    with current(numeric_current_fixture) as source:
        complete = source.card_payload(CARD)
        assert set(complete.revisions.values()) == {
            "f:199fa75484985d99a482b0054cc968f7"
        }
        matched = complete.fields[0].matches[0]
        assert matched is not None
        assert matched.frame.id == FRAME
        owner = matched.binding.occurrence_key().owner
        assert isinstance(owner, FaceRevisionOwner)
        assert owner.revision_id in complete.revisions
        use = SourceUse(
            binding_id=matched.binding.id,
            occurrence=matched.binding.occurrence_key(),
            ability_scope="ability_0",
        )
        metadata = meta()
        metadata["source"] = source_version(complete.payload)
        node: dict[str, JsonValue] = {
            "line": 1,
            "sources": [use.model_dump(mode="json")],
            "ports": {"imports": {}, "exports": {}},
            **head(),
        }
        card: dict[str, JsonValue] = {"meta": metadata, "abilities": [node]}
        for number in ("BP01-P09", "BP01-037"):
            results = validate_files(source, inputs(number, card), expander)
            assert len(results) == 1
            result = results[0]
            assert isinstance(result, CardAccepted)
            assert result.candidate.sidecar[0].source_proof == "current_source_checked"
            assert result.candidate.sidecar[0].frame_id == FRAME
            inventory = object_value(parse(result.source_inventory))
            assert inventory["abilities"] == [
                {
                    "source_use": use.model_dump(mode="json"),
                    "body": {**head(), "ports": {"imports": {}, "exports": {}}},
                }
            ]
            assert inventory["non_body"] == []
            assert object_value(result.lowering_input())["card_id"] == CARD
            assert (
                object_value(result.lowering_input())["source_inventory"] == inventory
            )
        record = companion(
            SourceContext(
                matched.frame,
                matched.binding,
                complete.fields[0].field.parts[0].canonical_source,
                source.classifier,
            )
        )
        reference = generated(record, {})
        reference["sources"] = [use.model_dump(mode="json")]
        collection = replace(
            inputs("BP01-P09", {"meta": metadata, "abilities": [reference]}),
            body_files=(
                input_file("rules/effect-bodies/BP01/000.yaml", shard(record)),
            ),
        )
        result = validate_files(source, collection, expander)[0]
        assert isinstance(result, CardAccepted)
        assert result.candidate.sidecar[0].body_version == record["body_version"]
        check_face_container(source, expander, card)
        check_rejected_payloads(source, expander, node, metadata)
        check_source_closure(source, expander, card)
        check_selection_input(source, expander, card, tmp_path / "authored")
        check_unsupported_source_siblings(
            numeric_current_fixture, expander, card, owner.revision_id
        )


def check_face_container(
    source: CurrentAuthorSources,
    expander: AuthorExpander,
    card: dict[str, JsonValue],
) -> None:
    changed = deepcopy(card)
    abilities = changed.pop("abilities")
    changed["faces"] = {"f:199fa75484985d99a482b0054cc968f7": {"abilities": abilities}}
    result = validate_files(source, inputs("BP01-037", changed), expander)[0]
    assert isinstance(result, CardAccepted)
    changed["faces"] = {"f:" + "0" * 32: {"abilities": abilities}}
    result = validate_files(source, inputs("BP01-037", changed), expander)[0]
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason == "source_face_container_mismatch"


def check_selection_input(
    source: CurrentAuthorSources,
    expander: AuthorExpander,
    card: dict[str, JsonValue],
    root: Path,
) -> None:
    write_inputs(root, inputs("BP01-P09", card))
    selected = object_value(
        source.selection_inputs(load_semantic_inputs(root, expander), expander)
    )
    assert selected["documents"] == []
    authors = array(selected["authors"])
    assert len(authors) == 1
    accepted = object_value(authors[0])
    assert accepted["status"] == "accepted"
    assert object_value(accepted["input"])["card_id"] == CARD
    changed = deepcopy(card)
    changed["abilities"] = []
    write_inputs(root, inputs("BP01-P09", changed))
    assert object_value(
        source.selection_inputs(load_semantic_inputs(root, expander), expander)
    )["authors"] == [
        {
            "status": "rejected",
            "card_no": "BP01-P09",
            "reason": "source_ability_closure_mismatch",
        }
    ]


def check_unsupported_source_siblings(
    fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    card: dict[str, JsonValue],
    revision: str,
) -> None:
    for raw, reason in (
        (RAW + "\n1枚引く。", "source_pending"),
        ("『仮token』\n" + RAW, "source_unresolved_issue"),
    ):
        with current(fixture) as source, pytest.MonkeyPatch.context() as patch:
            effect(source, patch, raw, revision=revision)
            result = validate_files(source, inputs("BP01-P09", card), expander)[0]
            assert isinstance(result, CardRejected)
            assert result.diagnostic.reason == reason
    with current(fixture) as source, pytest.MonkeyPatch.context() as patch:
        token_section(source, patch, revision, text="『仮トークン』" + RAW)
        result = validate_files(source, inputs("BP01-P09", card), expander)[0]
        assert isinstance(result, CardRejected)
        assert result.diagnostic.reason == "token_source_contract_unavailable"


def check_rejected_payloads(
    source: CurrentAuthorSources,
    expander: AuthorExpander,
    node: dict[str, JsonValue],
    metadata: JsonValue,
) -> None:
    for change in CHANGES:
        body = changed_head(change)
        changed = {key: value for key, value in node.items() if key != "fanfare"}
        changed.update(deepcopy(body))
        result = validate_files(
            source,
            inputs("BP01-P09", {"meta": metadata, "abilities": [changed]}),
            expander,
        )[0]
        assert isinstance(result, CardRejected)
        assert result.diagnostic.reason == "source_head_payload_mismatch"


def check_source_closure(
    source: CurrentAuthorSources, expander: AuthorExpander, card: dict[str, JsonValue]
) -> None:
    for change, reason in (
        ("missing", "source_ability_closure_mismatch"),
        ("duplicate", "duplicate_source_ability"),
        ("scope", "source_occurrence_mismatch"),
        ("role", "source_occurrence_mismatch"),
        ("owner", "source_current_owner_mismatch"),
        ("token-location", "source_occurrence_mismatch"),
    ):
        changed = deepcopy(card)
        abilities = array(changed["abilities"])
        if change == "missing":
            abilities.clear()
        elif change == "duplicate":
            abilities.append(deepcopy(abilities[0]))
        else:
            changed_use = object_value(array(object_value(abilities[0])["sources"])[0])
            occurrence = object_value(changed_use["occurrence"])
            if change == "scope":
                changed_use["ability_scope"] = "ability_1"
            elif change == "role":
                occurrence["role"] = "reminder"
            elif change == "owner":
                occurrence["owner"] = {
                    "kind": "face_revision",
                    "revision_id": "revision:old",
                }
            else:
                occurrence["role"] = "token_header"
        result = validate_files(source, inputs("BP01-P09", changed), expander)[0]
        assert isinstance(result, CardRejected)
        assert result.diagnostic.reason == reason

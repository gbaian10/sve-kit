"""Public parsed named-search inputs pass only through complete current validation."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.domains.effects.author_current import CardAccepted, CardRejected
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ability_nodes,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_set import AuthorInputSet
from sve_carddb.domains.effects.author_source_hash import source_version

from ...support.author_current_fixtures import (
    CurrentSourceFixture,
    current_source_fixture,
)
from ..translations.test_four_layer_fixed_actions import SEARCH_RAW
from .author_current_synthetic import effect, token_section
from .author_entry_cases import meta
from .author_search_cases import head, search_ability
from .test_author_current import current, input_file, write_inputs
from .test_author_entries import SCHEMAS
from .test_author_search_sources import bad_payload

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources

CARD = "c:e7b93196e1175515a42771a72c7b12da"
FACE = "f:e9d8deca215953a99d12bd07acd566fa"


@pytest.fixture(scope="module")
def fixture(tmp_path_factory: pytest.TempPathFactory) -> CurrentSourceFixture:
    return current_source_fixture(
        tmp_path_factory.mktemp("search-sources"),
        card_numbers=frozenset({"BP06-048", "BP06-P15"}),
    )


def collection(
    source: CurrentAuthorSources, *, companion: bool, number: str = "BP06-048"
) -> AuthorInputSet:
    complete = source._card_payload(CARD)
    matched = complete.fields[0].matches[0]
    assert matched is not None
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    entry, shards = search_ability(matched.frame, use, companion=companion)
    metadata = meta()
    metadata["source"] = source_version(complete.payload)
    file = input_file(
        "rules/author-effects/BP06/000.yaml",
        {
            "format": 1,
            "kind": "author_effect_set",
            "scope": "shared",
            "cards": {number: {"meta": metadata, "abilities": [entry]}},
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
    bodies = tuple(
        input_file("rules/effect-bodies/search/000.yaml", parse(raw)) for raw in shards
    )
    return AuthorInputSet(
        EffectsIndex.model_validate_json(index.raw), index, (file,), bodies, ()
    )


@pytest.mark.parametrize("number", ["BP06-048", "BP06-P15"])
@pytest.mark.parametrize("companion", [False, True])
def test_real_named_search_current(
    fixture: CurrentSourceFixture, tmp_path: Path, number: str, *, companion: bool
) -> None:
    expander = AuthorExpander(SCHEMAS)
    with current(fixture) as source:
        assert source._printing_card(number) == CARD
        write_inputs(tmp_path, collection(source, companion=companion, number=number))
        accepted = source.validate(tmp_path, expander)[0]
        assert isinstance(accepted, CardAccepted)
        expected = canonical(head() | {"ports": {"imports": {}, "exports": {}}})
        assert (
            equivalent_ability(next(ability_nodes(accepted.candidate.tree))[1])
            == expected
        )
        evidence = accepted.candidate.sidecar[0]
        assert evidence.source_proof == "current_source_checked"
        assert evidence.current_source is not None
        inventory = object_value(
            object_value(accepted.lowering_input())["source_inventory"]
        )
        obligations = array(inventory["abilities"])
        assert len(obligations) == 1
        required = object_value(obligations[0])
        assert canonical(required["body"]) == expected
        assert required["source_use"] == evidence.current_source.model_dump(mode="json")
        payload = object_value(parse(accepted.source_payload))
        faces = array(payload["faces"])
        assert len(faces) == 1
        assert object_value(faces[0])["face_id"] == FACE
        selection = object_value(source.selection_input(tmp_path, expander))
        selected = object_value(array(selection["authors"])[0])
        assert selected["status"] == "accepted"


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        ("missing", "source_ability_closure_mismatch"),
        ("owner", "source_current_owner_mismatch"),
        ("binding", "source_occurrence_mismatch"),
        ("line", "source_location_mismatch"),
        ("face", "source_face_container_mismatch"),
        ("scope", "source_occurrence_mismatch"),
        ("payload", "source_head_payload_mismatch"),
    ],
)
def test_current_search_rejects_incomplete_or_wrong_source(
    fixture: CurrentSourceFixture, tmp_path: Path, change: str, reason: str
) -> None:
    expander = AuthorExpander(SCHEMAS)
    with current(fixture) as source:
        inputs = collection(source, companion=False)
        write_inputs(tmp_path, inputs)
        tree = expander.schemas.author_tree(inputs.author_files[0].raw)
        card = object_value(object_value(object_value(tree)["cards"])["BP06-048"])
        node = object_value(array(card["abilities"])[0])
        use = object_value(array(node["sources"])[0])
        if change == "missing":
            card["abilities"] = []
        elif change == "owner":
            object_value(use["occurrence"])["owner"] = {
                "kind": "face_revision",
                "revision_id": "revision:wrong-owner",
            }
        elif change == "binding":
            use["binding_id"] = "bind:" + "f" * 64
        elif change == "line":
            node["line"] = 2
        elif change == "face":
            card["faces"] = {"f:" + "f" * 32: {"abilities": card.pop("abilities")}}
        elif change == "scope":
            use["ability_scope"] = "ability_1"
        else:
            node["lastword"] = bad_payload("up-to")["lastword"]
        (tmp_path / inputs.author_files[0].path).write_bytes(canonical(tree))
        rejected = source.validate(tmp_path, expander)[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == reason
        selected = object_value(
            array(object_value(source.selection_input(tmp_path, expander))["authors"])[
                0
            ]
        )
        assert selected["status"] == "rejected"


@pytest.mark.parametrize(
    "suffix", ["\n1枚引く。", "\n{守護}"], ids=["draw-sibling", "keyword-sibling"]
)
def test_search_does_not_drop_unmapped_siblings(
    fixture: CurrentSourceFixture,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    suffix: str,
) -> None:
    with current(fixture) as source:
        write_inputs(tmp_path, collection(source, companion=False))
        revision = next(iter(source._card_payload(CARD).revisions))
        effect(source, monkeypatch, SEARCH_RAW + suffix, revision=revision)
        rejected = source.validate(tmp_path, AuthorExpander(SCHEMAS))[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "source_pending"


def test_search_does_not_admit_a_token_section(
    fixture: CurrentSourceFixture, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as source:
        write_inputs(tmp_path, collection(source, companion=False))
        revision = next(iter(source._card_payload(CARD).revisions))
        token_section(
            source,
            monkeypatch,
            revision,
            text="『仮トークン』{ニュートラル}フォロワー{コスト1}{攻撃力}1/{体力}1\n"
            + SEARCH_RAW,
            kind="unknown",
        )
        rejected = source.validate(tmp_path, AuthorExpander(SCHEMAS))[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "token_source_contract_unavailable"

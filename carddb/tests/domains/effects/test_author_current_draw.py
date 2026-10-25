"""CP02-068 enters only through complete indexed current spell-source validation."""

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

from ...support.author_current_fixtures import current_source_fixture
from .author_current_synthetic import effect, token_section
from .author_draw_cases import head, spell_ability
from .author_entry_cases import meta
from .test_author_current import current, input_file, write_inputs
from .test_author_entries import SCHEMAS

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources


CARD = "c:0850c7b7bda15bbaa0cbd019e7fba60c"


def collection(source: CurrentAuthorSources, *, companion: bool) -> AuthorInputSet:
    complete = source._card_payload(CARD)
    matched = complete.fields[0].matches[0]
    assert matched is not None
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    entry, shards = spell_ability(matched.frame, use, companion=companion)
    metadata = meta()
    metadata["source"] = source_version(complete.payload)
    file = input_file(
        "rules/author-effects/CP02/000.yaml",
        {
            "format": 1,
            "kind": "author_effect_set",
            "scope": "shared",
            "cards": {"CP02-068": {"meta": metadata, "abilities": [entry]}},
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
                    "record_key": "CP02-068",
                }
            ],
        },
    )
    bodies = tuple(
        input_file("rules/effect-bodies/draw/000.yaml", parse(raw)) for raw in shards
    )
    return AuthorInputSet(
        EffectsIndex.model_validate_json(index.raw), index, (file,), bodies, ()
    )


def check_accepted(result: CardAccepted | CardRejected) -> None:
    assert isinstance(result, CardAccepted)
    expected = canonical(head() | {"ports": {"imports": {}, "exports": {}}})
    assert equivalent_ability(next(ability_nodes(result.candidate.tree))[1]) == expected
    evidence = result.candidate.sidecar[0]
    assert evidence.source_proof == "current_source_checked"
    inventory = object_value(object_value(result.lowering_input())["source_inventory"])
    required = object_value(array(inventory["abilities"])[0])
    assert evidence.current_source is not None
    assert required["source_use"] == evidence.current_source.model_dump(mode="json")
    assert canonical(required["body"]) == expected
    payload = object_value(parse(result.source_payload))
    face = object_value(array(payload["faces"])[0])
    assert face["face_id"] == "f:5090ff1ac6ec567691ddc8c4584ca8e1"
    attributes = object_value(face["attributes"])
    assert attributes["type_code"] == {"state": "known", "value": "spell"}
    assert attributes["cost"] == {"state": "known", "value": 4}
    for stat in ("attack", "defense"):
        assert attributes[stat] == {"state": "known", "value": None}


def check_mutation(
    source: CurrentAuthorSources,
    root: Path,
    inputs: AuthorInputSet,
    change: str,
    reason: str,
) -> None:
    expander = AuthorExpander(SCHEMAS)
    tree = expander.schemas.author_tree(inputs.author_files[0].raw)
    card = object_value(object_value(object_value(tree)["cards"])["CP02-068"])
    node = object_value(array(card["abilities"])[0])
    if change == "missing":
        card["abilities"] = []
    elif change == "count":
        node["spell"] = head(2)["spell"]
    elif change == "line":
        node["line"] = 2
    elif change == "face":
        card["faces"] = {"f:" + "f" * 32: {"abilities": card.pop("abilities")}}
    else:
        use = object_value(array(node["sources"])[0])
        if change == "binding":
            use["binding_id"] = "bind:" + "f" * 64
        else:
            object_value(use["occurrence"])["owner"] = {
                "kind": "face_revision",
                "revision_id": "revision:stale",
            }
    (root / inputs.author_files[0].path).write_bytes(canonical(tree))
    rejected = source.validate(root, expander)[0]
    assert isinstance(rejected, CardRejected)
    assert rejected.diagnostic.reason == reason


def check_unknown_attributes(source: CurrentAuthorSources, root: Path) -> None:
    revision = next(iter(source._card_payload(CARD).revisions))
    for attribute in ("cost",):
        with source.db.transaction():
            source.db.update("face_revision", {"id": revision}, {attribute: None})
        rejected = source.validate(root, AuthorExpander(SCHEMAS))[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "source_unknown"
        with source.db.transaction():
            source.db.update(
                "face_revision",
                {"id": revision},
                {attribute: 4},
            )


def test_real_current_spell_draw(tmp_path: Path) -> None:
    fixture = current_source_fixture(
        tmp_path / "sources", card_numbers=frozenset({"CP02-068"})
    )
    expander = AuthorExpander(SCHEMAS)
    with current(fixture) as source:
        for companion in (False, True):
            root = tmp_path / str(companion)
            inputs = collection(source, companion=companion)
            write_inputs(root, inputs)
            check_accepted(source.validate(root, expander)[0])
            selection = object_value(source.selection_input(root, expander))
            selected = object_value(array(selection["authors"])[0])
            assert selected["status"] == "accepted"
            for change, reason in (
                ("missing", "source_ability_closure_mismatch"),
                ("line", "source_location_mismatch"),
                ("binding", "source_occurrence_mismatch"),
                ("owner", "source_current_owner_mismatch"),
                ("face", "source_face_container_mismatch"),
            ):
                check_mutation(source, root, inputs, change, reason)
            if not companion:
                check_mutation(
                    source, root, inputs, "count", "source_head_payload_mismatch"
                )
            write_inputs(root, inputs)
            check_unknown_attributes(source, root)
        negative_root = tmp_path / "negative-sources"
        write_inputs(negative_root, collection(source, companion=False))
        check_unadmitted_current_sources(source, negative_root)


def check_unadmitted_current_sources(source: CurrentAuthorSources, root: Path) -> None:
    revision = next(iter(source._card_payload(CARD).revisions))
    original = source.db.select(
        "face_revision", ("effect_unit_id",), where={"id": revision}
    )[0].values["effect_unit_id"]
    expander = AuthorExpander(SCHEMAS)
    for text in (
        "3枚まで引く。",
        "相手は3枚引く。",
        "お互いに3枚引く。",
        "自分の場のフォロワーの数と同じ枚数を引く。",
        "自分の手札が3枚以下なら、3枚引く。",
        "手札1枚を捨てる：3枚引く。",
    ):
        with pytest.MonkeyPatch.context() as patch:
            effect(source, patch, text, revision=revision)
            rejected = source.validate(root, expander)[0]
            assert isinstance(rejected, CardRejected)
            assert rejected.diagnostic.reason in {
                "source_pending",
                "source_unresolved_issue",
            }
    with source.db.transaction():
        source.db.update(
            "face_revision", {"id": revision}, {"effect_unit_id": original}
        )
    with pytest.MonkeyPatch.context() as patch:
        token_section(
            source,
            patch,
            revision,
            text="『仮トークン』{ニュートラル}フォロワー{コスト1}{攻撃力}1/{体力}1\n3枚引く。",
            kind="unknown",
        )
        rejected = source.validate(root, expander)[0]
        assert isinstance(rejected, CardRejected)
        assert rejected.diagnostic.reason == "token_source_contract_unavailable"

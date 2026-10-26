"""Real adopted sources enter through the complete current author-set boundary."""

from contextlib import contextmanager
from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.domains.effects.author_current import (
    CardAccepted,
    CardRejected,
    CurrentAuthorSources,
)
from sve_carddb.domains.effects.author_set import AuthorInputSet, InputFile
from sve_carddb.domains.effects.author_source_hash import (
    AuthorSourcePayload,
    source_version,
)
from sve_carddb.domains.translations.four_layer_authored import FrameRecord
from sve_carddb.domains.translations.inputs import Snapshot

from ...support.semantic_definition_fixtures import with_definitions
from .author_current_goldens import FACTS, golden
from .author_current_synthetic import back_face, effect
from .author_entry_cases import meta
from .test_author_entries import SCHEMAS

if TYPE_CHECKING:
    from collections.abc import Generator
    from pathlib import Path

    from sve_carddb.domains.effects.author_expansion import AuthorExpander

    from ...support.author_current_fixtures import CurrentSourceFixture


CARDS = {
    "BP01-009": "c:9c5459446e0254818099800b4208cd96",
    "BP01-034": "c:b1d5ce73453a5efea8a2057e75743ea4",
    "BP01-092": "c:8c4b83cfcb625c0490eb8c66968f29dc",
}
REAL_AUTHOR_CARDS = {
    "BP01-037",
    "BP06-048",
    "BP01-046",
    "BP01-144",
    "BP01-176",
    "BP01-110",
    "BP01-171",
    "BP01-174",
    "BP03-115",
    "BP04-010",
    "BP04-075",
    "BP17-077",
    "BP18-058",
    "BP19-001",
    "BP19-P32",
    "BP19-050",
}
FRAME = "frame:f03820b9048f0bd911c528162f930b0e47d128493a84b4fe097f4c8a48608756"


@contextmanager
def current(
    base_current_fixture: CurrentSourceFixture,
) -> Generator[CurrentAuthorSources]:
    with base_current_fixture.template.copy() as db:
        yield CurrentAuthorSources(
            db,
            base_current_fixture.sources.stage(base_current_fixture.sources.build),
            base_current_fixture.snapshot,
            base_current_fixture.vocabulary,
            base_current_fixture.rules,
            base_current_fixture.jp_batch_id,
        )


def input_file(path: str, tree: JsonValue) -> InputFile:
    raw = canonical(tree)
    return InputFile(path, raw, digest(raw))


def inputs(
    cards: dict[str, JsonValue],
    *,
    bodies: tuple[InputFile, ...] = (),
    scope: str = "shared",
) -> AuthorInputSet:
    file = input_file(
        "rules/author-effects/BP01/000.yaml"
        if scope == "shared"
        else "rules/author-effects/BP01/001.yaml",
        {"format": 1, "kind": "author_effect_set", "scope": scope, "cards": cards},
    )
    index = input_file(
        "rules/effects-index.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [
                {
                    "card_id": CARDS[number],
                    "scope": scope,
                    "file": file.path,
                    "kind": "author_effect_set",
                    "record_key": number,
                }
                for number in cards
            ],
        },
    )
    return AuthorInputSet(
        EffectsIndex.model_validate_json(index.raw), index, (file,), bodies, ()
    )


def write_inputs(root: Path, collection: AuthorInputSet) -> None:
    files = [
        *collection.author_files,
        *collection.body_files,
        *collection.prototype_files,
    ]
    if collection.index_file is not None:
        files.append(collection.index_file)
    for file in files:
        path = root / file.path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(file.raw)


def validate_files(
    current: CurrentAuthorSources, collection: AuthorInputSet, expander: AuthorExpander
) -> tuple[CardAccepted | CardRejected, ...]:
    return current.validate_inputs(collection, expander)


def author(
    current: CurrentAuthorSources, number: str = "BP01-009"
) -> dict[str, JsonValue]:
    # Registry matches only construct the author input; independent goldens pin its hash.
    card = current.card_payload(CARDS[number])
    matched = card.fields[0].matches[0]
    assert matched is not None
    assert matched.frame.id == FRAME
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    metadata = meta()
    metadata["source"] = source_version(
        AuthorSourcePayload.model_validate_json(canonical(golden(number)))
    )
    return {
        "meta": metadata,
        "abilities": [
            {
                "line": 1,
                "sources": [use.model_dump(mode="json")],
                "ports": {"imports": {}, "exports": {}},
                "evolve": {
                    "cost": {
                        "pp": {
                            "BP01-009": 1,
                            "BP01-034": 2,
                            "BP01-092": 0,
                        }[number]
                    }
                },
            }
        ],
    }


def ability(card: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return object_value(array(card["abilities"])[0])


def rejected(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    card: dict[str, JsonValue],
    reason: str,
) -> CardRejected:
    result = validate_files(current, inputs({"BP01-009": card}), expander)
    assert len(result) == 1
    assert isinstance(result[0], CardRejected)
    assert result[0].diagnostic.reason == reason
    return result[0]


def check_real_current_entry_and_container(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    number: str,
    face_container: bool,
    prototypes: tuple[InputFile, ...],
) -> None:
    card = author(current, number)
    evolve = ability(card)["evolve"]
    if face_container:
        face = FACTS[number]["face"]
        card["faces"] = {face: {"abilities": card.pop("abilities")}}
    collection = replace(inputs({number: card}), prototype_files=prototypes)
    results = validate_files(current, collection, expander)
    assert len(results) == 1
    assert isinstance(results[0], CardAccepted)
    expected = canonical(golden(number))
    assert results[0].source_payload == expected
    assert results[0].source_version == source_version(
        AuthorSourcePayload.model_validate_json(expected)
    )
    evidence = results[0].candidate.sidecar[0]
    assert evidence.source_proof == "current_source_checked"
    assert evidence.input_hash == collection.author_files[0].input_hash
    assert evidence.frame_id == FRAME
    inventory = object_value(
        object_value(results[0].lowering_input())["source_inventory"]
    )
    required = object_value(array(inventory["abilities"])[0])
    assert evidence.current_source is not None
    assert required["source_use"] == evidence.current_source.model_dump(mode="json")
    assert object_value(required["body"])["evolve"] == evolve
    assert (
        object_value(
            object_value(object_value(results[0].candidate.tree)["cards"])[number]
        )["meta"]
        == card["meta"]
    )


def check_database_null_is_unknown_and_rejected(
    current: CurrentAuthorSources, expander: AuthorExpander, attribute: str
) -> None:
    card = author(current)
    original = current.db.select(
        "face_revision", (attribute,), where={"id": FACTS["BP01-009"]["revision"]}
    )[0].values[attribute]
    with current.db.transaction():
        current.db.update(
            "face_revision", {"id": FACTS["BP01-009"]["revision"]}, {attribute: None}
        )
    rejected(current, expander, card, "source_unknown")
    with current.db.transaction():
        current.db.update(
            "face_revision",
            {"id": FACTS["BP01-009"]["revision"]},
            {attribute: original},
        )


def check_complete_head_payload(
    current: CurrentAuthorSources, expander: AuthorExpander, head: dict[str, JsonValue]
) -> None:
    card = author(current)
    node = ability(card)
    del node["evolve"]
    node.update(head)
    rejected(current, expander, card, "source_head_payload_mismatch")


def check_exact_current_use(
    current: CurrentAuthorSources, expander: AuthorExpander, change: str, reason: str
) -> None:
    card = author(current)
    node = ability(card)
    use = object_value(array(node["sources"])[0])
    occurrence = object_value(use["occurrence"])
    if change == "hash":
        occurrence["source_hash"] = "f" * 64
    elif change == "binding":
        use["binding_id"] = "bind:" + "f" * 64
    elif change == "line":
        node["line"] = 2
    elif change == "owner":
        occurrence["owner"] = {"kind": "face_revision", "revision_id": "revision:old"}
    elif change == "meta":
        object_value(card["meta"])["source"] = "effect-source-v1:sha256:" + "f" * 64
    elif change == "missing":
        card["abilities"] = []
    elif change == "duplicate":
        face = next(iter(current.card_payload(CARDS["BP01-009"]).revisions.values()))
        card["faces"] = {face: {"abilities": [deepcopy(node)]}}
    elif change == "face":
        card["faces"] = {"f:" + "f" * 32: {"abilities": card.pop("abilities")}}
    else:
        card["deck_rule"] = {"deck": "main", "same_name_max": 9}
    rejected(current, expander, card, reason)


def check_mixed_same_file_retains_success(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    good, bad = author(current), author(current, "BP01-034")
    object_value(bad["meta"])["source"] = "effect-source-v1:sha256:" + "f" * 64
    result = validate_files(
        current, inputs({"BP01-009": good, "BP01-034": bad}), expander
    )
    assert isinstance(result[0], CardAccepted)
    assert isinstance(result[1], CardRejected)
    assert result[1].diagnostic.card_no == "BP01-034"


def check_synthetic_multiface_owner_controls_both_containers(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    face_container: bool,
) -> None:
    card = author(current)
    face, _ = back_face(current, monkeypatch)
    # Synthetic source setup has no independent adopted payload golden.
    complete = current.card_payload(CARDS["BP01-009"])
    field = next(field for field in complete.fields if field.source.face_id == face)
    matched = field.matches[0]
    assert matched is not None
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    back: JsonValue = {
        "line": 1,
        "sources": [use.model_dump(mode="json")],
        "ports": {"imports": {}, "exports": {}},
        "evolve": {"cost": {"pp": 2}},
    }
    if face_container:
        card["faces"] = {face: {"abilities": [back]}}
    else:
        array(card["abilities"]).append(back)
    object_value(card["meta"])["source"] = source_version(complete.payload)
    result = validate_files(current, inputs({"BP01-009": card}), expander)[0]
    assert isinstance(result, CardAccepted)
    assert len(result.candidate.sidecar) == 2
    assert (
        len(
            array(
                object_value(object_value(result.lowering_input())["source_inventory"])[
                    "abilities"
                ]
            )
        )
        == 2
    )


def check_current_evidence_replacement_drops_proof(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    result = validate_files(current, inputs({"BP01-009": author(current)}), expander)[0]
    assert isinstance(result, CardAccepted)
    evidence = result.candidate.sidecar[0]
    assert replace(evidence, input_hash="sha256:" + "f" * 64).source_proof == "none"
    with pytest.raises(TypeError, match="init=False"):
        replace(evidence, source_proof="current_source_checked")


def check_synthetic_sibling_root_closure(
    current: CurrentAuthorSources,
    base_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    family: str,
    change: str,
) -> None:
    raw = "{進化}{コスト１}:これは進化する。 " + (
        "{食事}{コスト２}:これは出走する。"
        if family == "feed"
        else "{憑依}{コスト２}:これはドライブを持つ。"
    )
    effect(current, monkeypatch, raw)
    # The compiled field is setup for a registry mutation, not the assertion.
    compiled = current.card_payload(CARDS["BP01-009"]).fields[0]
    frame, _ = compiled.derived.bind(0)
    original = next(
        record
        for record in base_current_fixture.snapshot.four_layer.records
        if isinstance(record, FrameRecord) and record.data.id == FRAME
    )
    record = original.model_copy(update={"data": frame})
    encoded = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [record.model_dump(mode="json", exclude_computed_fields=True)],
        }
    )
    snapshot = Snapshot(
        (),
        with_definitions(
            (("translations/templates/definitions/094.yaml", encoded, encoded),)
        ),
    )
    synthetic = CurrentAuthorSources(
        current.db,
        current.sources,
        snapshot,
        base_current_fixture.vocabulary,
        base_current_fixture.rules,
        base_current_fixture.jp_batch_id,
    )
    # Synthetic source setup has no independent adopted payload golden.
    complete = synthetic.card_payload(CARDS["BP01-009"])
    matched = complete.fields[0].matches[0]
    assert matched is not None
    abilities: list[JsonValue] = []
    for index, head in enumerate(("evolve", family)):
        use = SourceUse(
            binding_id=matched.binding.id,
            occurrence=matched.binding.occurrence_key(),
            ability_scope=f"ability_{index}",
        )
        abilities.append(
            {
                "line": 1,
                "sources": [use.model_dump(mode="json")],
                "ports": {"imports": {}, "exports": {}},
                head: {"cost": {"pp": 2 - index if change == "swapped" else index + 1}},
            }
        )
    if change == "times":
        object_value(object_value(abilities[1])["feed"])["times"] = 2
    elif change == "missing":
        abilities.pop()
    metadata = meta()
    metadata["source"] = source_version(complete.payload)
    result = validate_files(
        synthetic,
        inputs({"BP01-009": {"meta": metadata, "abilities": abilities}}),
        expander,
    )[0]
    if change == "none":
        assert isinstance(result, CardAccepted)
        assert len(result.candidate.sidecar) == 2
        assert [
            object_value(object_value(item)["source_use"])["ability_scope"]
            for item in array(
                object_value(object_value(result.lowering_input())["source_inventory"])[
                    "abilities"
                ]
            )
        ] == ["ability_0", "ability_1"]
    else:
        assert isinstance(result, CardRejected)
        assert result.diagnostic.reason == (
            "source_ability_closure_mismatch"
            if change == "missing"
            else "source_head_payload_mismatch"
        )


@pytest.mark.slow(
    reason="Checks every committed prototype shard against real current author sources once; "
    "retains the repository-wide prototype/author overlap regression (#1104)."
)
def test_real_current_author_set(
    base_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    real_author_inputs: AuthorInputSet,
) -> None:
    real = real_author_inputs
    assert real.index_file is not None
    record_keys = {record.record_key for record in real.index.records}
    assert record_keys == REAL_AUTHOR_CARDS | {
        "BP01-P25",
        "BP10-122",
        "CP02-068",
        "ECP01-002",
    }
    assert all(record.scope == "shared" for record in real.index.records)
    assert all(record.kind == "author_effect_set" for record in real.index.records)
    assert {file.path for file in real.author_files} == {
        "rules/author-effects/BP01/000.yaml",
        "rules/author-effects/BP01/001.yaml",
        "rules/author-effects/BP01/002.yaml",
        "rules/author-effects/BP01/003.yaml",
        "rules/author-effects/BP01/004.yaml",
        "rules/author-effects/BP01/005.yaml",
        "rules/author-effects/BP03/001.yaml",
        "rules/author-effects/BP04/000.yaml",
        "rules/author-effects/BP04/001.yaml",
        "rules/author-effects/BP17/001.yaml",
        "rules/author-effects/BP18/001.yaml",
        "rules/author-effects/BP19/000.yaml",
        "rules/author-effects/BP19/001.yaml",
        "rules/author-effects/BP06/000.yaml",
        "rules/author-effects/BP10/000.yaml",
        "rules/author-effects/CP02/000.yaml",
        "rules/author-effects/ECP01/000.yaml",
    }
    assert {file.path for file in real.body_files} == {
        "rules/effect-bodies/BP01/000.yaml",
        "rules/effect-bodies/BP01/001.yaml",
        "rules/effect-bodies/BP06/000.yaml",
        "rules/effect-bodies/BP10/000.yaml",
        "rules/effect-bodies/CP02/000.yaml",
        "rules/effect-bodies/ECP01/000.yaml",
        "rules/effect-bodies/BP19/000.yaml",
    }
    assert {file.path for file in real.prototype_files} == {
        path.relative_to(SCHEMAS.parent / "authored").as_posix()
        for path in (SCHEMAS.parent / "authored/rules/effects").glob("*.yaml")
    }
    assert real.prototype_files
    with current(base_current_fixture) as source:
        for index, number in enumerate(CARDS):
            check_real_current_entry_and_container(
                source,
                expander,
                number,
                index == 1,
                real.prototype_files if index == 0 else (),
            )
        check_mixed_same_file_retains_success(source, expander)
        check_current_evidence_replacement_drops_proof(source, expander)


def test_current_author_rejects_incomplete_or_wrong_source(
    base_current_fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(base_current_fixture) as source:
        heads: tuple[dict[str, JsonValue], ...] = (
            {"spell": {"do": []}},
            {"evolve": {"cost": {"pp": 2}}},
            {"evolve": {"cost": {"steps": [{"pp": 1}, {"ep": 1}]}}},
        )
        for head in heads:
            check_complete_head_payload(source, expander, head)
        for change, reason in (
            ("hash", "source_occurrence_mismatch"),
            ("binding", "source_occurrence_mismatch"),
            ("line", "source_location_mismatch"),
            ("owner", "source_current_owner_mismatch"),
            ("meta", "source_version_mismatch"),
            ("missing", "source_ability_closure_mismatch"),
            ("duplicate", "duplicate_source_ability"),
            ("face", "source_face_container_mismatch"),
            ("deck", "unsupported_source_deck_rule"),
        ):
            check_exact_current_use(source, expander, change, reason)


def test_current_author_requires_known_attributes(
    base_current_fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(base_current_fixture) as source:
        for attribute in ("cost", "attack", "defense"):
            check_database_null_is_unknown_and_rejected(source, expander, attribute)


def test_synthetic_current_author_preserves_face_owners(
    base_current_fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    for face_container in (False, True):
        with (
            current(base_current_fixture) as source,
            pytest.MonkeyPatch.context() as patch,
        ):
            check_synthetic_multiface_owner_controls_both_containers(
                source, expander, patch, face_container
            )


def test_synthetic_current_author_preserves_root_sibling_closure(
    base_current_fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    for family, change in (
        ("feed", "none"),
        ("feed", "missing"),
        ("feed", "swapped"),
        ("feed", "times"),
        ("ride", "none"),
    ):
        with (
            current(base_current_fixture) as source,
            pytest.MonkeyPatch.context() as patch,
        ):
            check_synthetic_sibling_root_closure(
                source, base_current_fixture, expander, patch, family, change
            )

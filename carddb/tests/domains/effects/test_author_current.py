"""Real adopted sources enter through the complete current author-set boundary."""

from copy import deepcopy
from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.domains.effects.author_current import (
    CardAccepted,
    CardRejected,
    CurrentAuthorSources,
)
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ExpansionError,
    body_version,
)
from sve_carddb.domains.effects.author_inputs import AuthorInputError
from sve_carddb.domains.effects.author_set import (
    AuthorInputSet,
    InputFile,
    load_author_set,
)
from sve_carddb.domains.effects.author_source_hash import (
    AuthorSourcePayload,
    source_version,
)
from sve_carddb.domains.translations.four_layer_authored import FrameRecord
from sve_carddb.domains.translations.inputs import Snapshot

from ...support.author_current_fixtures import (
    CurrentSourceFixture,
    current_source_fixture,
)
from .author_current_goldens import FACTS, golden
from .author_current_synthetic import back_face, effect, section
from .author_entry_cases import meta
from .test_author_entries import SCHEMAS, companion, shard
from .test_author_expansion import generated

if TYPE_CHECKING:
    from collections.abc import Iterator


CARDS = {
    "BP01-009": "c:9c5459446e0254818099800b4208cd96",
    "BP01-034": "c:b1d5ce73453a5efea8a2057e75743ea4",
    "BP01-092": "c:8c4b83cfcb625c0490eb8c66968f29dc",
}
FRAME = "frame:8ebd201e08a1e14f57b700f2fb76dc11afbf7f16261350a117e2ca75f1aac18c"
IDENTITIES = CARDS | {
    "BP01-110": "c:9c467cdec10851c3958323fe461496dd",
    "BP01-076": "c:31103e7d8c6b5a2d9e3838cac19b1c07",
    "BP01-020": "c:5a26f30fff6c59c4ba36ffe785afbe17",
    "BP08-038": "c:c780e65fdb615e298840ce5be62553a7",
    "CP04-060": "c:446aae79e53156c0875a89bd7c3bb6bc",
}


@pytest.fixture(scope="module")
def fixture(tmp_path_factory: pytest.TempPathFactory) -> CurrentSourceFixture:
    return current_source_fixture(tmp_path_factory.mktemp("current-source-store"))


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


@pytest.fixture
def current(fixture: CurrentSourceFixture) -> Iterator[CurrentAuthorSources]:
    with fixture.template.copy() as db:
        yield CurrentAuthorSources(
            db,
            fixture.sources,
            fixture.snapshot,
            fixture.vocabulary,
            fixture.rules,
            fixture.jp_batch_id,
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
                    "card_id": IDENTITIES[number],
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
    with TemporaryDirectory() as temporary:
        root = Path(temporary)
        write_inputs(root, collection)
        return current.validate(root, expander)


def author(
    current: CurrentAuthorSources, number: str = "BP01-009"
) -> dict[str, JsonValue]:
    # Registry matches only construct the author input; independent goldens pin its hash.
    card = current._card_payload(IDENTITIES[number])
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
        if number in CARDS
        else card.payload  # BP01-110 only seeds the prototype-overlap regression.
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
                            "BP01-110": 1,
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


@pytest.mark.parametrize("number", CARDS)
@pytest.mark.parametrize("face_container", [False, True])
def test_real_current_entry_and_container(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    number: str,
    face_container: bool,
) -> None:
    card = author(current, number)
    if face_container:
        face = FACTS[number]["face"]
        card["faces"] = {face: {"abilities": card.pop("abilities")}}
    collection = inputs({number: card})
    results = validate_files(current, collection, expander)
    assert len(results) == 1
    assert isinstance(results[0], CardAccepted)
    evidence = results[0].candidate.sidecar[0]
    assert evidence.source_proof == "current_source_checked"
    assert evidence.input_hash == collection.author_files[0].input_hash
    assert evidence.frame_id == FRAME
    assert (
        object_value(
            object_value(object_value(results[0].candidate.tree)["cards"])[number]
        )["meta"]
        == card["meta"]
    )


@pytest.mark.parametrize("number", CARDS)
def test_independent_real_payload_goldens(
    current: CurrentAuthorSources, expander: AuthorExpander, number: str
) -> None:
    expected = canonical(golden(number))
    result = validate_files(
        current, inputs({number: author(current, number)}), expander
    )[0]
    assert isinstance(result, CardAccepted)
    assert result.source_payload == expected
    assert result.source_version == source_version(
        AuthorSourcePayload.model_validate_json(expected)
    )


@pytest.mark.parametrize("attribute", ["class_code", "cost", "attack", "defense"])
def test_database_null_is_unknown_and_rejected(
    current: CurrentAuthorSources, expander: AuthorExpander, attribute: str
) -> None:
    card = author(current)
    with current.db.transaction():
        current.db.update(
            "face_revision", {"id": FACTS["BP01-009"]["revision"]}, {attribute: None}
        )
    rejected(current, expander, card, "source_unknown")


@pytest.mark.parametrize(
    "head",
    [
        {"spell": {"do": []}},
        {"feed": {"cost": {"pp": 1}}},
        {"evolve": {"cost": {"pp": 2}}},
        {"evolve": {"cost": {"steps": [{"pp": 1}, {"ep": 1}]}}},
        {"evolve": {"cost": {"steps": [{"pp": 1}, {"leader_life": 1}]}}},
        {"evolve": {"cost": {"pp": {"plus": [0, 1]}}}},
        {"evolve": {"cost": {"pp": 1}, "usable_if": {"my_turn": True}}},
    ],
    ids=[
        "wrong-spell",
        "wrong-feed",
        "wrong-cost",
        "extra-ep",
        "extra-life",
        "expression",
        "usable-if",
    ],
)
def test_complete_head_payload(
    current: CurrentAuthorSources, expander: AuthorExpander, head: dict[str, JsonValue]
) -> None:
    card = author(current)
    node = ability(card)
    del node["evolve"]
    node.update(head)
    rejected(current, expander, card, "source_head_payload_mismatch")


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        ("hash", "source_occurrence_mismatch"),
        ("binding", "source_occurrence_mismatch"),
        ("scope", "source_occurrence_mismatch"),
        ("line", "source_location_mismatch"),
        ("owner", "source_current_owner_mismatch"),
        ("meta", "source_version_mismatch"),
        ("missing", "source_ability_closure_mismatch"),
        ("duplicate", "duplicate_source_ability"),
        ("face", "source_face_container_mismatch"),
        ("deck", "unsupported_source_deck_rule"),
    ],
)
def test_exact_current_use(
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
    elif change == "scope":
        use["ability_scope"] = "ability_1"
    elif change == "line":
        node["line"] = 2
    elif change == "owner":
        occurrence["owner"] = {"kind": "face_revision", "revision_id": "revision:old"}
    elif change == "meta":
        object_value(card["meta"])["source"] = "effect-source-v1:sha256:" + "f" * 64
    elif change == "missing":
        card["abilities"] = []
    elif change == "duplicate":
        face = next(iter(current._card_payload(CARDS["BP01-009"]).revisions.values()))
        card["faces"] = {face: {"abilities": [deepcopy(node)]}}
    elif change == "face":
        card["faces"] = {"f:" + "f" * 32: {"abilities": card.pop("abilities")}}
    else:
        card["deck_rule"] = {"deck": "main", "same_name_max": 9}
    rejected(current, expander, card, reason)


@pytest.mark.parametrize(
    "owner",
    [
        {"kind": "printing_face", "printing_id": "p:other", "face_id": "f:other"},
        {"kind": "qa_version", "qa_version_id": "qa:other"},
        {"kind": "cr_clause", "cr_clause_id": "cr:other"},
        {"kind": "vocabulary", "vocabulary_kind": "trait", "vocabulary_code": "elf"},
        {"kind": "product", "product_id": "product:other"},
        {"kind": "keyword", "keyword_id": "keyword:other"},
        {"kind": "product_family", "product_family_id": "product:other"},
    ],
)
def test_owner_variants_are_not_current_faces(
    current: CurrentAuthorSources, expander: AuthorExpander, owner: dict[str, JsonValue]
) -> None:
    card = author(current)
    occurrence = object_value(
        object_value(array(ability(card)["sources"])[0])["occurrence"]
    )
    occurrence["owner"] = owner
    if owner["kind"] == "qa_version":
        occurrence["field"] = "answer"
    elif owner["kind"] in {"vocabulary", "product", "product_family"}:
        occurrence["field"] = "label"
    result = rejected(
        current, expander, card, "unsupported_source_owner_" + str(owner["kind"])
    )
    assert result.diagnostic.source is not None


def test_mixed_same_file_retains_success(
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


def test_companion_and_inline_share_current_proof(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    card = author(current)
    original = deepcopy(card)
    record = companion({"evolve": {"cost": {"pp": 1}}})
    record.update(
        frame_id=FRAME,
        params={},
        registrations=[{"id": "all", "domain": {"regions": ["jp"]}, "reviews": []}],
    )
    record["body_version"] = body_version(record)
    macro = generated(record)
    macro["sources"] = ability(card)["sources"]
    card["abilities"] = [macro]
    body = input_file("rules/effect-bodies/BP01/000.yaml", shard(record))
    expanded = validate_files(
        current, inputs({"BP01-009": card}, bodies=(body,)), expander
    )[0]
    inline = validate_files(current, inputs({"BP01-009": original}), expander)[0]
    assert isinstance(expanded, CardAccepted)
    assert isinstance(inline, CardAccepted)
    assert expanded.candidate.tree == inline.candidate.tree
    assert expanded.candidate.sidecar[0].body_version == record["body_version"]


@pytest.mark.parametrize("change", ["frame", "scope", "ports"])
def test_unused_companions_are_globally_checked(
    current: CurrentAuthorSources, expander: AuthorExpander, change: str
) -> None:
    record = companion({"evolve": {"cost": {"pp": 1}}})
    record.update(frame_id=FRAME, params={})
    if change == "frame":
        record["frame_id"] = "frame:" + "f" * 64
    elif change == "scope":
        record["ability_scope"] = "ability_9"
    else:
        record["ports"] = {
            "imports": {},
            "exports": {"future": {"path": [], "bind": "future"}},
        }
    record["body_version"] = body_version(record)
    body = input_file("rules/effect-bodies/BP01/000.yaml", shard(record))
    with pytest.raises(AuthorInputError, match="companion_registry"):
        validate_files(
            current, inputs({"BP01-009": author(current)}, bodies=(body,)), expander
        )


def test_opaque_en_is_per_card_rejected(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    result = validate_files(
        current, inputs({"BP01-009": 17}, scope="en_override"), expander
    )
    assert isinstance(result[0], CardRejected)
    assert result[0].diagnostic.reason == "en_source_contract_unavailable"


def test_real_prototype_inventory_uses_database_identity(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    collection = load_author_set(SCHEMAS.parent / "authored", expander.schemas)
    assert len(collection.prototype_files) == 47
    assert current.validate(SCHEMAS.parent / "authored", expander) == ()


def test_unresolved_prototype_key_is_global(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    collection = load_author_set(SCHEMAS.parent / "authored", expander.schemas)
    old = collection.prototype_files[0]
    tree = object_value(expander.schemas.prototype_tree(old.raw))
    cards = object_value(tree["cards"])
    cards["MISSING-999"] = cards.pop(next(iter(cards)))
    collection = replace(
        collection,
        prototype_files=(input_file(old.path, tree), *collection.prototype_files[1:]),
    )
    with pytest.raises(AuthorInputError, match="prototype_identity_unresolved"):
        validate_files(current, collection, expander)


def test_same_wording_other_real_owner_is_not_borrowed(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    card = author(current)
    # This private reconstruction only supplies a deliberately wrong source input.
    other = current._card_payload("c:9c467cdec10851c3958323fe461496dd")
    matched = other.fields[0].matches[0]
    assert matched is not None
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    ability(card)["sources"] = [use.model_dump(mode="json")]
    rejected(current, expander, card, "source_current_owner_mismatch")


@pytest.mark.parametrize(
    "change", ["missing", "renamed-old-id", "renamed-rekeyed", "pending"]
)
def test_true_registry_mutations_reject_old_author_use(
    current: CurrentAuthorSources,
    fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    change: str,
) -> None:
    card = author(current)
    original = next(
        record
        for record in fixture.snapshot.four_layer.records
        if isinstance(record, FrameRecord) and record.data.id == FRAME
    )
    frame = original.data
    # The compiled field is setup for a registry mutation, not the assertion.
    compiled = current._card_payload(CARDS["BP01-009"]).fields[0]
    matched = compiled.matches[0]
    assert matched is not None
    if change.startswith("renamed"):
        frame = frame.model_copy(
            update={
                "leaf_schema": frame.leaf_schema.model_copy(
                    update={
                        "slots": tuple(
                            slot.model_copy(update={"name": "renamed"})
                            for slot in frame.leaf_schema.slots
                        )
                    }
                )
            }
        )
    elif change == "pending":
        frame = frame.model_copy(
            update={
                "semantic_variant": frame.semantic_variant.model_copy(
                    update={
                        "state": "pending",
                        "key": None,
                        "scope": matched.binding.occurrence_key(),
                    }
                ),
                "projection": frame.projection.model_copy(
                    update={"projection_kind": "pending", "discriminator": None}
                ),
            }
        )
    if change in {"renamed-rekeyed", "pending"}:
        checksum = hash_payload(frame.payload(compiled.field.parts[0].canonical_source))
        frame = frame.model_copy(
            update={"id": "frame:" + checksum, "content_hash": checksum}
        )
    records: list[JsonValue] = (
        []
        if change == "missing"
        else [
            original.model_copy(update={"data": frame}).model_dump(
                mode="json", exclude_computed_fields=True
            )
        ]
    )
    raw = canonical({"format": 3, "kind": "translation_shard", "records": records})
    snapshot = Snapshot(
        (), (("translations/templates/definitions/094.yaml", raw, raw),)
    )
    altered = CurrentAuthorSources(
        current.db,
        fixture.sources,
        snapshot,
        fixture.vocabulary,
        fixture.rules,
        fixture.jp_batch_id,
    )
    result = validate_files(altered, inputs({"BP01-009": card}), expander)[0]
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason in {
        "source_registry_mismatch",
        "source_occurrence_mismatch",
    }


@pytest.mark.parametrize("face_container", [False, True])
def test_synthetic_multiface_owner_controls_both_containers(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    face_container: bool,
) -> None:
    card = author(current)
    face, _ = back_face(current, monkeypatch)
    # Synthetic source setup has no independent adopted payload golden.
    complete = current._card_payload(CARDS["BP01-009"])
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


def test_empty_effect_stays_unknown(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    card = author(current)
    effect(current, monkeypatch, "")
    rejected(current, expander, card, "source_unknown")


def test_current_evidence_replacement_drops_proof(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    result = validate_files(current, inputs({"BP01-009": author(current)}), expander)[0]
    assert isinstance(result, CardAccepted)
    evidence = result.candidate.sidecar[0]
    assert replace(evidence, input_hash="sha256:" + "f" * 64).source_proof == "none"
    with pytest.raises(TypeError, match="init=False"):
        replace(evidence, source_proof="current_source_checked")


@pytest.mark.parametrize(
    ("family", "change"),
    [
        (family, change)
        for family in ("feed", "ride")
        for change in ("none", "missing", "swapped", "extra")
    ]
    + [("feed", "times")],
)
def test_synthetic_sibling_root_closure(
    current: CurrentAuthorSources,
    fixture: CurrentSourceFixture,
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
    compiled = current._card_payload(CARDS["BP01-009"]).fields[0]
    frame, _ = compiled.derived.bind(0)
    original = next(
        record
        for record in fixture.snapshot.four_layer.records
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
        (), (("translations/templates/definitions/094.yaml", encoded, encoded),)
    )
    synthetic = CurrentAuthorSources(
        current.db,
        current.sources,
        snapshot,
        fixture.vocabulary,
        fixture.rules,
        fixture.jp_batch_id,
    )
    # Synthetic source setup has no independent adopted payload golden.
    complete = synthetic._card_payload(CARDS["BP01-009"])
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
    elif change == "extra":
        object_value(object_value(abilities[0])["evolve"])["into"] = (
            "Synthetic other form"
        )
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
    else:
        assert isinstance(result, CardRejected)
        assert result.diagnostic.reason == (
            "source_ability_closure_mismatch"
            if change == "missing"
            else "source_head_payload_mismatch"
        )


def test_real_pending_token_and_en_do_not_block_other_card(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    shared = inputs(
        {
            "BP01-009": author(current),
            **{
                number: {"meta": meta(), "abilities": []}
                for number in ("BP08-038", "CP04-060", "BP01-020", "BP01-076")
            },
        }
    )
    english = inputs({"BP01-009": None}, scope="en_override")
    index = input_file(
        "rules/effects-index.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [
                record.model_dump(mode="json")
                for record in (*shared.index.records, *english.index.records)
            ],
        },
    )
    combined = replace(
        shared,
        index=EffectsIndex.model_validate_json(index.raw),
        index_file=index,
        author_files=(*shared.author_files, *english.author_files),
    )
    result = validate_files(current, combined, expander)
    assert len(result) == 6
    assert isinstance(result[0], CardAccepted)
    reasons = {
        (item.record.record_key, item.record.scope): item.diagnostic.reason
        for item in result
        if isinstance(item, CardRejected)
    }
    assert reasons == {
        ("BP08-038", "shared"): "source_pending",
        ("CP04-060", "shared"): "source_pending",
        ("BP01-020", "shared"): "token_source_contract_unavailable",
        ("BP01-076", "shared"): "unsupported_source_projection",
        ("BP01-009", "en_override"): "en_source_contract_unavailable",
    }


def test_indexed_prototype_identity_and_nonoverlap(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    available = load_author_set(SCHEMAS.parent / "authored", expander.schemas)
    tree = object_value(
        expander.schemas.prototype_tree(available.prototype_files[0].raw)
    )
    original = object_value(tree["cards"])
    number = next(number for number in original if number != "BP01-009")
    tree["cards"] = {number: original[number]}
    prototype = input_file(available.prototype_files[0].path, tree)
    selected = inputs({"BP01-009": author(current)})
    identity = current.db.select(
        "printing", ("card_id",), where={"region": "jp", "card_no": number}
    )[0].values["card_id"]
    assert isinstance(identity, str)
    record: JsonValue = {
        "card_id": identity,
        "scope": "shared",
        "file": prototype.path,
        "kind": "effect_set",
        "record_key": number,
    }
    index = input_file(
        "rules/effects-index.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [selected.index.records[0].model_dump(mode="json"), record],
        },
    )
    collection = replace(
        selected,
        index=EffectsIndex.model_validate_json(index.raw),
        index_file=index,
        prototype_files=(prototype,),
    )
    results = validate_files(current, collection, expander)
    assert isinstance(results[0], CardAccepted)
    assert isinstance(results[1], CardRejected)
    assert results[1].diagnostic.reason == "unsupported_prototype"
    object_value(record)["card_id"] = "c:" + "f" * 32
    index = input_file(
        "rules/effects-index.yaml",
        {
            "format": 1,
            "kind": "effects_index",
            "records": [selected.index.records[0].model_dump(mode="json"), record],
        },
    )
    collection = replace(
        collection, index=EffectsIndex.model_validate_json(index.raw), index_file=index
    )
    with pytest.raises(AuthorInputError, match="prototype_index_identity_mismatch"):
        validate_files(current, collection, expander)


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        ("missing", "source_current_missing"),
        ("provisional", "provisional_record_key"),
        ("variant", "record_key_identity_mismatch"),
    ],
)
def test_current_and_all_printing_variants(
    current: CurrentAuthorSources, expander: AuthorExpander, change: str, reason: str
) -> None:
    card = author(current)
    db = current.db
    with db.transaction():
        if change == "missing":
            db.delete(
                "face_current", {"face_id": FACTS["BP01-009"]["face"], "region": "jp"}
            )
        else:
            row = db.select(
                "printing",
                db.columns("printing"),
                where={"region": "jp", "card_no": "BP01-009"},
            )[0]
            if change == "provisional":
                db.update(
                    "printing",
                    {"id": row.values["id"]},
                    {"card_no_state": "provisional"},
                )
            else:
                db.insert(
                    "printing",
                    dict(row.values)
                    | {
                        "id": "printing:synthetic-variant",
                        "card_id": CARDS["BP01-034"],
                        "variant_key": "synthetic-other",
                    },
                )
    rejected(current, expander, card, reason)


@pytest.mark.parametrize("wrong_section", [False, True])
def test_synthetic_section_zero_is_complete_and_one_based_line(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    wrong_section: bool,
) -> None:
    card = author(current)
    section(current, monkeypatch)
    # Synthetic source setup has no independent adopted payload golden.
    complete = current._card_payload(CARDS["BP01-009"])
    compiled = next(
        field for field in complete.fields if field.source.descriptor.field == "section"
    )
    matched = compiled.matches[0]
    assert matched is not None
    use = SourceUse(
        binding_id=matched.binding.id,
        occurrence=matched.binding.occurrence_key(),
        ability_scope="ability_0",
    )
    node = deepcopy(ability(card))
    node.update(
        section=1 if wrong_section else 0, sources=[use.model_dump(mode="json")]
    )
    array(card["abilities"]).append(node)
    object_value(card["meta"])["source"] = source_version(complete.payload)
    result = validate_files(current, inputs({"BP01-009": card}), expander)[0]
    if wrong_section:
        assert isinstance(result, CardRejected)
        assert result.diagnostic.reason == "source_location_mismatch"
    else:
        assert isinstance(result, CardAccepted)
        assert len(result.candidate.sidecar) == 2
    with current.db.transaction():
        current.db.delete(
            "face_text_section",
            {"revision_id": FACTS["BP01-009"]["revision"], "ordinal": 0},
        )
    rejected(current, expander, card, "source_sections_incomplete")


@pytest.mark.parametrize("scope", ["shared", "en_override"])
def test_complete_real_prototypes_overlap_only_shared(
    current: CurrentAuthorSources, expander: AuthorExpander, tmp_path: Path, scope: str
) -> None:
    available = load_author_set(SCHEMAS.parent / "authored", expander.schemas)
    selected = inputs({"BP01-110": author(current, "BP01-110")}, scope=scope)
    if scope == "shared":
        assert isinstance(validate_files(current, selected, expander)[0], CardAccepted)
    collection = replace(selected, prototype_files=available.prototype_files)
    write_inputs(tmp_path, collection)
    if scope == "shared":
        with pytest.raises(AuthorInputError, match="prototype_author_overlap"):
            current.validate(tmp_path, expander)
        # Caller-controlled lists cannot certify the complete inventory on disk.
        omitted = replace(collection, prototype_files=())
        with pytest.raises(AuthorInputError, match="author_root_required"):
            current.validate(omitted, expander)  # type: ignore[arg-type]
        with pytest.raises(AuthorInputError, match="prototype_author_overlap"):
            current.validate(tmp_path, expander)
    else:
        results = current.validate(tmp_path, expander)
        assert len(results) == 1
        assert isinstance(results[0], CardRejected)
        assert results[0].diagnostic.reason == "en_source_contract_unavailable"


def test_omitted_body_list_cannot_hide_duplicate_on_disk(
    current: CurrentAuthorSources, expander: AuthorExpander, tmp_path: Path
) -> None:
    record = companion({"evolve": {"cost": {"pp": 1}}})
    record.update(frame_id=FRAME, params={})
    record["body_version"] = body_version(record)
    bodies = tuple(
        input_file(f"rules/effect-bodies/BP01/{index:03}.yaml", shard(record))
        for index in range(2)
    )
    collection = inputs({"BP01-009": author(current)}, bodies=bodies)
    write_inputs(tmp_path, collection)
    omitted = replace(collection, body_files=bodies[:1])
    with pytest.raises(AuthorInputError, match="author_root_required"):
        current.validate(omitted, expander)  # type: ignore[arg-type]
    with pytest.raises(ExpansionError, match="duplicate_body"):
        current.validate(tmp_path, expander)


def test_synthetic_valid_other_face_container_rejects(
    current: CurrentAuthorSources,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    card = author(current)
    face, _ = back_face(current, monkeypatch)
    # Synthetic face insertion changes the source hash before container admission.
    complete = current._card_payload(CARDS["BP01-009"])
    object_value(card["meta"])["source"] = source_version(complete.payload)
    card["faces"] = {face: {"abilities": card.pop("abilities")}}
    rejected(current, expander, card, "source_face_container_mismatch")


def test_malformed_card_keeps_same_file_format_error_global(
    current: CurrentAuthorSources, expander: AuthorExpander
) -> None:
    good, bad = author(current), author(current, "BP01-034")
    ability(bad)["evolve"] = {"cost": {"pp": -1}}
    with pytest.raises(AuthorInputError):
        validate_files(current, inputs({"BP01-009": good, "BP01-034": bad}), expander)

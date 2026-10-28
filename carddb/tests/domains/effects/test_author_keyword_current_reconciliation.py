"""Whole current cards consume independent keyword obligations and separate evidence."""

from copy import deepcopy
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.contracts.author_evidence import CardKeywordSourceUse, SourceUse
from sve_carddb.contracts.author_handoff import (
    CurrentKeywordDeclaration,
    SourceInventory,
)
from sve_carddb.contracts.author_leaves import EffectsIndex
from sve_carddb.contracts.four_layer import FaceRevisionOwner
from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.domains.effects.author_current import CardAccepted, CardRejected
from sve_carddb.domains.effects.author_expansion import AuthorExpander, ability_nodes
from sve_carddb.domains.effects.author_inputs import AuthorInputError
from sve_carddb.domains.effects.author_set import AuthorInputSet
from sve_carddb.domains.effects.author_source_hash import source_version
from sve_carddb.domains.translations.inputs import Snapshot, validate_snapshot

from ...support.author_semantic_inputs import load_semantic_inputs
from ...support.semantic_definition_fixtures import with_definitions
from .author_current_goldens import FACTS
from .author_current_synthetic import back_face, effect, token_section
from .author_entry_cases import meta
from .test_author_current import (
    CARDS,
    current,
    input_file,
    validate_files,
    write_inputs,
)
from .test_author_keyword_sources import context

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.domains.effects.author_current import CurrentAuthorSources

    from ...support.author_current_fixtures import CurrentSourceFixture


def card(
    source: CurrentAuthorSources,
    number: str = "BP01-009",
    *,
    faces: bool = False,
    keywords: tuple[str, ...] = ("ward", "aura"),
) -> dict[str, JsonValue]:
    identity = source.db.select(
        "printing", ("card_id",), where={"region": "jp", "card_no": number}
    )[0].values["card_id"]
    assert isinstance(identity, str)
    complete = source.card_payload(identity)
    metadata = meta()
    metadata["source"] = source_version(complete.payload)
    result: dict[str, JsonValue] = {"meta": metadata, "abilities": []}
    for field in complete.fields:
        owner = field.field.source.owner
        assert isinstance(owner, FaceRevisionOwner)
        face = complete.revisions[owner.revision_id]
        for matched in field.matches:
            assert matched is not None
            key = matched.frame.semantic_variant.key
            if matched.frame.projection.projection_kind == "none":
                continue
            entry: dict[str, JsonValue] = {
                "line": matched.binding.line_ordinal + 1,
                "ports": {"imports": {}, "exports": {}},
            }
            if matched.binding.source.ordinal is not None:
                entry["section"] = matched.binding.source.ordinal
            if key == "card_keywords.v2":
                use = CardKeywordSourceUse(
                    binding_id=matched.binding.id,
                    occurrence=matched.binding.occurrence_key(),
                )
                entry["sources"] = [use.model_dump(mode="json")]
                entry["keywords"] = list(keywords)
            else:
                assert key == "evolve_entry.v1"
                body_use = SourceUse(
                    binding_id=matched.binding.id,
                    occurrence=matched.binding.occurrence_key(),
                    ability_scope="ability_0",
                )
                entry["sources"] = [body_use.model_dump(mode="json")]
                entry["evolve"] = {"cost": {"pp": 1}}
            container = result
            if faces:
                face_map = object_value(result.setdefault("faces", {}))
                container = object_value(face_map.setdefault(face, {"abilities": []}))
            array(container["abilities"]).append(entry)
    return result


def collection(
    source: CurrentAuthorSources, cards: dict[str, JsonValue]
) -> AuthorInputSet:
    document = input_file(
        "rules/author-effects/BP01/000.yaml",
        {"format": 1, "kind": "author_effect_set", "scope": "shared", "cards": cards},
    )
    records: list[JsonValue] = []
    for number in sorted(cards):
        identity = source.db.select(
            "printing", ("card_id",), where={"region": "jp", "card_no": number}
        )[0].values["card_id"]
        assert isinstance(identity, str)
        records.append(
            {
                "card_id": identity,
                "scope": "shared",
                "file": document.path,
                "kind": "author_effect_set",
                "record_key": number,
            }
        )
    index = input_file(
        "rules/effects-index/BP01.yaml",
        {"format": 1, "kind": "effects_index", "records": records},
    )
    return AuthorInputSet(
        EffectsIndex.model_validate_json(index.raw), (index,), (document,), (), ()
    )


def nodes(value: dict[str, JsonValue]) -> list[dict[str, JsonValue]]:
    return [entry for _, entry in ability_nodes({"cards": {"BP01-009": value}})]


def test_real_current_card_emits_closed_keyword_handoff(
    keyword_current_fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(keyword_current_fixture) as source:
        (result,) = validate_files(
            source, collection(source, {"BP01-138": card(source, "BP01-138")}), expander
        )
    assert isinstance(result, CardAccepted)
    wire = object_value(result.lowering_input())
    inventory = SourceInventory.model_validate_json(canonical(wire["source_inventory"]))
    declarations = array(wire["declaration_sidecar"])
    evidence = CurrentKeywordDeclaration.model_validate_json(canonical(declarations[0]))
    assert result.candidate.sidecar == ()
    assert inventory.abilities == ()
    assert len(declarations) == len(inventory.non_body) == 1
    obligation = inventory.non_body[0]
    assert obligation.kind == "printed_keywords"
    assert obligation.source_use == evidence.source_use
    assert obligation.declaration == evidence.declaration
    assert evidence.declaration.keywords == ("ward", "aura")
    assert evidence.face_id == "f:0a2de2c8ea0d5629b3f899fc2cc52c88"
    assert evidence.path == ("cards", "BP01-138", "abilities", 0)
    assert evidence.source_proof == "current_source_checked"
    assert "ability_scope" not in evidence.source_use.model_dump()
    assert (
        not {"body_version", "body_payload", "body_input_hash"}
        & evidence.model_dump().keys()
    )


@pytest.mark.parametrize("faces", [False, True])
def test_body_and_keyword_evidence_have_disjoint_paths(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    faces: bool,
) -> None:
    with current(keyword_current_fixture) as source:
        body = source.card_payload(CARDS["BP01-009"]).fields[0].source.text
        effect(source, monkeypatch, "【守護】【オーラ】\n" + body)
        (result,) = validate_files(
            source,
            collection(source, {"BP01-009": card(source, faces=faces)}),
            expander,
        )
    assert isinstance(result, CardAccepted)
    wire = object_value(result.lowering_input())
    inventory = SourceInventory.model_validate_json(canonical(wire["source_inventory"]))
    assert len(result.candidate.sidecar) == len(result.candidate.declarations) == 1
    assert result.candidate.sidecar[0].path != result.candidate.declarations[0].path
    assert len(inventory.abilities) == 1
    assert {item.kind for item in inventory.non_body} == {
        "metadata",
        "printed_keywords",
    }
    assert all(
        item.source_proof == "current_source_checked"
        for item in (*result.candidate.sidecar, *result.candidate.declarations)
    )


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        ("missing", "source_keyword_closure_mismatch"),
        ("missing-element", "source_keyword_payload_mismatch"),
        ("replace", "source_keyword_payload_mismatch"),
        ("extra", "source_keyword_payload_mismatch"),
        ("order", "source_keyword_payload_mismatch"),
        ("unsupported", "unsupported_keyword_declaration"),
        ("duplicate-use", "duplicate_source_keyword"),
        ("split-use", "source_keyword_payload_mismatch"),
        ("binding", "source_keyword_occurrence_mismatch"),
        ("owner", "source_current_owner_mismatch"),
        ("line", "source_location_mismatch"),
        ("segments", "source_keyword_occurrence_mismatch"),
        ("face", "source_face_container_mismatch"),
        ("missing-body", "source_ability_closure_mismatch"),
    ],
)
def test_whole_card_rejects_keyword_mismatch(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    change: str,
    reason: str,
) -> None:
    with current(keyword_current_fixture) as source:
        body = source.card_payload(CARDS["BP01-009"]).fields[0].source.text
        raw = "【守護】" if change in {"replace", "extra"} else "【守護】【オーラ】"
        effect(source, monkeypatch, raw + "\n" + body)
        authored = card(
            source,
            keywords=("ward",) if change in {"replace", "extra"} else ("ward", "aura"),
        )
        entry = nodes(authored)[0]
        use = object_value(array(entry["sources"])[0])
        occurrence = object_value(use["occurrence"])
        keyword_changes: dict[str, list[JsonValue]] = {
            "missing-element": ["ward"],
            "replace": ["aura"],
            "extra": ["ward", "aura"],
            "order": ["aura", "ward"],
            "unsupported": ["drain"],
        }
        if change in {"missing", "missing-body"}:
            array(authored["abilities"]).pop(0 if change == "missing" else -1)
        elif change in keyword_changes:
            entry["keywords"] = keyword_changes[change]
        elif change == "split-use":
            entry["keywords"] = ["ward"]
            second = deepcopy(entry)
            second["keywords"] = ["aura"]
            array(authored["abilities"]).append(second)
        elif change == "duplicate-use":
            array(authored["abilities"]).append(deepcopy(entry))
        elif change == "binding":
            use["binding_id"] = "bind:" + "f" * 64
        elif change == "owner":
            occurrence["owner"] = {
                "kind": "face_revision",
                "revision_id": FACTS["BP01-034"]["revision"],
            }
        elif change == "line":
            entry["line"] = 2
        elif change == "segments":
            occurrence["segments"] = [{"start": 0, "end": 4}]
        elif change == "face":
            authored["faces"] = {
                "f:" + "f" * 32: {"abilities": authored.pop("abilities")}
            }
        raw_file = collection(source, {"BP01-009": authored}).author_files[0].raw
        assert bool(expander.expand(raw_file, ()).declarations) == (change != "missing")
        (result,) = validate_files(
            source, collection(source, {"BP01-009": authored}), expander
        )
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason == reason


def test_duplicate_list_is_rejected_by_author_schema(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with current(keyword_current_fixture) as source:
        effect(source, monkeypatch, "【守護】")
        authored = card(source, keywords=("ward", "ward"))
        with pytest.raises(AuthorInputError):
            validate_files(source, collection(source, {"BP01-009": authored}), expander)


@pytest.mark.parametrize("change", ["valid", "missing-face", "borrow-face"])
def test_two_faces_consume_independent_same_keyword(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    change: str,
) -> None:
    with current(keyword_current_fixture) as source:
        _, revision = back_face(source, monkeypatch)
        effect(source, monkeypatch, "【守護】")
        effect(source, monkeypatch, "【守護】", revision=revision, face_ordinal=1)
        authored = card(source, faces=True, keywords=("ward",))
        entries = nodes(authored)
        if change == "missing-face":
            face_map = object_value(authored["faces"])
            object_value(list(face_map.values())[1])["abilities"] = []
        elif change == "borrow-face":
            entries[1]["sources"] = deepcopy(entries[0]["sources"])
        (result,) = validate_files(
            source, collection(source, {"BP01-009": authored}), expander
        )
    if change == "valid":
        assert isinstance(result, CardAccepted)
        assert len({item.face_id for item in result.candidate.declarations}) == 2
        assert (
            len(
                {
                    item.current_source.binding_id
                    for item in result.candidate.declarations
                    if item.current_source
                }
            )
            == 2
        )
    else:
        assert isinstance(result, CardRejected)
        assert result.diagnostic.reason == (
            "source_keyword_closure_mismatch"
            if change == "missing-face"
            else "source_face_container_mismatch"
        )


def test_source_printed_reverse_order_is_the_positive_order(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    frame = context("【オーラ】【守護】").frame
    content = canonical(
        {
            "format": 3,
            "kind": "translation_shard",
            "records": [
                {
                    "kind": "sentence_template",
                    "data": frame.model_dump(mode="json", exclude_computed_fields=True),
                }
            ],
        }
    )
    snapshot = Snapshot(
        keyword_current_fixture.snapshot.shards,
        with_definitions(
            (
                *keyword_current_fixture.snapshot.closure,
                (
                    "translations/templates/definitions/999.yaml",
                    content,
                    content,
                ),
            )
        ),
    )
    validate_snapshot(snapshot)
    with current(replace(keyword_current_fixture, snapshot=snapshot)) as source:
        effect(source, monkeypatch, "【オーラ】【守護】")
        (result,) = validate_files(
            source,
            collection(source, {"BP01-009": card(source, keywords=("aura", "ward"))}),
            expander,
        )
    assert isinstance(result, CardAccepted)
    assert result.candidate.declarations[0].declaration_payload == canonical(
        {"keywords": ["aura", "ward"], "ports": {"imports": {}, "exports": {}}}
    )


def test_structural_expansion_and_replacement_do_not_grant_proof(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with current(keyword_current_fixture) as source:
        effect(source, monkeypatch, "【守護】")
        inputs = collection(source, {"BP01-009": card(source, keywords=("ward",))})
        structural = expander.expand(inputs.author_files[0].raw, ())
        assert structural.sidecar == ()
        assert structural.declarations[0].source_proof == "none"
        with pytest.raises(ValidationError):
            structural.declarations[0].lowering_input()
        (obligation,) = source.keyword_obligations(CARDS["BP01-009"])
        derived = expander.expand_contexts(
            inputs.author_files[0].raw,
            (),
            {},
            declarations={structural.declarations[0].path: obligation},
        )
        assert derived.declarations[0].source_proof == "none"
        (result,) = validate_files(source, inputs, expander)
    assert isinstance(result, CardAccepted)
    assert replace(result.candidate.declarations[0]).source_proof == "none"


def test_selection_keeps_keyword_failures_without_fallback(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    tmp_path: Path,
) -> None:
    with current(keyword_current_fixture) as source:
        effect(source, monkeypatch, "【守護】")
        # An official JP printing of the same permanent card must share the rejection.
        printing = next(
            row.values
            for row in source.db.rows("printing")
            if row.values["card_no"] == "BP01-009"
        )
        with source.db.transaction():
            source.db.insert(
                "printing",
                dict(printing)
                | {"id": "SYN-KEYWORD-ALIAS", "card_no": "SYN-KEYWORD-ALIAS"},
            )
        inputs = collection(
            source,
            {
                "BP01-138": card(source, "BP01-138"),
                "BP01-009": card(source, keywords=("aura",)),
            },
        )
        write_inputs(tmp_path, inputs)
        selected = object_value(
            source.selection_inputs(load_semantic_inputs(tmp_path, expander), expander)
        )
    authors = [object_value(item) for item in array(selected["authors"])]
    accepted = [item for item in authors if item["status"] == "accepted"]
    rejected = [item for item in authors if item["status"] == "rejected"]
    assert len(accepted) == 1
    assert len(array(object_value(accepted[0]["input"])["declaration_sidecar"])) == 1
    assert {item["card_no"] for item in rejected} >= {
        "BP01-009",
        "SYN-KEYWORD-ALIAS",
    }
    assert {item["reason"] for item in rejected} == {"source_keyword_payload_mismatch"}
    assert selected["documents"] == []


def test_token_section_rejects_whole_current_author(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with current(keyword_current_fixture) as source:
        effect(source, monkeypatch, "【守護】")
        authored = card(source, keywords=("ward",))
        token_section(source, monkeypatch, FACTS["BP01-009"]["revision"])
        (result,) = validate_files(
            source, collection(source, {"BP01-009": authored}), expander
        )
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason == "token_source_contract_unavailable"


@pytest.fixture(scope="session")
def handoff(
    keyword_current_fixture: CurrentSourceFixture, expander: AuthorExpander
) -> bytes:
    with current(keyword_current_fixture) as source:
        (result,) = validate_files(
            source, collection(source, {"BP01-138": card(source, "BP01-138")}), expander
        )
    assert isinstance(result, CardAccepted)
    return canonical(result.lowering_input())


@pytest.mark.parametrize(
    "change",
    [
        "unknown-kind",
        "metadata-payload",
        "duplicate-keyword",
        "nonempty-ports",
        "scoped-use",
        "body-version",
        "missing-frame",
        "unknown-keyword",
        "proof-none",
        "missing-recipe",
        "null-hash",
        "negative-path",
    ],
)
def test_internal_handoff_types_are_closed(handoff: bytes, change: str) -> None:
    wire = object_value(parse(handoff))
    declaration = object_value(array(wire["declaration_sidecar"])[0])
    inventory = object_value(wire["source_inventory"])
    expected = object_value(array(inventory["non_body"])[0])
    if change in {"unknown-kind", "metadata-payload"}:
        expected["kind"] = "future" if change == "unknown-kind" else "metadata"
        with pytest.raises(ValidationError):
            SourceInventory.model_validate_json(canonical(inventory))
        return
    updates: dict[str, dict[str, JsonValue]] = {
        "body-version": {"body_version": None},
        "unknown-keyword": {
            "declaration": {
                "keywords": ["quick"],
                "ports": {"imports": {}, "exports": {}},
            }
        },
        "duplicate-keyword": {
            "declaration": {
                "keywords": ["ward", "ward"],
                "ports": {"imports": {}, "exports": {}},
            }
        },
        "proof-none": {"source_proof": "none"},
        "null-hash": {"source_input_hash": None},
        "negative-path": {"path": ["cards", "BP01-138", "abilities", -1]},
    }
    if change in updates:
        declaration.update(updates[change])
    elif change in {"missing-frame", "missing-recipe"}:
        declaration.pop(
            "frame_id" if change == "missing-frame" else "source_adapter_recipe"
        )
    elif change == "scoped-use":
        object_value(declaration["source_use"])["ability_scope"] = "ability_0"
    else:
        object_value(object_value(declaration["declaration"])["ports"])["exports"] = {
            "p": {"path": [], "bind": "x"}
        }
    with pytest.raises(ValidationError):
        CurrentKeywordDeclaration.model_validate_json(canonical(declaration))


@pytest.mark.parametrize(
    "missing", ["card_keywords.v2", "metadata.layout.v1", "evolve_entry.v1"]
)
def test_missing_registry_sibling_rejects_entire_author(
    keyword_registry_snapshots: Mapping[str, Snapshot],
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    missing: str,
) -> None:
    with monkeypatch.context() as setup, current(keyword_current_fixture) as source:
        body = source.card_payload(CARDS["BP01-009"]).fields[0].source.text
        raw = "【守護】\n" + body
        effect(source, setup, raw)
        authored = card(source, keywords=("ward",))
    incomplete = replace(
        keyword_current_fixture,
        snapshot=keyword_registry_snapshots[missing],
    )
    with current(incomplete) as source:
        effect(source, monkeypatch, raw)
        (result,) = validate_files(
            source, collection(source, {"BP01-009": authored}), expander
        )
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason == "source_registry_mismatch"


def test_body_only_token_face_is_outside_first_batch(
    keyword_current_fixture: CurrentSourceFixture, expander: AuthorExpander
) -> None:
    with current(keyword_current_fixture) as source:
        with source.db.transaction():
            source.db.insert(
                "face_special_kind",
                {
                    "revision_id": FACTS["BP01-009"]["revision"],
                    "special_kind_code": "token",
                },
            )
        authored = card(source)
        assert all("keywords" not in entry for entry in nodes(authored))
        (result,) = validate_files(
            source, collection(source, {"BP01-009": authored}), expander
        )
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason == "token_source_contract_unavailable"


@pytest.mark.parametrize(
    ("raw", "reason"),
    [
        ("【守護】【ドレイン】", "unsupported_source_keyword"),
        ("【守護】\n【守護】", "unsupported_duplicate_source_keyword"),
        ("【守護】\n不明な能力。", "source_pending"),
    ],
    ids=("unsupported-source", "duplicate-source", "pending-source"),
)
def test_source_side_failure_rejects_whole_card(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
    raw: str,
    reason: str,
) -> None:
    with current(keyword_current_fixture) as source:
        effect(source, monkeypatch, "【守護】")
        authored = card(source, keywords=("ward",))
        effect(source, monkeypatch, raw)
        if reason == "unsupported_duplicate_source_keyword":
            object_value(authored["meta"])["source"] = source_version(
                source.card_payload(CARDS["BP01-009"]).payload
            )
        (result,) = validate_files(
            source, collection(source, {"BP01-009": authored}), expander
        )
    assert isinstance(result, CardRejected)
    assert result.diagnostic.reason == reason


def test_root_declaration_on_second_face_keeps_its_owner(
    keyword_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with current(keyword_current_fixture) as source:
        face, revision = back_face(source, monkeypatch)
        effect(source, monkeypatch, "【守護】", revision=revision, face_ordinal=1)
        authored = card(source, keywords=("ward",))
        assert "faces" not in authored
        (result,) = validate_files(
            source, collection(source, {"BP01-009": authored}), expander
        )
    assert isinstance(result, CardAccepted)
    (declaration,) = result.candidate.declarations
    assert declaration.face_id == face
    assert declaration.path == ("cards", "BP01-009", "abilities", 1)
    assert declaration.current_source is not None
    assert declaration.current_source.occurrence.owner == FaceRevisionOwner(
        kind="face_revision", revision_id=revision
    )

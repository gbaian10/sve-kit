"""Current keyword obligations use complete public inputs and explicit synthetic mutations."""

from dataclasses import replace
from sqlite3 import IntegrityError

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.four_layer import FaceRevisionOwner
from sve_carddb.domains.effects.author_current import CardAccepted
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_source_hash import source_version
from sve_carddb.domains.effects.author_sources import ADAPTER_RECIPE, SourceError

from ...support.author_current_fixtures import (
    CARD_NUMBERS,
    CurrentSourceFixture,
    current_source_fixture,
    entry_snapshot,
)
from ...support.time_budget import slow_fixture
from .author_current_goldens import FACTS
from .author_current_synthetic import back_face, effect, token_section
from .author_entry_cases import meta
from .test_author_current import CARDS, current, inputs, validate_files
from .test_author_entries import SCHEMAS


@pytest.fixture(scope="module")
@slow_fixture(
    reason="Build the real current-source database for keyword declarations "
    "once for this module; each test copies the database before writing."
)
def fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return current_source_fixture(
        tmp_path_factory.mktemp("keyword-current"),
        card_numbers=CARD_NUMBERS | {"BP01-138"},
        snapshot=entry_snapshot(include_keywords=True),
        preparation=author_source_preparation,
    )


def test_real_printed_keywords_retain_exact_current_face(
    fixture: CurrentSourceFixture,
) -> None:
    with current(fixture) as sources:
        row = sources.db.select(
            "printing", ("card_id",), where={"region": "jp", "card_no": "BP01-138"}
        )[0]
        card_id = row.values["card_id"]
        assert isinstance(card_id, str)
        obligations = sources.keyword_obligations(card_id)
        assert len(obligations) == 1
        found = obligations[0]
        assert found.keywords == ("ward", "aura")
        assert found.face_id == "f:0a2de2c8ea0d5629b3f899fc2cc52c88"
        assert (
            found.source_use.binding_id
            == "bind:c7d543bc814803cab3fae8757c7f4c6f261488b904c21838c426066b593351b5"
        )
        assert found.kind == "printed_keywords"
        assert found.adapter_recipe == ADAPTER_RECIPE
        owner = found.source_use.occurrence.owner
        assert isinstance(owner, FaceRevisionOwner)
        revision = sources.db.select(
            "face_revision", ("face_id",), where={"id": owner.revision_id}
        )[0]
        assert found.face_id == revision.values["face_id"]
        assert found.source.frame.projection.projection_kind == "card_field"
        assert found.source.frame.projection.scopes == ()
        assert "ability_scope" not in found.source_use.model_dump()
        assert found.source.source_proof == "none"
        assert found.declaration_payload() == {
            "keywords": ["ward", "aura"],
            "ports": {"imports": {}, "exports": {}},
        }


@pytest.mark.parametrize(
    ("raw", "expected"),
    [
        ("【守護】", ("ward",)),
        ("【オーラ】", ("aura",)),
        ("【守護】【オーラ】", ("ward", "aura")),
    ],
    ids=("ward", "aura", "both"),
)
def test_synthetic_current_source_preserves_full_declaration(
    fixture: CurrentSourceFixture,
    monkeypatch: pytest.MonkeyPatch,
    raw: str,
    expected: tuple[str, ...],
) -> None:
    with current(fixture) as sources:
        effect(sources, monkeypatch, raw)
        (found,) = sources.keyword_obligations(CARDS["BP01-009"])
        assert found.keywords == expected
        assert found.face_id == FACTS["BP01-009"]["face"]
        assert found.source_use.occurrence.line_ordinal == 0


def test_body_and_layout_do_not_create_keyword_obligations(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as sources:
        assert sources.keyword_obligations(CARDS["BP01-009"]) == ()
        body = sources.card_payload(CARDS["BP01-009"]).fields[0].source.text
        effect(sources, monkeypatch, "【守護】\n" + body)
        (found,) = sources.keyword_obligations(CARDS["BP01-009"])
        assert found.keywords == ("ward",)


def test_two_faces_with_same_keyword_are_independent_obligations(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as sources:
        face, revision = back_face(sources, monkeypatch)
        effect(sources, monkeypatch, "【守護】")
        effect(sources, monkeypatch, "【守護】", revision=revision, face_ordinal=1)
        found = sources.keyword_obligations(CARDS["BP01-009"])
        assert {item.face_id for item in found} == {FACTS["BP01-009"]["face"], face}
        assert [item.keywords for item in found] == [("ward",), ("ward",)]
        assert len({item.source_use.binding_id for item in found}) == 2
        assert len({item.source_use.occurrence.owner for item in found}) == 2


def test_unknown_sibling_text_cannot_hide_a_keyword_obligation(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as sources:
        _, revision = back_face(sources, monkeypatch)
        effect(sources, monkeypatch, "【守護】")
        effect(sources, monkeypatch, "", revision=revision, face_ordinal=1)
        with pytest.raises(SourceError, match="source_unknown"):
            sources.keyword_obligations(CARDS["BP01-009"])


def test_unknown_source_attribute_is_not_a_complete_current_source(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as sources:
        effect(sources, monkeypatch, "【守護】")
        with sources.db.transaction():
            sources.db.update(
                "face_revision", {"id": FACTS["BP01-009"]["revision"]}, {"attack": None}
            )
        with pytest.raises(SourceError, match="source_unknown"):
            sources.keyword_obligations(CARDS["BP01-009"])


def test_token_face_keyword_requires_a_separate_owner_contract(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as sources:
        effect(sources, monkeypatch, "【守護】")
        with sources.db.transaction():
            sources.db.insert(
                "face_special_kind",
                {
                    "revision_id": FACTS["BP01-009"]["revision"],
                    "special_kind_code": "token",
                },
            )
        with pytest.raises(SourceError, match="token_source_contract_unavailable"):
            sources.keyword_obligations(CARDS["BP01-009"])


@pytest.mark.parametrize(
    ("raw", "reason"),
    [
        ("【守護】\n【守護】", "unsupported_duplicate_source_keyword"),
        ("【守護】【守護】", "unsupported_duplicate_source_keyword"),
        ("【守護】【ドレイン】", "unsupported_source_keyword"),
        ("【守護】\n不明な能力。", "source_pending"),
        ("{クイック}", "source_unresolved_issue"),
        ("（プレイしたターンから攻撃できる。）", "source_pending"),
        ("【ドレイン】", "unsupported_source_keyword"),
        ("【守護】【疾走】", "source_registry_mismatch"),
        ("【疾走】（プレイしたターンから攻撃できる。）", "source_registry_mismatch"),
    ],
    ids=(
        "duplicate-line",
        "duplicate-element",
        "unsupported-sibling",
        "pending-sibling",
        "quick-missing-concept",
        "reminder-only",
        "unsupported-keyword",
        "unregistered-combination",
        "unregistered-reminder",
    ),
)
def test_entire_source_must_be_supported(
    fixture: CurrentSourceFixture,
    monkeypatch: pytest.MonkeyPatch,
    raw: str,
    reason: str,
) -> None:
    with current(fixture) as sources:
        effect(sources, monkeypatch, raw)
        with pytest.raises(SourceError, match=reason):
            sources.keyword_obligations(CARDS["BP01-009"])


@pytest.mark.parametrize(
    "missing", ["card_keywords.v2", "metadata.layout.v1", "evolve_entry.v1"]
)
def test_missing_registry_sibling_never_yields_partial_expectations(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch, missing: str
) -> None:
    incomplete = replace(
        fixture,
        snapshot=entry_snapshot(
            include_keywords=True, exclude_semantics=frozenset({missing})
        ),
    )
    with current(incomplete) as sources:
        body = sources.card_payload(CARDS["BP01-009"]).fields[0].source.text
        effect(sources, monkeypatch, "【守護】\n" + body)
        with pytest.raises(SourceError, match="source_registry_mismatch"):
            sources.keyword_obligations(CARDS["BP01-009"])


def test_database_rejects_cross_card_current_revision(
    fixture: CurrentSourceFixture,
) -> None:
    with current(fixture) as sources:
        with (
            pytest.raises(IntegrityError, match="Foreign key check failed"),
            sources.db.transaction(),
        ):
            sources.db.update(
                "face_current",
                {"face_id": FACTS["BP01-009"]["face"], "region": "jp"},
                {"revision_id": FACTS["BP01-034"]["revision"]},
            )
        assert sources.keyword_obligations(CARDS["BP01-009"]) == ()


def test_unclosed_token_owner_still_rejects_entire_source(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as sources:
        effect(sources, monkeypatch, "【守護】")
        token_section(sources, monkeypatch, FACTS["BP01-009"]["revision"])
        with pytest.raises(SourceError, match="token_source_contract_unavailable"):
            sources.keyword_obligations(CARDS["BP01-009"])


def test_source_obligations_are_consumed_by_current_keyword_authors(
    fixture: CurrentSourceFixture, monkeypatch: pytest.MonkeyPatch
) -> None:
    with current(fixture) as sources:
        effect(sources, monkeypatch, "【守護】")
        (found,) = sources.keyword_obligations(CARDS["BP01-009"])
        metadata = meta()
        metadata["source"] = source_version(
            sources.card_payload(CARDS["BP01-009"]).payload
        )
        card: dict[str, JsonValue] = {
            "meta": metadata,
            "abilities": [
                {
                    "line": 1,
                    "sources": [found.source_use.model_dump(mode="json")],
                    "ports": {"imports": {}, "exports": {}},
                    "keywords": ["ward"],
                }
            ],
        }
        (result,) = validate_files(
            sources, inputs({"BP01-009": card}), AuthorExpander(SCHEMAS)
        )
        assert isinstance(result, CardAccepted)
        assert result.candidate.sidecar == ()
        assert result.candidate.declarations[0].source_proof == "current_source_checked"

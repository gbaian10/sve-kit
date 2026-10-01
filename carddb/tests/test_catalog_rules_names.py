"""Exact regional names are distinct from card identity, aliases and deck counts."""

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- mutate and derive within one rollback transaction

from typing import TYPE_CHECKING, Literal

import pytest

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.catalog.importer import populate_catalog
from sve_carddb.catalog.models import NameBinding
from sve_carddb.catalog.rules_names import populate_rules_names
from sve_carddb.products.models import LocalizedText
from sve_carddb.text_observations.intern import TextInterner

from .build_db_fixtures import seed
from .test_catalog import catalog, context

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import Database


@pytest.fixture
def db() -> Iterator[Database]:
    with create_database(compile_t0()) as database:
        seed(database)
        with database.transaction():
            database.delete(
                "face_rules_name",
                {
                    "face_id": "face",
                    "region": "jp",
                    "rules_name_id": "rules_name",
                    "role": "primary",
                },
            )
        yield database


def set_name(db: Database, region: str, name: str) -> None:
    text = TextInterner(db).intern(
        LocalizedText(lang="ja" if region == "jp" else "en", text=name)
    )
    if region == "jp":
        db.update("face_revision", {"id": "revision"}, {"name_unit_id": text})
    else:
        original = dict(db.rows("face_revision")[0].values)
        db.insert(
            "face_revision",
            original | {"id": "revision:en", "region": "en", "name_unit_id": text},
        )
        db.insert(
            "face_current",
            {
                "face_id": "face",
                "region": "en",
                "revision_id": "revision:en",
                "basis": "latest_observed_no_errata",
                "decision_id": None,
            },
        )


def test_same_exact_name_in_two_regions_keeps_two_groups(db: Database) -> None:
    with db.transaction():
        db.insert(
            "language",
            {"code": "en", "fallback_order": Json([]), "display_name": "English"},
        )
        set_name(db, "jp", "SameⓈa")
        set_name(db, "en", "SameⓈa")
        assert populate_rules_names(db) == 2
        assert populate_rules_names(db) == 0
    links = db.rows("face_rules_name")
    assert len(links) == 2
    assert links[0].values["rules_name_id"] != links[1].values["rules_name_id"]
    assert {r.values["region"] for r in links} == {"jp", "en"}
    assert len(db.rows("face")) == 1


@pytest.mark.parametrize(
    "name", ["ExactⓈa", "ExactSa", "ExactⓈA", " ExactⓈa", "ExactⓈa ", "e\u0301", "é"]
)
def test_names_are_not_unicode_folded_or_trimmed(db: Database, name: str) -> None:
    with db.transaction():
        set_name(db, "jp", name)
        assert populate_rules_names(db) == 1
    assert any(
        row.values["official_name"] == name and row.values["region"] == "jp"
        for row in db.rows("rules_name")
    )


def test_pending_effect_does_not_discard_unanimous_official_name(db: Database) -> None:
    with db.transaction():
        db.delete("face_current", {"face_id": "face", "region": "jp"})
        assert populate_rules_names(db) == 1
    assert len(db.rows("face_rules_name")) == 1
    assert not db.rows("face_current")


def test_pending_different_names_cannot_choose_by_printing_order(db: Database) -> None:
    with db.transaction():
        db.delete("face_current", {"face_id": "face", "region": "jp"})
        original = dict(db.rows("face_revision")[0].values)
        text = TextInterner(db).intern(
            LocalizedText(lang="ja", text="Another exact name")
        )
        db.insert(
            "face_revision",
            original | {"id": "revision:other", "revision": 1, "name_unit_id": text},
        )
        source = dict(db.rows("source_record")[0].values)
        db.insert("source_record", source | {"id": "source:other"})
        observation = dict(db.rows("printing_face_observation")[0].values)
        db.insert(
            "printing_face_observation",
            observation
            | {"source_id": "source:other", "revision_id": "revision:other"},
        )
        assert populate_rules_names(db) == 0
    assert not db.rows("face_rules_name")


def test_two_faces_with_same_regional_name_share_group_without_merging_cards(
    db: Database,
) -> None:
    with db.transaction():
        card = dict(db.rows("card")[0].values)
        db.insert("card", card | {"id": "another-card"})
        db.insert(
            "face",
            {
                "id": "another-face",
                "card_id": "another-card",
                "ordinal": 0,
                "side": "front",
                "decision_id": None,
            },
        )
        original = dict(db.rows("face_revision")[0].values)
        db.insert(
            "face_revision",
            original | {"id": "another-revision", "face_id": "another-face"},
        )
        db.insert(
            "face_current",
            {
                "face_id": "another-face",
                "region": "jp",
                "revision_id": "another-revision",
                "basis": "latest_observed_no_errata",
                "decision_id": None,
            },
        )
        assert populate_rules_names(db) == 2
    assert len({row.values["rules_name_id"] for row in db.rows("face_rules_name")}) == 1
    assert len(db.rows("face")) == 2


def test_construction_name_hash_collision_is_not_silently_reused(
    db: Database, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(
        "sve_carddb.catalog.rules_names.digest", lambda _: "sha256:" + "a" * 64
    )
    with db.transaction():
        populate_rules_names(db)
        db.delete(
            "face_rules_name",
            {
                "face_id": "face",
                "region": "jp",
                "rules_name_id": "rn:v1:" + "a" * 64,
                "role": "primary",
            },
        )
    with pytest.raises(ValueError, match="ID collision"), db.transaction():
        set_name(db, "jp", "Another")
        populate_rules_names(db)


@pytest.mark.parametrize("role", ["collab", "treated_as"])
def test_special_name_needs_explicit_decision_and_present_region(
    db: Database, role: Literal["collab", "treated_as"]
) -> None:
    value = catalog().model_copy(
        update={
            "names": (
                NameBinding(
                    face_id="face",
                    region="jp",
                    official_name="Synthetic special name",
                    role=role,
                    decision_id="decision",
                ),
            )
        }
    )
    with db.transaction():
        populate_catalog(db, value, build=context(value), published=())
    assert any(
        row.values["role"] == role and row.values["decision_id"] == "decision"
        for row in db.rows("face_rules_name")
    )
    wrong = value.model_copy(
        update={"names": (value.names[0].model_copy(update={"region": "en"}),)}
    )
    with pytest.raises(ValueError, match="face in its region"), db.transaction():
        populate_catalog(db, wrong, build=context(wrong), published=())


def test_existing_primary_cannot_accumulate_a_new_name(db: Database) -> None:
    with db.transaction():
        populate_rules_names(db)
    with pytest.raises(ValueError, match="Conflicting primary"), db.transaction():
        set_name(db, "jp", "Different")
        populate_rules_names(db)

"""Adopted art uses reject candidate decisions at transaction verification."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build

from .build_db_fixtures import rows

if TYPE_CHECKING:
    from sve_carddb.build_db import Database


def seed(db: Database, state: str, *, used: bool) -> None:
    """Keep the art decision independent of other synthetic parent decisions."""
    fixtures = rows()
    for name in (
        "decision",
        "language",
        "text_unit",
        "product_family",
        "card",
        "face",
        "source_record",
        "printing",
        "printing_face",
    ):
        db.insert(name, fixtures[name])
    db.insert("decision", rows()["decision"] | {"id": "art_decision", "state": state})
    db.insert(
        "art",
        {
            "id": "art",
            "card_id": "card",
            "face_id": "face",
            "classification": "unclassified",
            "decision_id": "art_decision",
        },
    )
    if used:
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"art_id": "art"},
        )


@pytest.mark.parametrize("state", ["sampled", "confirmed"])
def test_adopted_art_use_allows_both_review_levels(state: str) -> None:
    with create_database(compile_build(("art",))) as db:
        with db.transaction():
            seed(db, state, used=True)
        assert db.rows("printing_face")[0].values["art_id"] == "art"


@pytest.mark.parametrize(
    "state", ["proposed", "model_reviewed", "rejected", "disputed"]
)
def test_unused_candidate_art_can_remain_in_build(state: str) -> None:
    with create_database(compile_build(("art",))) as db:
        with db.transaction():
            seed(db, state, used=False)
        assert len(db.rows("art")) == 1
        assert db.rows("printing_face")[0].values["art_id"] is None


@pytest.mark.parametrize(
    "state", ["proposed", "model_reviewed", "rejected", "disputed"]
)
def test_art_decision_change_rechecks_use_and_rolls_back(state: str) -> None:
    with create_database(compile_build(("art",))) as db:
        with db.transaction():
            seed(db, "confirmed", used=True)
        before = db.rows("decision")
        with (
            pytest.raises(sqlite3.IntegrityError, match="art_use_adopted"),
            db.transaction(),
        ):
            db.update("decision", {"id": "art_decision"}, {"state": state})
        assert db.rows("decision") == before
        assert db.rows("printing_face")[0].values["art_id"] == "art"


def test_new_use_of_candidate_art_fails_at_commit() -> None:
    with create_database(compile_build(("art",))) as db:
        with db.transaction():
            seed(db, "proposed", used=False)
        with (
            pytest.raises(sqlite3.IntegrityError, match="art_use_adopted"),
            db.transaction(),
        ):
            db.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"art_id": "art"},
            )
        assert db.rows("printing_face")[0].values["art_id"] is None
        assert len(db.rows("art")) == 1

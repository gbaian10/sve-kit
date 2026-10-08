"""Owner isolation and fail-closed scope at the optional name-use boundary."""

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- deferred graph checks run at transaction exit; exact messages identify the intended guard

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.translations.names.sources import NameOwner, name_source

from .name_build_fixtures import template

if TYPE_CHECKING:
    from .database_fixtures import DatabaseTemplate

REVISION = NameOwner("face_revision", "revision")
PRINTING = NameOwner("printing_face", "printing", "face")
ONE_OWNER = (
    "(face_revision_id IS NOT NULL) + (printing_id IS NOT NULL) + "
    "(qa_version_id IS NOT NULL) + (cr_clause_id IS NOT NULL) + "
    "(vocabulary_kind IS NOT NULL) + (keyword_id IS NOT NULL) + "
    "(product_family_id IS NOT NULL) + (product_id IS NOT NULL) = 1"
)


@pytest.fixture(scope="module")
def names() -> DatabaseTemplate:
    return template()


def _assert_error(message: str) -> str:
    return "^" + re.escape(message) + "$"


@pytest.mark.parametrize(
    "owner",
    [
        NameOwner("face_revision", "missing"),
        NameOwner("printing_face", "missing", "face"),
    ],
)
def test_absent_owner_is_not_missing_translation(
    names: DatabaseTemplate, owner: NameOwner
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            ValueError, match=_assert_error("Name build owner or source row is absent")
        ),
    ):
        name_source(db, owner)


def test_absent_printing_face_is_rejected(names: DatabaseTemplate) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            ValueError, match=_assert_error("Name build printing face is absent")
        ),
    ):
        name_source(db, NameOwner("printing_face", "printing", "back"))


@pytest.mark.parametrize(
    ("kind", "identifier", "face"),
    [
        ("face_revision", "", None),
        ("face_revision", "revision", "face"),
        ("printing_face", "printing", None),
        ("printing_face", "printing", ""),
        ("unknown", "revision", None),
    ],
)
def test_owner_shape_is_closed(kind: str, identifier: str, face: str | None) -> None:
    with pytest.raises(ValueError, match=_assert_error("Invalid name build owner")):
        NameOwner(kind, identifier, face)  # type: ignore[arg-type]


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("missing_name", "Known printed name is missing its own source"),
        ("wrong_parent", "Name build printing face belongs to another card"),
        ("wrong_face_parent", "Name build printing face belongs to another card"),
        ("wrong_language", "Name build source language differs from owner region"),
        ("wrong_hash", "Name build source exact hash mismatch"),
        ("missing_unit", "Name build owner or source row is absent"),
    ],
)
def test_printed_source_is_revalidated(
    names: DatabaseTemplate, mutation: str, message: str
) -> None:
    with (
        names.copy() as db,
        pytest.raises(ValueError, match=_assert_error(message)),
        db.transaction(),
    ):
        if mutation == "missing_name":
            db.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"printed_name_unit_id": None},
            )
        elif mutation in {"wrong_parent", "wrong_face_parent"}:
            db._connection.execute("PRAGMA defer_foreign_keys = ON")
            db.insert(
                "card",
                {
                    "id": "other",
                    "layout": "single",
                    "identity_state": "confirmed",
                    "home_set_id": "family",
                },
            )
            table, identifier = (
                ("printing", "printing")
                if mutation == "wrong_parent"
                else ("face", "face")
            )
            db.update(table, {"id": identifier}, {"card_id": "other"})
        elif mutation == "wrong_language":
            db.update("text_unit", {"id": "printed-name"}, {"lang": "en"})
        elif mutation == "missing_unit":
            db.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"printed_name_unit_id": "absent"},
            )
        else:
            db.update(
                "text_unit",
                {"id": "printed-name"},
                {"text": "Same hash different text"},
            )
        name_source(db, PRINTING)


def test_source_names_are_owner_local(names: DatabaseTemplate) -> None:
    with names.copy() as db:
        front = name_source(db, REVISION)
        printed = name_source(db, PRINTING)
        back = name_source(db, NameOwner("face_revision", "back-revision"))
        english = name_source(db, NameOwner("face_revision", "english-revision"))
        assert front is not None
        assert front.text == "Synthetic text"
        assert printed is not None
        assert printed.text == "Synthetic old name"
        assert back is not None
        assert back.text == "Synthetic back name"
        assert english is not None
        assert english.lang == "en"


@pytest.mark.parametrize("state", ["unknown", "omitted"])
def test_unknown_printed_source_is_unavailable(
    names: DatabaseTemplate, state: str
) -> None:
    with names.copy() as db, db.transaction():
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"printed_text_state": state},
        )
        assert name_source(db, PRINTING) is None


@pytest.mark.parametrize(
    "state", ["verified", "derived_no_errata", "derived_from_errata"]
)
def test_known_printed_source_is_available(names: DatabaseTemplate, state: str) -> None:
    with names.copy() as db, db.transaction():
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"printed_text_state": state},
        )
        assert name_source(db, PRINTING) is not None

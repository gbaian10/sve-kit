"""Direct whole-field translations share one context per source text and never merge conflicts."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.translations.direct import write

from ...support.build_db_fixtures import seed
from ...support.database_fixtures import DatabaseTemplate

if TYPE_CHECKING:
    from sve_carddb.build import Database

FOLLOWER = {"vocabulary_kind": "type", "vocabulary_code": "follower"}
SYNTHETIC = {"vocabulary_kind": "class", "vocabulary_code": "synthetic"}


@pytest.fixture(scope="module")
def template() -> DatabaseTemplate:
    schema = compile_build(("t0", "translation_evidence", "translation_names"))
    with create_database(schema) as db:
        seed(db)
        return DatabaseTemplate(schema, db._connection.serialize())


def put(db: Database, owner: dict[str, str], text: str = "從者") -> None:
    with db.transaction():
        write(
            db,
            owner,
            field="label",
            lang="zh-Hant",
            source_unit_id="text",
            text=text,
            origin="machine",
            low_confidence=True,
        )


def test_two_owners_of_one_text_share_context_and_translation(
    template: DatabaseTemplate,
) -> None:
    with template.copy() as db:
        put(db, FOLLOWER)
        put(db, SYNTHETIC)
        assert len(db.rows("translation_use")) == 2
        assert len(db.rows("translation_context")) == 1
        translation = db.rows("translation")
        assert len(translation) == 1
        assert translation[0].values["text"] == "從者"
        assert translation[0].values["origin"] == "machine"
        assert translation[0].values["authority"] == "unofficial"
        assert translation[0].values["low_confidence"] is True
        assert len(db.rows("translation_selection")) == 1


def test_repeating_the_same_owner_is_idempotent(template: DatabaseTemplate) -> None:
    with template.copy() as db:
        put(db, FOLLOWER)
        put(db, FOLLOWER)
        assert len(db.rows("translation_use")) == 1


def test_different_text_for_one_source_is_rejected(template: DatabaseTemplate) -> None:
    with template.copy() as db:
        put(db, FOLLOWER)
        with pytest.raises(ValueError, match="two direct translations"):
            put(db, SYNTHETIC, "別的")


@pytest.mark.parametrize("owner", [{}, {"no_such_column": "x"}])
def test_unknown_owner_columns_are_rejected(
    template: DatabaseTemplate, owner: dict[str, str]
) -> None:
    with template.copy() as db, pytest.raises(ValueError, match="owner columns"):
        put(db, owner)


def test_absent_source_text_is_rejected(template: DatabaseTemplate) -> None:
    with (
        template.copy() as db,
        db.transaction(),
        pytest.raises(ValueError, match="text unit is absent"),
    ):
        write(
            db,
            FOLLOWER,
            field="label",
            lang="zh-Hant",
            source_unit_id="missing",
            text="x",
            origin="machine",
            low_confidence=False,
        )


@pytest.mark.parametrize("mutation", ["owner", "field", "source", "origin"])
def test_official_direct_translation_requires_a_sourced_label(
    template: DatabaseTemplate, mutation: str
) -> None:
    message = (
        "Unofficial direct translation cannot claim an official source"
        if mutation == "origin"
        else "Official direct translation requires a sourced vocabulary, family or keyword label"
    )
    with (
        template.copy() as db,
        db.transaction(),
        pytest.raises(ValueError, match=message),
    ):
        write(
            db,
            {"face_revision_id": "revision"} if mutation == "owner" else FOLLOWER,
            field="name" if mutation == "field" else "label",
            lang="en",
            source_unit_id="text",
            text="Synthetic label",
            origin="project" if mutation == "origin" else "official",
            source_id=None if mutation == "source" else "source",
            low_confidence=False,
        )


def test_official_card_name_still_cannot_enter_the_common_selection(
    template: DatabaseTemplate,
) -> None:
    with template.copy() as db:
        put(db, {"face_revision_id": "revision"})
        translation = db.rows("translation")[0].values
        with (
            pytest.raises(
                sqlite3.IntegrityError, match="name_selection_owner_eligibility"
            ),
            db.transaction(),
        ):
            db.update(
                "translation",
                {"id": translation["id"]},
                {
                    "origin": "official",
                    "authority": "sve_official",
                    "source_id": "source",
                },
            )


@pytest.mark.parametrize("owner", [FOLLOWER, {"product_family_id": "family"}])
def test_sourced_official_vocabulary_and_family_labels_keep_sve_authority(
    template: DatabaseTemplate, owner: dict[str, str]
) -> None:
    with template.copy() as db:
        with db.transaction():
            write(
                db,
                owner,
                field="label",
                lang="en",
                source_unit_id="text",
                text="Synthetic official label",
                origin="official",
                source_id="source",
                low_confidence=False,
            )
        selected = db.rows("translation")[0].values
        assert selected["authority"] == "sve_official"
        assert selected["source_id"] == "source"


def test_official_keyword_label_requires_source(template: DatabaseTemplate) -> None:
    with template.copy() as db, pytest.raises(ValueError, match="sourced"):
        write(
            db,
            {"keyword_id": "kw:synthetic"},
            field="label",
            lang="zh-Hant",
            source_unit_id="text",
            text="合成關鍵字",
            origin="official",
            low_confidence=False,
        )

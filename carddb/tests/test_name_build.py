"""Owner isolation and fail-closed scope at the optional name-use boundary."""

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- deferred graph checks run at transaction exit; exact messages identify the intended guard

import re
import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database, rebuild_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_db.t2_translation import OWNERS
from sve_carddb.translations.name_build import (
    NameOwner,
    bind_name_use,
    default_name_context,
    name_source,
    select_unofficial_name,
)

from .build_db_fixtures import INSTANT, seed
from .name_build_fixtures import populate, template

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database, Value

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


def _context(db: Database, owner: NameOwner = REVISION) -> str:
    identifier = default_name_context(db, owner)
    assert identifier is not None
    return identifier


def _translation(db: Database, context: str) -> None:
    source = next(
        r.values for r in db.rows("translation_context") if r.values["id"] == context
    )
    unit = next(
        r.values
        for r in db.rows("text_unit")
        if r.values["id"] == source["source_unit_id"]
    )
    db.insert(
        "translation",
        {
            "id": "translated",
            "context_id": context,
            "target_lang": "zh-Hant",
            "revision": 1,
            "text": "合成譯名",
            "origin": "machine",
            "authority": "unofficial",
            "status": "reviewed",
            "source_hash": unit["content_hash"],
            "translated_by": "Synthetic",
            "translated_at": INSTANT,
            "decision_id": "decision",
        },
    )


def _use(context: str, **changes: Value) -> dict[str, Value]:
    values: dict[str, Value] = {name: None for group in OWNERS for name in group}
    values.update(
        id="use",
        context_id=context,
        field="name",
        ordinal=None,
        face_revision_id="revision",
    )
    return values | changes


def _assert_error(message: str) -> str:
    return "^" + re.escape(message) + "$"


def test_current_printed_back_and_english_have_their_own_names(
    names: DatabaseTemplate,
) -> None:
    with names.copy() as db:
        revision_source = name_source(db, REVISION)
        printed_source = name_source(db, PRINTING)
        assert revision_source is not None
        assert printed_source is not None
        assert revision_source.text == "Synthetic text"
        assert printed_source.text == "Synthetic old name"
        back = name_source(db, NameOwner("face_revision", "back-revision"))
        english = name_source(db, NameOwner("face_revision", "english-revision"))
        assert back is not None
        assert back.text == "Synthetic back name"
        assert english is not None
        assert english.lang == "en"
        with db.transaction():
            current = _context(db)
            assert current == _context(db)
            printed = _context(db, PRINTING)
            assert current != printed
            assert bind_name_use(db, REVISION, current) == bind_name_use(
                db, REVISION, current
            )
            bind_name_use(db, PRINTING, printed)
            _translation(db, current)
            select_unofficial_name(db, current, "zh-Hant", "translated")
        assert len(db.rows("translation_use")) == 2
        assert (
            db.rows("translation_selection")[0].values["translation_id"] == "translated"
        )


def test_contract_context_and_use_id_golden(names: DatabaseTemplate) -> None:
    with names.copy() as db, db.transaction():
        context = _context(db)
        assert (
            context
            == "ctx:83617c3185d7566f835633d2d00498945dbb9db43467ce0e502994df7b0df8a5"
        )
        assert (
            bind_name_use(db, REVISION, context)
            == "use:5770cff9d515afc0612c197fcf9cfd87a9eacdbae3751275b0fb4f2da54783c3"
        )
        printed = _context(db, PRINTING)
        assert (
            printed
            == "ctx:c15a3583b7faea70067189e589bce9dbac8a0575cb73bf9e2b76fd8a7d88b2fa"
        )
        assert (
            bind_name_use(db, PRINTING, printed)
            == "use:68ea89344624932785d4243f857a2ab2ab2d78a6401c19e9901aea91f49f7d04"
        )


def test_same_source_unofficial_names_share_context_but_keep_distinct_uses(
    names: DatabaseTemplate,
) -> None:
    with names.copy() as db, db.transaction():
        db.update("face_revision", {"id": "back-revision"}, {"name_unit_id": "text"})
        back = NameOwner("face_revision", "back-revision")
        context = _context(db)
        assert _context(db, back) == context
        front_use = bind_name_use(db, REVISION, context)
        back_use = bind_name_use(db, back, context)
        assert front_use != back_use
        _translation(db, context)
        select_unofficial_name(db, context, "zh-Hant", "translated")


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


@pytest.mark.parametrize("state", ["unknown", "omitted"])
def test_unknown_printed_name_never_borrows_current(
    names: DatabaseTemplate, state: str
) -> None:
    with names.copy() as db:
        with db.transaction():
            db.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"printed_text_state": state},
            )
            assert name_source(db, PRINTING) is None
            assert default_name_context(db, PRINTING) is None
        with (
            pytest.raises(
                ValueError,
                match=_assert_error(
                    "Unknown printed name cannot acquire a translation use"
                ),
            ),
            db.transaction(),
        ):
            bind_name_use(db, PRINTING, _context(db))
        assert not db.rows("translation_context")
        assert not db.rows("translation_use")


@pytest.mark.parametrize(
    "state", ["verified", "derived_no_errata", "derived_from_errata"]
)
def test_known_printed_states_are_accepted(names: DatabaseTemplate, state: str) -> None:
    with names.copy() as db, db.transaction():
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"printed_text_state": state},
        )
        bind_name_use(db, PRINTING, _context(db, PRINTING))


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


@pytest.mark.parametrize(
    "owner", [PRINTING, NameOwner("face_revision", "back-revision")]
)
def test_context_hit_does_not_bypass_owner_source(
    names: DatabaseTemplate, owner: NameOwner
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            ValueError,
            match=_assert_error("Name use context does not match its owner source"),
        ),
        db.transaction(),
    ):
        bind_name_use(db, owner, _context(db))


def test_nondefault_assignment_is_not_enabled_by_a_decision_row(
    names: DatabaseTemplate,
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            ValueError,
            match=_assert_error(
                "Name variants require complete assignment replay support"
            ),
        ),
        db.transaction(),
    ):
        context = _context(db)
        db.update(
            "translation_context",
            {"id": context},
            {"semantic_variant": "another", "decision_id": "decision"},
        )
        db.update("decision", {"id": "decision"}, {"category": "context_assignment"})
        bind_name_use(db, REVISION, context)


@pytest.mark.parametrize(
    ("authority", "origin"),
    [("digital_official", "official_sv1"), ("sve_official", "official_sve")],
)
def test_official_translation_cannot_become_shared_selection(
    names: DatabaseTemplate, authority: str, origin: str
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            ValueError,
            match=_assert_error(
                "Shared official name selection requires owner eligibility checks"
            ),
        ),
        db.transaction(),
    ):
        context = _context(db)
        _translation(db, context)
        db.update(
            "translation",
            {"id": "translated"},
            {"authority": authority, "origin": origin},
        )
        select_unofficial_name(db, context, "zh-Hant", "translated")


@pytest.mark.parametrize("failure", ["context", "language", "draft", "stale"])
def test_selection_is_exact_and_reviewed(names: DatabaseTemplate, failure: str) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            ValueError,
            match=_assert_error(
                "Name selection must match an exact reviewed context and language"
            ),
        ),
        db.transaction(),
    ):
        context = _context(db)
        _translation(db, context)
        selected_context = _context(db, PRINTING) if failure == "context" else context
        language = "en" if failure == "language" else "zh-Hant"
        if failure in {"draft", "stale"}:
            db.update("translation", {"id": "translated"}, {"status": failure})
        select_unofficial_name(db, selected_context, language, "translated")


@pytest.mark.parametrize(
    ("changes", "check"),
    [
        ({"field": "effect"}, "name_use_supported_scope"),
        ({"ordinal": 0}, "name_use_supported_scope"),
        (
            {"face_revision_id": None, "product_id": "product"},
            "name_use_supported_scope",
        ),
        ({"face_revision_id": "other-revision"}, "name_use_revision_source"),
        (
            {"face_revision_id": None, "printing_id": "printing", "face_id": "face"},
            "name_use_printed_source",
        ),
    ],
)
def test_direct_writes_still_verify_scope_and_owner_source(
    names: DatabaseTemplate, changes: dict[str, Value], check: str
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            sqlite3.IntegrityError,
            match=_assert_error("Cross-table check failed: " + check),
        ),
        db.transaction(),
    ):
        db.insert("translation_use", _use(_context(db), **changes))


@pytest.mark.parametrize("state", ["unknown", "omitted", "missing_name"])
def test_direct_printed_use_cannot_bypass_known_source(
    names: DatabaseTemplate, state: str
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            sqlite3.IntegrityError,
            match=_assert_error("Cross-table check failed: name_use_printed_source"),
        ),
        db.transaction(),
    ):
        context = _context(db, PRINTING)
        changes: dict[str, Value] = (
            {"printed_name_unit_id": None}
            if state == "missing_name"
            else {"printed_text_state": state}
        )
        db.update(
            "printing_face", {"printing_id": "printing", "face_id": "face"}, changes
        )
        db.insert(
            "translation_use",
            _use(
                context, face_revision_id=None, printing_id="printing", face_id="face"
            ),
        )


def test_direct_nondefault_use_is_rejected(names: DatabaseTemplate) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            sqlite3.IntegrityError,
            match=_assert_error("Cross-table check failed: name_use_adopted_variant"),
        ),
        db.transaction(),
    ):
        context = _context(db)
        db.update(
            "translation_context",
            {"id": context},
            {"semantic_variant": "another", "decision_id": "decision"},
        )
        db.insert("translation_use", _use(context))


def _adopted_variant_use(db: Database, *, fault: str | None = None) -> None:
    context = _context(db)
    db.update(
        "decision",
        {"id": "decision"},
        {
            "category": "context_assignment",
            "reviewed_by": "Other reviewer" if fault == "reviewer" else "gbaian10",
        },
    )
    db.update(
        "decision_source",
        {"decision_id": "decision", "source_id": "source", "role": "synthetic"},
        {"role": "synthetic" if fault == "identity" else "name_identity:synthetic"},
    )
    db.update(
        "translation_context",
        {"id": context},
        {"semantic_variant": "another", "decision_id": "decision"},
    )
    db.insert("translation_use", _use(context))


def test_direct_nondefault_use_accepts_adopted_identity(
    names: DatabaseTemplate,
) -> None:
    with names.copy() as db, db.transaction():
        _adopted_variant_use(db)


@pytest.mark.parametrize("fault", ["reviewer", "identity"])
def test_direct_nondefault_use_requires_maintainer_and_identity_audit(
    names: DatabaseTemplate, fault: str
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            sqlite3.IntegrityError,
            match=_assert_error("Cross-table check failed: name_use_adopted_variant"),
        ),
        db.transaction(),
    ):
        _adopted_variant_use(db, fault=fault)


@pytest.mark.parametrize(
    ("failure", "check"),
    [
        ("context", "translation_selection_exact"),
        ("language", "translation_selection_exact"),
        ("draft", "translation_selection_exact"),
        ("stale", "translation_selection_exact"),
        ("official", "name_selection_owner_eligibility"),
    ],
)
def test_direct_selection_cannot_bypass_validation(
    names: DatabaseTemplate, failure: str, check: str
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            sqlite3.IntegrityError,
            match=_assert_error("Cross-table check failed: " + check),
        ),
        db.transaction(),
    ):
        context = _context(db)
        _translation(db, context)
        if failure in {"draft", "stale"}:
            db.update("translation", {"id": "translated"}, {"status": failure})
        if failure == "official":
            db.update(
                "translation",
                {"id": "translated"},
                {"origin": "official_sv1", "authority": "digital_official"},
            )
        db.insert(
            "translation_selection",
            {
                "context_id": _context(db, PRINTING)
                if failure == "context"
                else context,
                "target_lang": "en" if failure == "language" else "zh-Hant",
                "translation_id": "translated",
            },
        )


@pytest.mark.parametrize(
    ("changes", "constraint"),
    [
        ({"face_revision_id": None}, ONE_OWNER),
        ({"printing_id": "printing", "face_id": "face"}, ONE_OWNER),
        (
            {"face_revision_id": None, "printing_id": "printing"},
            "(printing_id IS NULL) = (face_id IS NULL)",
        ),
        (
            {"vocabulary_kind": "type"},
            "(vocabulary_kind IS NULL) = (vocabulary_code IS NULL)",
        ),
    ],
)
def test_exactly_one_complete_owner_group(
    names: DatabaseTemplate, changes: dict[str, Value], constraint: str
) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            sqlite3.IntegrityError,
            match=_assert_error("CHECK constraint failed: " + constraint),
        ),
        db.transaction(),
    ):
        db.insert("translation_use", _use(_context(db), **changes))


def test_null_ordinal_still_has_an_owner_unique_key(names: DatabaseTemplate) -> None:
    with (
        names.copy() as db,
        pytest.raises(
            sqlite3.IntegrityError,
            match=_assert_error(
                "UNIQUE constraint failed: translation_use.face_revision_id, translation_use.field"
            ),
        ),
        db.transaction(),
    ):
        context = _context(db)
        db.insert("translation_use", _use(context))
        db.insert("translation_use", _use(context, id="another-use"))


def test_disabled_capability_retains_old_minimum_tables() -> None:
    old = {table.name for table in compile_build().tables}
    enabled = {table.name for table in compile_build(("translation_names",)).tables}
    assert {"translation_use", "translation_selection"} <= enabled
    assert not {"translation_use", "translation_selection"} & old


def test_rebuild_adds_name_tables_and_failure_preserves_old_file(
    tmp_path: Path,
) -> None:
    path = tmp_path / "build.sqlite"
    with create_database(compile_t0(), path) as db:
        seed(db)
    original = path.read_bytes()

    def invalid(db: Database) -> None:
        populate(db)
        db.insert(
            "translation_use", _use(_context(db), face_revision_id="other-revision")
        )

    schema = compile_build(("translation_names",))
    with pytest.raises(
        sqlite3.IntegrityError,
        match=_assert_error("Cross-table check failed: name_use_revision_source"),
    ):
        rebuild_database(schema, path, invalid)
    assert path.read_bytes() == original
    assert list(tmp_path.iterdir()) == [path]
    rebuild_database(schema, path, populate)
    with sqlite3.connect(path) as connection:
        assert connection.execute("PRAGMA user_version").fetchall() == [(5,)]
        assert connection.execute("SELECT * FROM translation_use").fetchall() == []
        assert connection.execute("PRAGMA foreign_key_check").fetchall() == []

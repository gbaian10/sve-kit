"""Catalog reference, language, configuration and decision counterexamples."""

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- setup mutations must be in the same rollback transaction as the failing projection

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Json
from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.importer import (
    catalog_configuration,
    populate_catalog,
    resolve_alias,
)
from sve_carddb.catalog.languages import register_languages
from sve_carddb.catalog.models import Alias, Catalog, NameBinding, Term
from sve_carddb.products.models import Language, LocalizedText

from .database_fixtures import DatabaseTemplate
from .test_catalog_symbols import symbol

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import Database


@pytest.fixture(scope="module")
def catalog_database_template(
    t0_database_template: DatabaseTemplate,
) -> DatabaseTemplate:
    with t0_database_template.copy() as database:
        with database.transaction():
            database.update(
                "rules_name", {"id": "rules_name"}, {"official_name": "Synthetic text"}
            )
        return DatabaseTemplate(
            t0_database_template.schema, database._connection.serialize()
        )


@pytest.fixture
def db(catalog_database_template: DatabaseTemplate) -> Iterator[Database]:
    with catalog_database_template.copy() as database:
        yield database


def catalog() -> Catalog:
    return Catalog(
        languages=(
            Language(code="en", fallback_order=(), display_name="English"),
            Language(
                code="zh-Hant", fallback_order=("ja", "en"), display_name="繁體中文"
            ),
        ),
        terms=tuple(
            Term(kind="trait", code=code, label=LocalizedText(lang="ja", text=code))
            for code in ("first", "second")
        ),
        aliases=tuple(
            Alias(
                kind="trait",
                code=code,
                lang="ja",
                text=text,
                normalized=text.casefold(),
            )
            for code, text in (
                ("first", "Shared"),
                ("second", "shared"),
                ("second", "first"),
            )
        ),
        symbols=(),
        normalizer_version="synthetic-explicit-v1",
    )


def context(value: Catalog) -> BuildContext:
    return BuildContext.from_inputs(
        "a" * 40, {"synthetic.lock": b"lock"}, catalog_configuration(value)
    )


def test_catalog_aliases_are_multivalued_and_canonical_codes_win(db: Database) -> None:
    value = catalog()
    with db.transaction():
        populate_catalog(db, value, build=context(value), published=())
        populate_catalog(db, value, build=context(value), published=())
    assert resolve_alias(
        db,
        "trait",
        "ja",
        "Shared",
        normalized="shared",
        normalizer_version=value.normalizer_version,
    ) == ("first", "second")
    assert resolve_alias(
        db,
        "trait",
        "ja",
        "first",
        normalized="first",
        normalizer_version=value.normalizer_version,
    ) == ("first",)
    with pytest.raises(ValueError, match="normalizer"):
        resolve_alias(
            db,
            "trait",
            "ja",
            "Shared",
            normalized="shared",
            normalizer_version="unknown",
        )
    assert len(db.rows("text_symbol")) == 1


@pytest.mark.parametrize(
    "kind", ["keyword", "stamp", "unknown", "card", "product_family", "trait"]
)
def test_alias_invalid_target_rolls_back_every_catalog_row(
    db: Database, kind: str
) -> None:
    value = catalog().model_copy(
        update={
            "aliases": (
                Alias(
                    kind=kind, code="missing", lang="ja", text="Bad", normalized="bad"
                ),
            )
        }
    )
    before = db.rows("vocabulary")
    with pytest.raises(ValueError, match="target"), db.transaction():
        populate_catalog(db, value, build=context(value), published=())
    assert db.rows("vocabulary") == before
    assert len(db.rows("language")) == 1


@pytest.mark.parametrize("fallback", [("en",), ("fr",), ("zh-Hant",)])
def test_language_fallback_cannot_self_reference_or_escape_registration(
    db: Database, fallback: tuple[str, ...]
) -> None:
    value = catalog().model_copy(
        update={
            "languages": (
                Language(code="en", fallback_order=fallback, display_name="English"),
            )
        }
    )
    with pytest.raises(ValueError, match="fallback"), db.transaction():
        populate_catalog(db, value, build=context(value), published=())
    assert len(db.rows("language")) == 1


@pytest.mark.parametrize(
    "mutation",
    ["unpinned", "unconfirmed", "no_evidence", "raw_only", "unknown_language"],
)
def test_symbol_cannot_bypass_pins_and_adoption(
    db: Database, mutation: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    value = catalog().model_copy(update={"symbols": (symbol(),)})
    build = context(value)
    if mutation == "unpinned":
        build = BuildContext.from_inputs("a" * 40, {"synthetic.lock": b"lock"}, {})
    with (
        pytest.raises(ValueError, match=r"configuration|decision|evidence|language"),
        db.transaction(),
    ):
        if mutation == "unconfirmed":
            db.update("decision", {"id": "decision"}, {"state": "proposed"})
        elif mutation == "no_evidence":
            db.delete(
                "decision_source",
                {"decision_id": "decision", "source_id": "source", "role": "synthetic"},
            )
        elif mutation == "raw_only":
            db.update(
                "source_record",
                {"id": "source"},
                {
                    "kind": "official_page",
                    "url": "https://example.com",
                    "fetched_at": "2026-09-29T00:00:00Z",
                },
            )
        elif mutation == "unknown_language":
            # Isolate downstream projection from the deliberately closed adoption gate.
            monkeypatch.setattr("sve_carddb.catalog.importer._adopted", lambda *_: None)
            value = value.model_copy(
                update={
                    "symbols": (
                        symbol().model_copy(
                            update={
                                "spellings": tuple(
                                    s.model_copy(update={"lang": "fr"})
                                    for s in symbol().spellings
                                )
                            }
                        ),
                    )
                }
            )
            build = context(value)
        populate_catalog(db, value, build=build, published=())
    assert len(db.rows("text_symbol")) == 1


def test_native_alias_validation_rejects_reserved_kind_spoofing(db: Database) -> None:
    with pytest.raises(ValueError, match="target"), db.transaction():
        db.insert(
            "vocabulary",
            {
                "kind": "keyword",
                "code": "spoof",
                "label_unit_id": "text",
                "active": True,
            },
        )
        db.insert(
            "search_alias",
            {
                "kind": "keyword",
                "code": "spoof",
                "lang": "ja",
                "text": "Alias",
                "normalized": "alias",
                "normalizer_version": "synthetic",
            },
        )


def test_published_text_union_cannot_change_without_a_new_pin(db: Database) -> None:
    value = catalog()
    with pytest.raises(ValueError, match="configuration"), db.transaction():
        populate_catalog(
            db,
            value,
            build=context(value),
            published=(LocalizedText(lang="ja", text="Historical synthetic text"),),
        )


def test_language_config_is_extensible_and_duplicates_conflicts_fail(
    db: Database,
) -> None:
    value = catalog().model_copy(
        update={
            "languages": (
                Language(code="fr", fallback_order=("ja",), display_name="French"),
            )
        }
    )
    with db.transaction():
        populate_catalog(db, value, build=context(value), published=())
    assert any(row.values["code"] == "fr" for row in db.rows("language"))
    conflicting = value.model_copy(
        update={
            "languages": (
                Language(code="fr", fallback_order=(), display_name="French"),
            )
        }
    )
    with pytest.raises(ValueError, match="Conflicting language"), db.transaction():
        populate_catalog(db, conflicting, build=context(conflicting), published=())
    duplicates = value.model_copy(
        update={"languages": (value.languages[0], value.languages[0])}
    )
    with pytest.raises(ValueError, match="Duplicate language"), db.transaction():
        populate_catalog(db, duplicates, build=context(duplicates), published=())


@pytest.mark.parametrize("lang", ["ja", "en"])
def test_registered_chinese_cannot_be_ui_fallback(db: Database, lang: str) -> None:
    with db.transaction():
        register_languages(db, catalog().languages)
    with pytest.raises(ValueError, match="Traditional Chinese"), db.transaction():
        db.update("language", {"code": lang}, {"fallback_order": Json(["zh-Hant"])})
        register_languages(db, ())


@pytest.mark.parametrize("row_kind", ["term", "alias", "symbol"])
def test_existing_catalog_rows_require_identical_content(
    db: Database, row_kind: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    # A synthetic verifier isolates exact projection reuse; it grants no real adoption.
    monkeypatch.setattr("sve_carddb.catalog.importer._adopted", lambda *_: None)
    value = catalog().model_copy(update={"symbols": (symbol(),)})
    with db.transaction():
        populate_catalog(db, value, build=context(value), published=())
        populate_catalog(db, value, build=context(value), published=())
    if row_kind == "term":
        changed = value.model_copy(
            update={
                "terms": (
                    value.terms[0].model_copy(update={"active": False}),
                    *value.terms[1:],
                )
            }
        )
    elif row_kind == "alias":
        changed = value.model_copy(
            update={
                "aliases": (
                    value.aliases[0].model_copy(update={"normalized": "different"}),
                    *value.aliases[1:],
                )
            }
        )
    else:
        changed = value.model_copy(
            update={"symbols": (symbol().model_copy(update={"code": "different"}),)}
        )
    before = tuple(
        db.rows(table) for table in ("vocabulary", "search_alias", "text_symbol")
    )
    with pytest.raises(ValueError, match="Conflicting catalog row"), db.transaction():
        populate_catalog(db, changed, build=context(changed), published=())
    assert before == tuple(
        db.rows(table) for table in ("vocabulary", "search_alias", "text_symbol")
    )


@pytest.mark.parametrize("entry", ["alias", "symbol", "name"])
@pytest.mark.parametrize("change", ["unchanged", "member", "source_hash"])
@pytest.mark.parametrize(
    "category", ["identity", "product_family", "vocabulary", "text_symbol"]
)
def test_confirmed_decision_cannot_prove_catalog_adoption(
    db: Database, entry: str, category: str, change: str
) -> None:
    value = catalog()
    if entry == "alias":
        value = value.model_copy(
            update={
                "aliases": (
                    value.aliases[0].model_copy(update={"decision_id": "decision"}),
                )
            }
        )
    elif entry == "symbol":
        value = value.model_copy(update={"symbols": (symbol(),)})
    else:
        value = value.model_copy(
            update={
                "names": (
                    NameBinding(
                        face_id="face",
                        region="jp",
                        official_name="Synthetic text",
                        role="collab",
                        decision_id="decision",
                    ),
                )
            }
        )
    if change == "member":
        if entry == "alias":
            value = value.model_copy(
                update={
                    "aliases": (value.aliases[0].model_copy(update={"code": "second"}),)
                }
            )
        elif entry == "symbol":
            value = value.model_copy(
                update={
                    "symbols": (
                        value.symbols[0].model_copy(update={"code": "another_member"}),
                    )
                }
            )
        else:
            value = value.model_copy(
                update={
                    "names": (
                        value.names[0].model_copy(
                            update={"official_name": "Another member"}
                        ),
                    )
                }
            )
    before = tuple(
        db.rows(table)
        for table in ("vocabulary", "search_alias", "text_symbol", "face_rules_name")
    )
    with (
        pytest.raises(ValueError, match="category, exact members or freshness"),
        db.transaction(),
    ):
        db.update("decision", {"id": "decision"}, {"category": category})
        if change == "source_hash":
            db.update(
                "source_record", {"id": "source"}, {"sha256": "sha256:" + "b" * 64}
            )
        populate_catalog(db, value, build=context(value), published=())
    assert before == tuple(
        db.rows(table)
        for table in ("vocabulary", "search_alias", "text_symbol", "face_rules_name")
    )


def test_missing_decision_source_is_reported_before_contract_gate(db: Database) -> None:
    value = catalog().model_copy(update={"symbols": (symbol(),)})
    with (
        pytest.raises(ValueError, match="confirmed sourced decision"),
        db.transaction(),
    ):
        db.delete(
            "decision_source",
            {"decision_id": "decision", "source_id": "source", "role": "synthetic"},
        )
        populate_catalog(db, value, build=context(value), published=())


def test_alias_resolution_is_scoped_to_requested_language(db: Database) -> None:
    value = catalog()
    value = value.model_copy(
        update={
            "aliases": (
                Alias(
                    kind="trait",
                    code="first",
                    lang="ja",
                    text="Shared",
                    normalized="shared",
                ),
                Alias(
                    kind="trait",
                    code="second",
                    lang="en",
                    text="Shared",
                    normalized="shared",
                ),
            )
        }
    )
    with db.transaction():
        populate_catalog(db, value, build=context(value), published=())
    for lang, expected in (("ja", ("first",)), ("en", ("second",)), ("zh-Hant", ())):
        assert (
            resolve_alias(
                db,
                "trait",
                lang,
                "Shared",
                normalized="shared",
                normalizer_version=value.normalizer_version,
            )
            == expected
        )

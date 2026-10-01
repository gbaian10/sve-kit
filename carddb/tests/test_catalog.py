"""Catalog reference, language, configuration and decision counterexamples."""

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- setup mutations must be in the same rollback transaction as the failing projection

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.importer import (
    catalog_configuration,
    populate_catalog,
    resolve_alias,
)
from sve_carddb.catalog.models import Alias, Catalog, Term
from sve_carddb.products.models import Language, LocalizedText

from .build_db_fixtures import seed
from .test_catalog_symbols import symbol

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import Database


@pytest.fixture
def db() -> Iterator[Database]:
    with create_database(compile_t0()) as database:
        seed(database)
        with database.transaction():
            database.update(
                "rules_name", {"id": "rules_name"}, {"official_name": "Synthetic text"}
            )
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
        symbols=(symbol(),),
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
    assert len(db.rows("text_symbol")) == 2


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
def test_symbol_cannot_bypass_pins_and_adoption(db: Database, mutation: str) -> None:
    value = catalog()
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

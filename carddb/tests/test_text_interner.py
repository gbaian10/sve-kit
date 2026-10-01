"""Exact byte, language, short-ID and historical-union collision counterexamples."""

import hashlib
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.products.models import LocalizedText
from sve_carddb.text_observations.intern import TextInterner

if TYPE_CHECKING:
    from sve_carddb.build_db import Database


def register(db: Database) -> None:
    for language in ("ja", "en"):
        db.insert(
            "language",
            {"code": language, "display_name": "Synthetic", "fallback_order": Json([])},
        )


def test_exact_utf8_and_language_define_keys_without_normalization() -> None:
    with create_database(compile_t0()) as db, db.transaction():
        register(db)
        texts = TextInterner(db)
        one = texts.intern(LocalizedText(lang="ja", text="é"))
        assert one == "t:ja:" + hashlib.sha256("é".encode()).hexdigest()[:16]
        assert texts.intern(LocalizedText(lang="ja", text="é")) == one
        assert texts.intern(LocalizedText(lang="ja", text="e\u0301")) != one
        assert texts.intern(LocalizedText(lang="en", text="é")) != one
        assert len(db.rows("text_unit")) == 3


@pytest.mark.parametrize("kind", ["short", "full"])
def test_existing_collision_compares_exact_bytes_and_never_replaces(kind: str) -> None:
    digest = hashlib.sha256(b"New synthetic").hexdigest()
    with create_database(compile_t0()) as db:
        with db.transaction():
            register(db)
            db.insert(
                "text_unit",
                {
                    "id": "t:ja:" + (digest[:16] if kind == "short" else "0" * 16),
                    "lang": "ja",
                    "text": "Older synthetic",
                    "content_hash": "sha256:"
                    + ("0" * 64 if kind == "short" else digest),
                },
            )
        before = db.rows("text_unit")
        with pytest.raises(ValueError, match="collision"), db.transaction():
            TextInterner(db).intern(LocalizedText(lang="ja", text="New synthetic"))
        assert db.rows("text_unit") == before


@pytest.mark.parametrize("full", [False, True])
def test_published_union_catches_an_unreferenced_historic_collision(
    monkeypatch: pytest.MonkeyPatch, full: bool
) -> None:
    original = hashlib.sha256

    class Hash:
        def __init__(self, value: bytes) -> None:
            self.value = value

        def hexdigest(self) -> str:
            return (
                "a" * 64 if full else "a" * 16 + original(self.value).hexdigest()[16:]
            )

    with create_database(compile_t0()) as db:
        with db.transaction():
            register(db)
        monkeypatch.setattr("sve_carddb.text_observations.intern.hashlib.sha256", Hash)
        texts = TextInterner(
            db, published=(LocalizedText(lang="ja", text="Historic unreferenced"),)
        )
        with pytest.raises(ValueError, match="collision"), db.transaction():
            texts.intern(LocalizedText(lang="ja", text="Current different"))
        assert not db.rows("text_unit")


def test_historical_identical_text_gets_persisted_in_the_new_build() -> None:
    value = LocalizedText(lang="ja", text="Historic exact")
    with create_database(compile_t0()) as db, db.transaction():
        register(db)
        texts = TextInterner(db, published=(value, value))
        assert not db.rows("text_unit")
        identifier = texts.intern(value)
        assert db.rows("text_unit")[0].values["id"] == identifier
        assert len(db.rows("text_unit")) == 1


def test_unregistered_language_has_no_implicit_fallback() -> None:
    with (
        create_database(compile_t0()) as db,
        pytest.raises(ValueError, match="language"),
        db.transaction(),
    ):
        TextInterner(db).intern(LocalizedText(lang="ja", text="Synthetic"))

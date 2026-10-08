"""Independent counterexamples use synthetic names, never official card wording."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.domains.products.models import LocalizedText
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.translations.inputs import load_glossary
from sve_carddb.domains.translations.models import Span
from sve_carddb.domains.translations.sources import excerpt, pointer, project

from .translation_fixtures import choice, envelope, template, term, write

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from .database_fixtures import DatabaseTemplate


@pytest.fixture(scope="module")
def digital_template() -> DatabaseTemplate:
    return template()


@pytest.fixture
def glossary(tmp_path: Path) -> tuple[Path, dict[str, dict[str, JsonValue]]]:
    shards = {
        "translations/glossary/concepts/001.yaml": envelope([term()]),
        "translations/glossary/choices/001.yaml": envelope([choice()]),
    }
    write(tmp_path, shards)
    return tmp_path, shards


def test_same_translated_word_keeps_distinct_concepts(tmp_path: Path) -> None:
    terms = [
        term("resource.ep", category="rule_term"),
        term("card.ep", category="card_name"),
        term("resource.sep", category="rule_term"),
        term("card.sep", category="card_name"),
        term("trait.spirit", category="trait"),
    ]
    choices = [
        choice("resource.ep"),
        choice("card.ep"),
        choice("resource.sep"),
        choice("card.sep"),
        choice("trait.spirit"),
    ]
    write(
        tmp_path,
        {
            "translations/glossary/concepts/001.yaml": envelope(terms),
            "translations/glossary/choices/001.yaml": envelope(choices),
        },
    )

    loaded = load_glossary(tmp_path)
    assert len(loaded.current_records()) == 10
    assert (
        len({r.data.id for r in loaded.current_records() if r.kind == "glossary_term"})
        == 5
    )


def test_identical_labels_coexist_across_glossary_and_vocabulary(
    digital_template: DatabaseTemplate,
) -> None:
    with digital_template.copy() as db, db.transaction():
        interner = TextInterner(db)
        for concept, ja, zh, category in (
            ("resource.ep", "EP", "EP", "rule_term"),
            ("resource.sep", "SEP", "SEP", "rule_term"),
            ("trait.spirit", "仮合成族", "合成族", "trait"),
        ):
            db.insert(
                "glossary_term",
                {
                    "id": "term:" + concept,
                    "concept_key": concept,
                    "source_ja": ja,
                    "category": category,
                    "authored_source_id": "source",
                    "record_key": canonical(
                        ["glossary_term", "term:" + concept]
                    ).decode(),
                    "origin": "project",
                    "low_confidence": False,
                },
            )
            db.insert(
                "glossary_translation",
                {
                    "term_id": "term:" + concept,
                    "lang": "zh-Hant",
                    "text": zh,
                    "origin": "project",
                    "authored_source_id": "source",
                    "record_key": canonical(
                        ["glossary_choice", "term:" + concept, "zh-Hant"]
                    ).decode(),
                    "low_confidence": False,
                },
            )
        for kind, code, text in (
            ("type", "ep", "EP"),
            ("type", "sep", "SEP"),
            ("class", "elf", "合成族"),
        ):
            db.insert(
                "vocabulary",
                {
                    "kind": kind,
                    "code": code,
                    "label_unit_id": interner.intern(
                        LocalizedText(lang="zh-Hant", text=text)
                    ),
                    "active": True,
                },
            )
        assert len(db.rows("glossary_translation")) == 3
        assert {
            (r.values["kind"], r.values["code"]) for r in db.rows("vocabulary")
        } == {
            ("type", "ep"),
            ("type", "sep"),
            ("class", "elf"),
        }


@pytest.mark.parametrize(
    "filename", ["003.yaml.tmp-abc", "notes.txt", "unindexed.yaml"]
)
def test_nondata_files_are_ignored(
    glossary: tuple[Path, dict[str, dict[str, JsonValue]]], filename: str
) -> None:
    root, _ = glossary
    (root / "translations" / filename).write_text("synthetic")
    assert load_glossary(root).current_records()


@pytest.mark.parametrize("state", ["proposed", "model_reviewed"])
def test_high_confidence_does_not_promote_review(
    digital_template: DatabaseTemplate, state: str
) -> None:
    with digital_template.copy() as db:
        with (
            pytest.raises(sqlite3.IntegrityError, match="digital_link_adopted"),
            db.transaction(),
        ):
            db.update(
                "decision",
                {"id": "decision"},
                {
                    "state": state,
                    "confidence": "high",
                },
            )


@pytest.mark.parametrize("span", [(0, 5), (2, 1), (0, 3)])
def test_exact_codepoint_span_rejects_invalid_and_utf16_offsets(
    span: tuple[int, int],
) -> None:
    with pytest.raises(ValueError, match=r"^Invalid exact source span$"):
        excerpt("😀甲", Span(start=span[0], end=span[1]))


def test_exact_span_and_pointer() -> None:
    assert excerpt("😀甲乙", Span(start=1, end=2)) == "甲"
    assert pointer({"a/b": ["synthetic"]}, "/a~1b/0") == "synthetic"


@pytest.mark.parametrize(
    ("locator", "message"),
    [
        ("/a~2b", "Noncanonical JSON Pointer"),
        ("/a~1b/00", "Evidence JSON Pointer is absent"),
        ("/a~1b/2", "Evidence JSON Pointer is absent"),
        ("query", "Evidence locator must be a JSON Pointer"),
    ],
)
def test_pointer_rejection_is_specific(locator: str, message: str) -> None:
    with pytest.raises(ValueError, match="^" + message + "$"):
        pointer({"a/b": ["synthetic"]}, locator)


def test_source_requires_official_endpoint_and_language() -> None:
    raw = canonical({"data": {"errors": []}})
    assert (
        project(raw, "https://shadowverse-portal.com/api/v1/cards?lang=zh-tw", "sv1")[0]
        == "zh-Hant"
    )


@pytest.mark.parametrize(
    ("url", "message"),
    [
        (
            "https://example.invalid/api/v1/cards?lang=ja",
            "Digital evidence source URL mismatch",
        ),
        (
            "https://shadowverse-portal.com/api/v1/cards?lang=ja&lang=en",
            "Digital evidence source URL mismatch",
        ),
        (
            "https://shadowverse-portal.com/api/v1/cards?lang=fr",
            "Unsupported digital source language",
        ),
    ],
)
def test_source_endpoint_rejection_is_specific(url: str, message: str) -> None:
    with pytest.raises(ValueError, match="^" + message + "$"):
        project(canonical({"data": {"errors": []}}), url, "sv1")


@pytest.mark.parametrize("mode", ["absent", "file_link", "parent_link"])
def test_index_must_be_regular_input(tmp_path: Path, mode: str) -> None:
    root = tmp_path / "authored"
    if mode != "absent":
        target = tmp_path / "target"
        write(target, {})
        root.mkdir()
        if mode == "file_link":
            (root / "translations/glossary").mkdir(parents=True)
            (root / "translations/index.yaml").symlink_to(
                target / "translations/index.yaml"
            )
        else:
            (root / "translations").symlink_to(
                target / "translations", target_is_directory=True
            )
    if mode == "parent_link":
        with pytest.raises(ValueError, match=r"^Symlink authored data area$"):
            load_glossary(root)
    elif mode == "absent":
        with pytest.raises(ValueError, match="Missing authored data area"):
            load_glossary(root)
    else:
        assert not load_glossary(root).current_records()


@pytest.mark.parametrize(
    ("game", "raw", "url", "message"),
    [
        (
            "sv1",
            {"data": {"errors": ["Synthetic failure"]}},
            "https://shadowverse-portal.com/api/v1/cards?lang=ja",
            "Frozen sv1 API reports errors",
        ),
        (
            "svwb",
            {"data_headers": {"result_code": 0}},
            "https://shadowverse-wb.com/web/CardList/cardList?lang=ja",
            "Frozen svwb API reports errors",
        ),
        (
            "jp",
            {},
            "https://example.invalid/cardlist/?cardno=SYNTHETIC",
            "JP glossary source URL mismatch",
        ),
    ],
)
def test_frozen_endpoint_failure(
    game: str, raw: JsonValue, url: str, message: str
) -> None:
    with pytest.raises(ValueError, match="^" + message + "$"):
        project(canonical(raw), url, game)

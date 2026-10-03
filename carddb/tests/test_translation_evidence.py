"""Independent counterexamples use synthetic names, never official card wording."""

import copy
import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.products.models import LocalizedText
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.text_observations.intern import TextInterner
from sve_carddb.translations.digital import select_name
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.models import Decision, Span
from sve_carddb.translations.sources import excerpt, pointer, project

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
    assert len(loaded.effective()) == 10
    assert (
        len({r.data.id for r, _ in loaded.records() if r.kind == "glossary_term"}) == 5
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
                    "decision_id": "decision",
                },
            )
            db.insert(
                "glossary_translation",
                {
                    "term_id": "term:" + concept,
                    "lang": "zh-Hant",
                    "text": zh,
                    "origin": "project",
                    "decision_id": "decision",
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
def test_every_unindexed_file_is_rejected(
    glossary: tuple[Path, dict[str, dict[str, JsonValue]]], filename: str
) -> None:
    root, _ = glossary
    (root / "translations" / filename).write_text("synthetic")
    with pytest.raises(ValueError, match="closure"):
        load_glossary(root)


@pytest.mark.parametrize(
    "fault",
    [
        "missing",
        "hash",
        "gap",
        "symlink",
        "unknown",
        "candidate",
        "key",
        "id",
        "members",
        "checked",
        "human",
        "sample",
        "format",
    ],
)
def test_independent_glossary_boundary(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements] -- each signed guard is mutated independently
    glossary: tuple[Path, dict[str, dict[str, JsonValue]]], fault: str
) -> None:
    root, shards = glossary
    name = "translations/glossary/concepts/001.yaml"
    shard = shards[name]
    if fault == "missing":
        (root / name).unlink()
    elif fault == "hash":
        (root / name).write_text("{}")
    elif fault == "gap":
        shards[name.replace("001", "002")] = shards.pop(name)
        (root / name).unlink()
        write(root, shards)
    elif fault == "symlink":
        old = root / name
        old.unlink()
        old.symlink_to(root / "translations/index.yaml")
    else:
        records = shard["records"]
        decisions = shard["decisions"]
        assert isinstance(records, list)
        assert isinstance(records[0], dict)
        assert isinstance(decisions, list)
        assert isinstance(decisions[0], dict)
        record = records[0]
        decision = decisions[0]
        if fault == "unknown":
            record["extra"] = "unexpected"
        elif fault == "candidate":
            decision["state"] = "model_reviewed"
        elif fault == "key":
            record["record_key"] = '["glossary_term","term:other"]'
        elif fault == "id":
            data = record["data"]
            assert isinstance(data, dict)
            data["concept_key"] = "other"
        elif fault == "members":
            decision["members"] = []
        elif fault == "checked":
            decision["sample_ids"] = []
        elif fault == "human":
            decision["reviewed_by"] = ""
        elif fault == "sample":
            decision["state"] = "sampled"
            decision["sample_ids"] = []
        elif fault == "format":
            shard["translation_authored_format"] = True
        if fault in {"key", "id", "unknown"}:
            shards[name] = envelope([record])
        write(root, shards)
    messages = {
        "missing": "^Translation indexed file closure differs from disk$",
        "hash": "^Translation shard hash mismatch$",
        "gap": "^Translation shard sequence gap$",
        "symlink": "^Symlink translation input$",
        "unknown": r"^Invalid translation authored fields at records\.0\.glossary_term\.extra$",
        "candidate": r"^Invalid translation authored fields at decisions\.0\.state$",
        "key": "^Translation record kind/key/filing mismatch$",
        "id": "^Permanent term ID differs from concept key$",
        "members": "^Translation decision exact membership mismatch$",
        "checked": "^Translation decision requires actual checked members$",
        "human": r"^Invalid translation authored fields at decisions\.0\.reviewed_by$",
        "sample": "^Translation decision requires actual checked members$",
        "format": r"^Invalid translation authored fields at translation_authored_format$",
    }
    with pytest.raises(ValueError, match=messages[fault]):
        load_glossary(root)


@pytest.mark.parametrize(
    "fault", ["fork", "gap", "wrong_hash", "wrong_key", "wrong_decision"]
)
def test_choice_history_cannot_reuse_a_decision(
    glossary: tuple[Path, dict[str, dict[str, JsonValue]]], fault: str
) -> None:
    root, shards = glossary
    old = choice()
    first = shards["translations/glossary/choices/001.yaml"]
    decision = first["default_decision_id"]
    previous = {
        "record_key": old["record_key"],
        "record_hash": digest(canonical(old)),
        "decision_id": decision,
    }
    second = choice(number=2, predecessor=previous)
    if fault == "fork":
        second = copy.deepcopy(old)
    elif fault == "gap":
        second = choice(number=3, predecessor=previous)
    else:
        previous[
            {
                "wrong_hash": "record_hash",
                "wrong_key": "record_key",
                "wrong_decision": "decision_id",
            }[fault]
        ] = "sha256:" + "0" * 64 if fault == "wrong_hash" else "other"
    shards["translations/glossary/choices/002.yaml"] = envelope([second])
    write(root, shards)
    with pytest.raises(
        ValueError,
        match={
            "fork": "^Duplicate immutable translation record$",
            "gap": "^Translation adoption sequence gap or fork$",
            "wrong_hash": "^Translation predecessor mismatch$",
            "wrong_key": "^Translation predecessor mismatch$",
            "wrong_decision": "^Translation predecessor mismatch$",
        }[fault],
    ):
        load_glossary(root)


def test_explicit_withdrawal_preserves_old_evidence(
    glossary: tuple[Path, dict[str, dict[str, JsonValue]]],
) -> None:
    root, shards = glossary
    previous = {
        "record_key": choice()["record_key"],
        "record_hash": digest(canonical(choice())),
        "decision_id": shards["translations/glossary/choices/001.yaml"][
            "default_decision_id"
        ],
    }
    shards["translations/glossary/choices/002.yaml"] = envelope(
        [choice(number=2, predecessor=previous, value=None)]
    )
    write(root, shards)
    loaded = load_glossary(root)
    assert len(loaded.records()) == 3
    assert [
        r.data.value for r, _ in loaded.effective() if r.kind == "glossary_choice"
    ] == [None]


def test_svwb_priority_and_all_faces(digital_template: DatabaseTemplate) -> None:
    with digital_template.copy() as db:
        assert select_name(db, card_id="card", face_id="front", lang="zh-Hant") == (
            "svwb:normal zh-Hant",
            "official_svwb",
            "decision",
        )
        assert select_name(db, card_id="card", face_id="back", lang="zh-Hant") == (
            "svwb:evolved zh-Hant",
            "official_svwb",
            "decision",
        )
        assert select_name(db, card_id="card", face_id="front", lang="en") is None
        assert (
            select_name(
                db, card_id="card", face_id="front", lang="zh-Hant", field="effect"
            )
            is None
        )


@pytest.mark.parametrize("relation", ["same_character", "name_only"])
def test_relation_is_not_concept_adoption(
    digital_template: DatabaseTemplate, relation: str
) -> None:
    with digital_template.copy() as db, db.transaction():
        for row in db.rows("digital_link"):
            db.update("digital_link", {"id": row.values["id"]}, {"relation": relation})
        assert select_name(db, card_id="card", face_id="front", lang="zh-Hant") is None


@pytest.mark.parametrize("state", ["proposed", "model_reviewed"])
def test_high_confidence_does_not_promote_review(
    digital_template: DatabaseTemplate, state: str
) -> None:
    with digital_template.copy() as db:
        with (  # ruff: ignore[pytest-raises-with-multiple-statements] -- assert selection independently before commit rejects the link
            pytest.raises(sqlite3.IntegrityError, match="digital_link_adopted"),
            db.transaction(),
        ):
            db.update(
                "decision",
                {"id": "decision"},
                {
                    "state": state,
                    "confidence": "high",
                    "reviewed_by": None,
                    "reviewed_at": None,
                },
            )
            assert (
                select_name(db, card_id="card", face_id="front", lang="zh-Hant") is None
            )


def test_fallback_to_sv1_and_missing_face_does_not_guess(
    digital_template: DatabaseTemplate,
) -> None:
    with digital_template.copy() as db, db.transaction():
        db.delete("digital_link", {"id": "svwb:normal"})
        selected = select_name(db, card_id="card", face_id="front", lang="zh-Hant")
        assert selected is not None
        assert selected[1] == "official_sv1"
        db.update("digital_link", {"id": "sv1:normal"}, {"face_id": None})
        assert select_name(db, card_id="card", face_id="front", lang="zh-Hant") is None


def test_unlocated_digital_face_is_missing_translation(
    digital_template: DatabaseTemplate,
) -> None:
    with digital_template.copy() as db, db.transaction():
        db.delete("digital_link", {"id": "svwb:normal"})
        db.update("digital_link", {"id": "sv1:normal"}, {"digital_face_id": None})
        assert select_name(db, card_id="card", face_id="front", lang="zh-Hant") is None


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


@pytest.mark.parametrize(
    "table", ["glossary_term", "glossary_translation", "digital_link_coverage"]
)
def test_database_rejects_unadopted_translation_evidence(
    digital_template: DatabaseTemplate, table: str
) -> None:
    with digital_template.copy() as db:
        with db.transaction():
            db.insert(
                "decision",
                {
                    "id": "unadopted",
                    "state": "proposed",
                    "scope": "record",
                    "category": "synthetic",
                    "authored_by": "Synthetic tool",
                    "authored_at": "2026-10-02T00:00:00Z",
                    "note": "Unreviewed.",
                },
            )
            if table == "glossary_translation":
                db.insert(
                    "glossary_term",
                    {
                        "id": "term:rule.test",
                        "concept_key": "rule.test",
                        "source_ja": "Synthetic",
                        "category": "rule_term",
                        "decision_id": "decision",
                    },
                )
        if table == "glossary_term":
            values = {
                "id": "term:rule.test",
                "concept_key": "rule.test",
                "source_ja": "Synthetic",
                "category": "rule_term",
            }
        elif table == "glossary_translation":
            values = {
                "term_id": "term:rule.test",
                "lang": "zh-Hant",
                "text": "Synthetic",
                "origin": "project",
            }
        else:
            values = {
                "card_id": "card",
                "game": "svwb",
                "state": "partial",
                "as_of": "2026-10-02",
            }
        with (
            pytest.raises(
                sqlite3.IntegrityError,
                match={
                    "glossary_term": "glossary_term_adopted",
                    "glossary_translation": "glossary_translation_adopted",
                    "digital_link_coverage": "digital_coverage_adopted",
                }[table],
            ),
            db.transaction(),
        ):
            db.insert(table, {**values, "decision_id": "unadopted"})


@pytest.mark.parametrize("mode", ["absent", "file_link", "parent_link"])
def test_index_must_be_regular_input(tmp_path: Path, mode: str) -> None:
    root = tmp_path / "authored"
    if mode != "absent":
        target = tmp_path / "target"
        write(target, {})
        root.mkdir()
        if mode == "file_link":
            (root / "translations").mkdir()
            (root / "translations/index.yaml").symlink_to(
                target / "translations/index.yaml"
            )
        else:
            (root / "translations").symlink_to(
                target / "translations", target_is_directory=True
            )
    with pytest.raises(ValueError, match=r"^Missing or symlink translation input$"):
        load_glossary(root)


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("inventory", "Template inventories require the #52 loader"),
        ("include", "Unsafe or unsupported translation include"),
        ("unsorted", "Translation members must be sorted and unique"),
        ("duplicate", "Translation members must be sorted and unique"),
        ("partial_confirmed", "Confirmed translation must check every member"),
        ("first_predecessor", "Initial choice must have no predecessor"),
        ("unadopted_term", "Glossary choice references an unadopted concept"),
        ("concept_reuse", "Duplicate immutable translation record"),
    ],
)
def test_glossary_inventory_counterexamples(
    tmp_path: Path, fault: str, message: str
) -> None:
    definitions = [term(), term("rule.second")]
    selected = choice()
    if fault == "first_predecessor":
        object_value(selected["data"])["predecessor"] = {
            "record_key": "prior",
            "record_hash": digest(b"prior"),
            "decision_id": "prior",
        }
    if fault == "unadopted_term":
        selected = choice("rule.absent")
    concept_shard = envelope(definitions)
    if fault == "unsorted":
        concept_shard["records"] = list(reversed(array(concept_shard["records"])))
    elif fault == "duplicate":
        concept_shard["records"] = [definitions[0], definitions[0]]
    elif fault == "partial_confirmed":
        object_value(array(concept_shard["decisions"])[0])["sample_ids"] = [
            definitions[0]["record_key"]
        ]
    shards = {
        "translations/glossary/concepts/001.yaml": concept_shard,
        "translations/glossary/choices/001.yaml": envelope([selected]),
    }
    if fault == "concept_reuse":
        shards["translations/glossary/concepts/002.yaml"] = envelope([term()])
    if fault == "include":
        shards["translations/unsupported.yaml"] = shards.pop(
            "translations/glossary/concepts/001.yaml"
        )
    write(tmp_path, shards)
    if fault == "inventory":
        path = tmp_path / "translations/index.yaml"
        index = object_value(parse(path.read_bytes()))
        index["inventories"] = {"templates": digest(b"synthetic")}
        path.write_bytes(canonical(index))
    with pytest.raises(ValueError, match="^" + message + "$"):
        load_glossary(tmp_path)


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("authored_by", " ", "Adopted glossary requires author and human reviewer"),
        ("reviewed_by", "\t", "Adopted glossary requires author and human reviewer"),
        (
            "reviewed_at",
            "2026-10-02T12:00:00Z",
            "Day precision must use UTC midnight encoding",
        ),
    ],
)
def test_actual_review_receipt_fields(field: str, value: str, message: str) -> None:
    data = object_value(array(envelope([term()])["decisions"])[0])
    data[field] = value
    with pytest.raises(ValueError, match=message):
        Decision.model_validate_json(canonical(data))


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


@pytest.mark.parametrize(
    ("fault", "message"),
    [
        ("owner", "Name owner/card mismatch"),
        ("parent", "Digital link parent mismatch"),
        ("language", "Digital name language mismatch"),
        ("ambiguous", "Ambiguous adopted digital names"),
    ],
)
def test_selection_rejects_inconsistent_owner_evidence(
    digital_template: DatabaseTemplate, fault: str, message: str
) -> None:
    with digital_template.copy() as db:
        with (  # ruff: ignore[pytest-raises-with-multiple-statements] -- inspect corrupt graph before commit-time constraints and roll back on refusal
            pytest.raises(ValueError, match="^" + message + "$"),
            db.transaction(),
        ):
            if fault == "parent":
                db.update(
                    "digital_link",
                    {"id": "svwb:normal"},
                    {"digital_face_id": "sv1:normal"},
                )
            elif fault == "language":
                db.update(
                    "digital_text",
                    {"digital_face_id": "svwb:normal", "lang": "zh-Hant"},
                    {"name_unit_id": "svwb:normal:ja"},
                )
            elif fault == "ambiguous":
                db.update("digital_link", {"id": "svwb:evolved"}, {"face_id": "front"})
            select_name(
                db,
                card_id="other" if fault == "owner" else "card",
                face_id="front",
                lang="zh-Hant",
            )

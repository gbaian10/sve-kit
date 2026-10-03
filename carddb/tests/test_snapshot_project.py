"""Public projection acceptance cases use synthetic DB inputs and independent oracles."""

import sqlite3
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError
from pydantic import JsonValue, RootModel

import sve_carddb.snapshot.project as project_module
from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.database import open_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.routes.defaults import GeneralEvidence, select_defaults
from sve_carddb.snapshot.contract import tables
from sve_carddb.snapshot.project import (
    Decisions,
    DisplayBinding,
    DisplayText,
    display_text,
    effective_support,
    project,
)
from sve_carddb.snapshot.project.closure import validate_closure
from sve_carddb.snapshot.project.records import initial
from sve_carddb.snapshot.project.regions import Dates, region_views
from sve_carddb.snapshot.project.shape import tuple_value
from sve_carddb.snapshot.project.source import Source
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    object_value,
    parse,
    string,
)

from .snapshot_project_fixtures import SETTINGS, TEXT, decisions, populate, schema

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import CompiledSchema, Database, Value
    from sve_carddb.snapshot.project import Projection
    from sve_carddb.snapshot.project.source import Record


@pytest.fixture(scope="module")
def projection_schema() -> CompiledSchema:
    return schema()


@pytest.fixture
def db(projection_schema: CompiledSchema) -> Iterator[Database]:
    with create_database(projection_schema) as database:
        with database.transaction():
            populate(database)
        yield database


def projected(db: Database, chosen: Decisions | None = None) -> Projection:
    return project(
        db,
        regions=("jp",),
        as_of="2026-10-01",
        settings=SETTINGS,
        decisions=decisions() if chosen is None else chosen,
    )


def one(result: Projection, table: str, identifier: str | None = None) -> Record:
    rows = result.tables[table]
    if identifier is None:
        assert len(rows) == 1
        return rows[0]
    return next(row for row in rows if row.get("id", row.get("card_id")) == identifier)


def test_all_collections_and_every_public_field_have_synthetic_rows(
    db: Database,
) -> None:
    result = projected(db)
    assert set(result.tables) == set(tables())
    assert len(result.tables) == 43
    assert all(result.tables.values())
    assert one(result, "digital_art") == {
        "id": "digital-art",
        "digital_card_id": "digital",
        "phase": "normal",
        "style_key": "standard",
    }
    assert one(result, "digital_link")["digital_phase"] == "normal"
    assert one(result, "art")["artists"] == [
        {"artist_id": "artist", "role": "illustration"}
    ]
    assert one(result, "printing")["int_id"] == 20001
    assert one(result, "printing")["card_no"] == "TEST-001Ⓢa"
    assert one(result, "printing")["premium"] is None
    assert one(result, "image_asset")["format"] == "png"
    assert one(result, "rules_profile")["revisions"] == [
        {
            "id": "profile_revision",
            "effective_from": "2026-09-29",
            "effective_until": None,
            "cr_version_id": None,
            "default_copy_limit": None,
            "construction_rules_ref": None,
        }
    ]
    assert one(result, "restriction")["members"] == [
        {"rules_name_id": "rules_name", "choice_option": 0, "deck_scope": "all"}
    ]
    assert one(result, "qa_version")["cards"] == ["card"]
    assert one(result, "errata")["versions"] == [
        {
            "id": "errata_v",
            "revision": 0,
            "announced_on": None,
            "effective_on": None,
            "date_raw": None,
            "reason_unit_id": None,
            "exchange_offered": None,
            "changes": [
                {
                    "face_id": "face",
                    "field": "effect",
                    "before": "Before",
                    "after": "Synthetic text",
                }
            ],
            "printings": [{"printing_id": "printing", "scope": "confirmed_applies"}],
        }
    ]
    assert one(result, "ruling_revision", "ruling-rev")["evidence"] == [
        {
            "qa_version_id": "qa_v",
            "cr_clause_id": None,
            "source_url": None,
            "role": "supporting",
            "quote": "Synthetic text",
            "locator": "answer",
        }
    ]
    assert result.metadata["qa_card_ids"] == ["card"]
    assert result.metadata["errata_card_ids"] == ["card"]


def test_build_audit_columns_and_unused_text_never_ship(db: Database) -> None:
    with db.transaction():
        db.insert(
            "text_unit",
            {
                "id": "t:ja:" + "b" * 16,
                "lang": "ja",
                "text": "PRIVATE UNUSED TEXT",
                "content_hash": "sha256:" + "b" * 64,
            },
        )
    result = projected(db)
    content = canonical(
        {
            "tables": {table: list(rows) for table, rows in result.tables.items()},
            "config": result.config,
            "metadata": result.metadata,
        }
    )
    for secret in (
        b"PRIVATE",
        b"authored_by",
        b"raw_locator",
        b"content_hash",
        b"source_id",
        b"translated_by",
        b"reviewed_at",
        b"candidate_hash",
        b"recipe_version",
    ):
        assert secret not in content
    assert len(result.tables["text_unit"]) == 3


def test_support_exactly_once_including_tombstone_and_always_manual(
    db: Database,
) -> None:
    result = projected(db)
    assert [row["card_id"] for row in result.tables["card_engine_support"]] == [
        "card",
        "old_card",
    ]
    for row in result.tables["card_engine_support"]:
        assert object_value(row["shared"]) == {
            "status": "missing_dsl",
            "dsl_status": None,
            "dsl_version": None,
            "dsl_id": None,
            "validation_state": "not_applicable",
            "reasons": ["missing_dsl"],
            "reason_detail": None,
            "program_ref": None,
            "ruling_revision_ids": [],
        }
        for region in ("jp", "en"):
            assert effective_support(row, region)["automatic"] is False
    en = effective_support(one(result, "card_engine_support", "card"), "en")
    assert set(map(string, array(en["reasons"]))) == {
        "missing_dsl",
        "missing_region_source",
        "region_text_unreviewed",
        "mapping_unconfirmed",
    }
    assert "retired" in array(
        effective_support(one(result, "card_engine_support", "old_card"), "jp")[
            "reasons"
        ]
    )


def test_missing_capabilities_have_empty_arrays_nulls_and_unknown_not_absent() -> None:
    with create_database(compile_t0()) as db:
        with db.transaction():
            populate(db, ancillary=False, future=False)
        result = projected(db, Decisions())
    for table in (
        "art",
        "qa",
        "qa_version",
        "errata",
        "cr_version",
        "image_asset",
        "image_variant",
        "translation",
        "keyword",
        "card_mechanic_coverage",
        "mechanic_projection",
    ):
        assert result.tables[table] == []
    assert result.metadata["qa_card_ids"] == []
    assert result.metadata["errata_card_ids"] == []
    assert result.metadata["coverage"] == {
        "reviews": [],
        "translations": [],
        "mechanics": [
            {
                "region": "jp",
                "scope": "shared",
                "total_cards": 1,
                "any_annotated_cards": 0,
                "fully_annotated_cards": 0,
                "unknown_cards": 1,
                "eligibility": "all_non_retired_cards_in_region",
                "by_keyword": [],
            }
        ],
    }
    assert one(result, "printing")["decklog_source_url"] is None
    assert (
        object_value(array(one(result, "card", "card")["regions"])[0])["release_state"]
        == "unknown"
    )
    assert one(result, "face")["wording"] == []
    assert len(array(result.config["image_sizes"])) == 5


def test_selected_translation_is_owner_bound_and_missing_language_keeps_source(
    db: Database,
) -> None:
    result = projected(db)
    revision = one(result, "face_revision")
    assert revision["translations"] == [
        {
            "field": "name",
            "ordinal": None,
            "target_lang": "zh-Hant",
            "translation_id": "translation",
            "basis": "own_source",
        }
    ]
    assert revision["name_unit_id"] == TEXT
    assert revision["effect_unit_id"] == TEXT
    assert one(result, "translation")["source_unit_id"] == TEXT
    with db.transaction():
        db.update("translation", {"id": "translation"}, {"status": "stale"})
    result = projected(db)
    assert result.tables["translation"] == []
    assert one(result, "face_revision")["translations"] == []


def test_translation_context_mismatch_rejected(db: Database) -> None:
    with db.transaction():
        db.update(
            "translation_context",
            {"id": "context"},
            {"source_unit_id": "t:ja:" + "b" * 16},
        )
    with pytest.raises(ValueError, match="owner/context"):
        projected(db)


def test_corrections_do_not_mark_shared_text_or_other_printing(db: Database) -> None:
    with db.transaction():
        raw = dict(db.rows("printing")[0].values)
        db.insert("printing", raw | {"id": "other", "card_no": "TEST-002"})
        db.insert(
            "card_int_id",
            {"printing_id": "other", "int_id": 20002, "allocated_at": "2026-09-29"},
        )
        db.insert(
            "printing_face",
            dict(db.rows("printing_face")[0].values) | {"printing_id": "other"},
        )
    result = projected(db)
    assert object_value(array(one(result, "printing", "printing")["faces"])[0])[
        "corrections"
    ] == [
        {
            "field": "effect",
            "corrected_from": "Before",
            "is_corrected": True,
            "reason": "Synthetic correction",
            "source_url": "https://example.invalid/card",
        }
    ]
    assert (
        object_value(array(one(result, "printing", "other")["faces"])[0])["corrections"]
        == []
    )
    assert all("corrections" not in row for row in result.tables["text_unit"])
    with db.transaction():
        db.update(
            "correction_application",
            {"correction_id": "correction", "source_id": "source"},
            {"status": "already_fixed"},
        )
    assert one(projected(db), "face_revision")["corrections"] == []


@pytest.mark.parametrize(
    ("precision", "raw"), [("month", "2026-09"), ("year", "2026"), ("unknown", None)]
)
def test_dates_do_not_invent_day_precision(
    db: Database, precision: str, raw: str | None
) -> None:
    with db.transaction():
        db.update(
            "product", {"id": "product"}, {"date_precision": precision, "date_raw": raw}
        )
    result = projected(db)
    assert one(result, "product")["released_on"] is None
    assert one(result, "product")["date_raw"] == raw
    inclusion = one(result, "printing_product")
    assert inclusion["available_on"] is None
    assert inclusion["date_precision"] is None
    assert inclusion["first_inclusion_state"] == "unknown"
    assert all(
        object_value(raw)["debut_state"] == "unknown"
        for raw in array(one(result, "card", "card")["regions"])
    )


def test_unknown_override_beats_known_product_date(db: Database) -> None:
    with db.transaction():
        db.update(
            "product",
            {"id": "product"},
            {"date_precision": "day", "released_on": "2026-09-01"},
        )
    result = projected(db)
    assert one(result, "printing_product")["first_inclusion_state"] == "first"
    assert object_value(array(one(result, "card", "card")["regions"])[1])[
        "debut_product_ids"
    ] == ["product"]
    with db.transaction():
        db.update(
            "printing_product",
            {"printing_id": "printing", "product_id": "product"},
            {"first_available_precision": "unknown"},
        )
    result = projected(db)
    assert one(result, "printing_product")["first_inclusion_state"] == "unknown"
    assert one(result, "printing_product")["date_precision"] == "unknown"


def test_historical_art_and_artist_are_pruned_with_tombstone(db: Database) -> None:
    with db.transaction():
        db.insert("art", dict(db.rows("art")[0].values) | {"id": "old-art"})
        db.insert(
            "artist",
            {
                "id": "old-artist",
                "display_name": "Private history",
                "decision_id": "decision",
            },
        )
        db.insert(
            "art_artist",
            {
                "art_id": "old-art",
                "artist_id": "old-artist",
                "role": "illustration",
                "source_id": "source",
            },
        )
    result = projected(db)
    assert [row["id"] for row in result.tables["art"]] == ["art"]
    assert [row["id"] for row in result.tables["artist"]] == ["artist"]
    assert one(result, "art")["regions"] == ["jp"]


@pytest.mark.parametrize("publication_state", ["pending", "withdrawn"])
def test_unpublished_image_cannot_expose_variants(
    db: Database, publication_state: str
) -> None:
    with db.transaction():
        db.update(
            "image_asset",
            {"id": "image"},
            {
                "publication_state": publication_state,
                "withdrawal_reason": "Synthetic withdrawal"
                if publication_state == "withdrawn"
                else None,
            },
        )
        db.delete(
            "image_variant",
            {"image_id": "image", "size_key": "card_s", "format": "webp"},
        )
    result = projected(db)
    assert (
        one(result, "image_asset")["source_url"] == "https://example.invalid/source.png"
    )
    assert result.tables["image_variant"] == []


def pending() -> Record:
    return {
        "region": "jp",
        "state": "pending",
        "display": {"revision_id": None, "basis": "candidates"},
        "candidates": [{"printing_id": "printing", "revision_id": "revision"}],
        "undated_printing_ids": ["printing"],
    }


def test_pending_display_is_not_current_and_adds_manual_block(db: Database) -> None:
    with db.transaction():
        db.delete("face_current", {"face_id": "face", "region": "jp"})
    result = projected(db, replace(decisions(), wording={"face": (pending(),)}))
    assert one(result, "face")["current"] == []
    assert one(result, "face")["wording"] == [pending()]
    assert "wording_pending" in array(
        effective_support(one(result, "card_engine_support", "card"), "jp")["reasons"]
    )
    assert one(result, "printing")["decklog_available"] is True
    assert one(result, "printing")["int_id"] == 20001
    assert result.tables["card_route_alias"]


def test_pending_old_current_preserved_without_blanket_pending_block(
    db: Database,
) -> None:
    wording = pending() | {"display": {"revision_id": "revision", "basis": "current"}}
    result = projected(db, replace(decisions(), wording={"face": (wording,)}))
    assert one(result, "face")["current"] == [
        {
            "region": "jp",
            "revision_id": "revision",
            "basis": "latest_observed_no_errata",
        }
    ]
    assert "wording_pending" not in array(
        effective_support(one(result, "card_engine_support", "card"), "jp")["reasons"]
    )


@pytest.mark.parametrize(
    "change",
    [
        {"state": "settled"},
        {"raw_locator": "private"},
        {"undated_printing_ids": ["missing"]},
        {"candidates": [{"printing_id": "printing", "revision_id": "missing"}]},
        {"display": {"revision_id": "revision", "basis": "current"}},
        {"display": {"revision_id": None, "basis": "latest_known_release"}},
    ],
)
def test_invalid_wording_adapter_rejected(db: Database, change: Record) -> None:
    with db.transaction():
        db.delete("face_current", {"face_id": "face", "region": "jp"})
    with pytest.raises((ValueError, KeyError)):
        projected(db, replace(decisions(), wording={"face": (pending() | change,)}))


def test_public_reference_and_extra_nested_fields_rejected(db: Database) -> None:
    result = projected(db)
    one(result, "face_revision")["name_unit_id"] = "missing"
    with pytest.raises(ValueError, match="Dangling"):
        validate_closure(result.tables)
    row = one(result, "printing")
    row["raw_locator"] = "private"
    with pytest.raises(ValueError, match="whitelist"):
        tuple_value("printing", row)


def test_effective_support_applies_override_then_blocks(db: Database) -> None:
    support = one(projected(db), "card_engine_support", "card")
    passed = object_value(support["shared"]) | {
        "status": "engine_passed",
        "reasons": [],
    }
    support["shared"] = passed
    support["region_blocks"] = [{"region": "en", "reasons": ["mapping_unconfirmed"]}]
    assert effective_support(support, "jp")["automatic"] is True
    assert effective_support(support, "en")["effective_status"] == "reviewed"
    assert effective_support(support, "en")["automatic"] is False
    support["overrides"] = [
        {
            "region": "en",
            "support": passed | {"status": "draft", "reasons": ["invalid_candidate"]},
        }
    ]
    assert effective_support(support, "en")["effective_status"] == "draft"
    assert effective_support(support, "en")["reasons"] == [
        "invalid_candidate",
        "mapping_unconfirmed",
    ]


def test_repeated_projection_is_stable_and_does_not_mutate_database(
    db: Database,
) -> None:
    before = {
        table: db.rows(table)
        for table in ("face", "printing", "text_unit", "source_record")
    }
    first, second = projected(db), projected(db)
    assert first == second
    first.config["languages"] = []
    assert projected(db) == second
    assert {table: db.rows(table) for table in before} == before


def test_search_normalizer_and_source_window_scope_are_checked(db: Database) -> None:
    with db.transaction():
        db.update(
            "search_alias",
            {"kind": "card", "code": "card", "lang": "ja", "text": "Synthetic alias"},
            {"normalizer_version": "different-v1"},
        )
    with pytest.raises(ValueError, match="normalizer"):
        projected(db)
    with db.transaction():
        db.update(
            "search_alias",
            {"kind": "card", "code": "card", "lang": "ja", "text": "Synthetic alias"},
            {"normalizer_version": "synthetic-v1"},
        )
        db.update(
            "source_coverage",
            {
                "kind": "cardlist",
                "region": "jp",
                "scope_key": "region:*",
                "from_date": "2026-09-29",
                "as_of": "2026-09-29",
            },
            {"scope_key": "private:something"},
        )
    with pytest.raises(ValueError, match="scope is not public"):
        projected(db)


def test_scalar_nullability_is_schema_authoritative(db: Database) -> None:
    with db.transaction():
        db.update("product", {"id": "product"}, {"product_type": None})
    assert one(projected(db), "product")["product_type"] is None
    record: Record = {"id": "synthetic", "lang": "ja", "text": None}
    with pytest.raises(ValidationError):
        tuple_value("text_unit", record)


def test_database_explicit_select_validates_whitelist_and_storage(db: Database) -> None:
    assert dict(db.select("source_record", ("id", "url"))[0].values) == {
        "id": "source",
        "url": "https://example.invalid/card",
    }
    for selected in ((), ("id", "id")):
        with pytest.raises(ValueError, match="unique"):
            db.select("source_record", selected)
    with pytest.raises(ValueError, match="Unknown column"):
        db.select("source_record", ("id; DROP TABLE card",))


def test_display_fallback_uses_exact_owner_source_and_no_unrelated_language(
    db: Database,
) -> None:
    result = projected(db)
    revision = one(result, "face_revision")
    assert display_text(result.tables, revision, "name", "zh-Hant") == DisplayText(
        "Synthetic translation", "zh-Hant", "translation", False
    )
    assert display_text(result.tables, revision, "effect", "zh-Hant") == DisplayText(
        "Synthetic text", "ja", None, True
    )
    assert display_text(result.tables, revision, "name", "en") == DisplayText(
        "Synthetic text", "ja", None, True
    )
    assert display_text(result.tables, revision, "name", "ja") == DisplayText(
        "Synthetic text", "ja", None, False
    )
    face = object_value(array(one(result, "printing")["faces"])[0])
    assert display_text(result.tables, face, "effect", "en") == DisplayText(
        None, None, None, True
    )


def test_current_of_filtered_region_does_not_leave_dangling_revision(
    db: Database,
) -> None:
    with db.transaction():
        db.insert(
            "face_revision",
            dict(db.rows("face_revision")[0].values)
            | {"id": "revision-en", "region": "en"},
        )
        db.insert(
            "face_current",
            {
                "face_id": "face",
                "region": "en",
                "revision_id": "revision-en",
                "basis": "latest_observed_no_errata",
            },
        )
    result = projected(db)
    assert one(result, "face")["current"] == [
        {
            "region": "jp",
            "revision_id": "revision",
            "basis": "latest_observed_no_errata",
        }
    ]
    assert [row["id"] for row in result.tables["face_revision"]] == ["revision"]


def test_unresolved_regional_rules_divergence_keeps_manual_reason(db: Database) -> None:
    with db.transaction():
        db.update(
            "region_divergence",
            {"card_id": "card", "region": "en", "field_scope": "name"},
            {"field_scope": "rules", "resolved": False},
        )
    result = projected(
        db, replace(decisions(), aligned_regions=frozenset({("card", "en")}))
    )
    support = effective_support(one(result, "card_engine_support", "card"), "en")
    assert "region_divergence" in array(support["reasons"])
    assert support["automatic"] is False


def test_default_general_evidence_and_date_uncertainty_are_separate(
    db: Database,
) -> None:
    with db.transaction():
        db.delete("default_printing_override", {"card_id": "card", "region": "jp"})
    chosen = replace(
        decisions(), general_evidence={"printing": GeneralEvidence(True, True, False)}
    )
    result = projected(db, chosen)
    assert (
        object_value(array(one(result, "card", "card")["regions"])[1])["default_method"]
        == "candidate_general"
    )
    with db.transaction():
        db.update(
            "product",
            {"id": "product"},
            {"date_precision": "day", "released_on": "2026-09-29"},
        )
        db.update("printing", {"id": "printing"}, {"premium": False})
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"signed": False, "embellishment_state": "confirmed"},
        )
    result = projected(db, chosen)
    assert (
        object_value(array(one(result, "card", "card")["regions"])[1])["default_method"]
        == "earliest_general"
    )


def test_two_contexts_for_same_source_can_select_different_field_translations(
    db: Database,
) -> None:
    with db.transaction():
        db.insert(
            "translation_context",
            {
                "id": "context2",
                "source_unit_id": TEXT,
                "semantic_variant": "second",
                "decision_id": "decision",
            },
        )
        db.insert(
            "translation",
            dict(db.rows("translation")[0].values)
            | {
                "id": "translation2",
                "context_id": "context2",
                "text": "Other synthetic translation",
            },
        )
        db.insert(
            "translation_selection",
            {
                "context_id": "context2",
                "target_lang": "zh-Hant",
                "translation_id": "translation2",
            },
        )
        db.insert(
            "translation_use",
            dict(db.rows("translation_use")[0].values)
            | {"id": "use2", "context_id": "context2", "field": "effect"},
        )
    result = projected(db)
    revision = one(result, "face_revision")
    assert (
        display_text(result.tables, revision, "name", "zh-Hant").text
        == "Synthetic translation"
    )
    assert (
        display_text(result.tables, revision, "effect", "zh-Hant").text
        == "Other synthetic translation"
    )
    assert {row["source_unit_id"] for row in result.tables["translation"]} == {TEXT}


@pytest.mark.parametrize(
    ("owner", "field", "ordinal"),
    [
        ({"face_revision_id": "revision"}, "section", 0),
        ({"face_revision_id": None, "keyword_id": "keyword"}, "action_label", 0),
        ({"face_revision_id": None, "keyword_id": "keyword"}, "label", None),
        (
            {
                "face_revision_id": None,
                "vocabulary_kind": "type",
                "vocabulary_code": "follower",
            },
            "label",
            None,
        ),
        ({"face_revision_id": None, "product_family_id": "family"}, "name", None),
        ({"face_revision_id": None, "product_id": "product"}, "name", None),
        ({"face_revision_id": None, "qa_version_id": "qa_v"}, "answer", None),
    ],
)
def test_translation_owner_and_ordinal_are_exact(
    db: Database, owner: dict[str, Value], field: str, ordinal: int | None
) -> None:
    with db.transaction():
        db.update(
            "translation_use",
            {"id": "use"},
            owner | {"field": field, "ordinal": ordinal},
        )
        if field == "label" and owner.get("keyword_id") == "keyword":
            keyword_text = "t:ja:" + digest(b"Synthetic keyword")[7:23]
            db.insert(
                "text_unit",
                {
                    "id": keyword_text,
                    "lang": "ja",
                    "text": "Synthetic keyword",
                    "content_hash": digest(b"Synthetic keyword"),
                },
            )
            db.update(
                "translation_context",
                {"id": "context"},
                {"source_unit_id": keyword_text},
            )
    result = projected(db)
    assert len(result.tables["translation"]) == 1
    bindings = [
        object_value(raw)
        for table in result.tables.values()
        for row in table
        for raw in array(row.get("translations", []))
    ]
    assert len(bindings) == 1
    assert bindings[0]["field"] == field
    assert bindings[0]["ordinal"] == ordinal


def test_cross_region_translation_does_not_use_pending_display_as_current(
    db: Database,
) -> None:
    with db.transaction():
        db.delete("face_current", {"face_id": "face", "region": "jp"})
    chosen = replace(
        decisions(),
        wording={"face": (pending(),)},
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision"),
                "en",
                "official_counterpart",
                "translation",
            ),
        ),
        aligned_regions=frozenset({("card", "en")}),
    )
    result = projected(db, chosen)
    assert len(result.tables["translation"]) == 1
    assert (
        object_value(array(one(result, "face_revision")["translations"])[0])["basis"]
        == "own_source"
    )


def test_selection_and_owner_ambiguity_are_rejected(db: Database) -> None:
    with db.transaction():
        db.update(
            "translation_selection",
            {"context_id": "context", "target_lang": "zh-Hant"},
            {"target_lang": "en"},
        )
    with pytest.raises(ValueError, match=r"^Translation selection context mismatch$"):
        projected(db)
    with db.transaction():
        db.update(
            "translation_selection",
            {"context_id": "context", "target_lang": "en"},
            {"target_lang": "zh-Hant"},
        )
    with (
        pytest.raises(sqlite3.IntegrityError, match=r"^CHECK constraint failed: "),
        db.transaction(),
    ):
        db.update("translation_use", {"id": "use"}, {"product_id": "product"})


@pytest.mark.parametrize(
    "change",
    [
        {"complete_all": True},
        {"complete_mode": "include", "complete_keyword_ids": Json(["keyword"])},
        {"complete_keyword_ids": Json(["missing"])},
    ],
)
def test_invalid_coverage_cannot_claim_absence(
    db: Database, change: dict[str, Value]
) -> None:
    with db.transaction():
        db.update("card_mechanic_coverage", {"card_id": "card"}, change)
    with pytest.raises(ValueError, match=r"coverage|reference"):
        projected(db)


def test_complete_coverage_counts_and_complement_encoding(db: Database) -> None:
    with db.transaction():
        db.update(
            "card_mechanic_coverage",
            {"card_id": "card"},
            {"complete_all": True, "partial_mode": "include"},
        )
    result = projected(db)
    coverage = object_value(
        array(object_value(result.metadata["coverage"])["mechanics"])[0]
    )
    assert coverage["fully_annotated_cards"] == 1
    assert coverage["by_keyword"] == [{"keyword_id": "keyword", "complete_cards": 1}]
    with db.transaction():
        db.update(
            "card_mechanic_coverage",
            {"card_id": "card"},
            {"complete_all": False, "complete_mode": "exclude"},
        )
    assert projected(db).metadata["coverage"] == result.metadata["coverage"]


def test_ruling_requires_explicit_active_scopes_and_undecided_has_no_hint(
    db: Database,
) -> None:
    with pytest.raises(ValueError, match="active scopes"):
        projected(db, Decisions())
    with db.transaction():
        db.update("ruling_revision", {"id": "ruling-rev"}, {"strength": "undecided"})
    result = projected(db)
    assert one(result, "ruling_revision", "ruling-rev")["hints"] == []
    result = projected(
        db,
        replace(decisions(), active_scopes={"ruling-rev": (), "ruling-rev2": (TEXT,)}),
    )
    assert one(result, "ruling_revision", "ruling-rev")["active_scopes"] == []


def test_missing_reskin_eligibility_does_not_imply_cross_region_rules(
    db: Database,
) -> None:
    result = projected(db, replace(decisions(), related_regions={}))
    assert result.tables["card_related"] == []


def test_invalid_image_size_config_fails_before_export(db: Database) -> None:
    with db.transaction():
        db.update("image_size", {"key": "card_s"}, {"max_width": 999})
    with pytest.raises(ValueError, match="image size"):
        projected(db)


@pytest.mark.parametrize("regions", [(), ("en", "jp", "jp"), ("cn",), ("jp", "en")])
def test_region_scope_must_be_explicit_and_canonical(
    db: Database, regions: tuple[str, ...]
) -> None:
    with pytest.raises(ValueError, match="regions required"):
        project(db, regions=regions, as_of="2026-10-01", settings=SETTINGS)


def test_observation_state_whitelist_and_missing_effect_are_not_empty_text(
    db: Database,
) -> None:
    for state in ("invalid", "missing_effect"):
        with pytest.raises(ValueError, match=r"observation|Observation"):
            projected(
                db,
                replace(
                    decisions(),
                    observation_states={("printing", "face", "source"): state},
                ),
            )


def test_empty_effect_is_public_exact_text_and_available_observation(
    db: Database,
) -> None:
    empty = "t:ja:" + digest(b"")[7:23]
    with db.transaction():
        db.insert(
            "text_unit",
            {"id": empty, "lang": "ja", "text": "", "content_hash": digest(b"")},
        )
        db.update("face_revision", {"id": "revision"}, {"effect_unit_id": empty})
    result = projected(db)
    assert one(result, "face_revision")["effect_unit_id"] == empty
    text = one(result, "text_unit", empty)["text"]
    assert isinstance(text, str)
    assert not text
    assert object_value(array(one(result, "printing")["faces"])[0])["observations"] == [
        {
            "revision_id": "revision",
            "state": "available",
            "source_url": "https://example.invalid/card",
        }
    ]


def test_synthetic_nullable_observation_keeps_missing_main_text_unknown() -> None:
    with create_database(schema(nullable_observation=True)) as db:
        with db.transaction():
            populate(db)
            db.delete("face_current", {"face_id": "face", "region": "jp"})
            db.update(
                "printing_face_observation",
                {"printing_id": "printing", "face_id": "face", "source_id": "source"},
                {"revision_id": None},
            )
        wording = pending() | {
            "display": {"revision_id": None, "basis": "candidates"},
            "candidates": [{"printing_id": "printing", "revision_id": None}],
        }
        result = projected(db, replace(decisions(), wording={"face": (wording,)}))
    observed = object_value(array(one(result, "printing")["faces"])[0])["observations"]
    assert observed == [
        {
            "revision_id": None,
            "state": "missing_effect",
            "source_url": "https://example.invalid/card",
        }
    ]
    assert one(result, "face")["current"] == []
    assert object_value(array(one(result, "face")["wording"])[0])["display"] == {
        "revision_id": None,
        "basis": "candidates",
    }


def test_unadopted_observation_requires_wording_adapter(db: Database) -> None:
    with db.transaction():
        db.delete("face_current", {"face_id": "face", "region": "jp"})
    with pytest.raises(ValueError, match="wording projection"):
        projected(db)


def test_exact_public_text_id_is_rechecked(db: Database) -> None:
    with db.transaction():
        db.update("text_unit", {"id": TEXT}, {"text": "Changed synthetic text"})
    with pytest.raises(ValueError, match="Text ID"):
        projected(db)


def test_independent_ancillary_oracle_matches_complete_rows(db: Database) -> None:
    expected = object_value(
        parse(
            (
                Path(__file__).parent
                / "fixtures/snapshot-project/expected-ancillary.json"
            ).read_bytes()
        )
    )
    result = projected(db)
    assert len(expected) == 31
    for table, rows in expected.items():
        assert result.tables[table] == rows, table


def dual_region(db: Database) -> None:
    with db.transaction():
        db.insert(
            "printing",
            dict(db.rows("printing")[0].values)
            | {
                "id": "printing-en",
                "region": "en",
                "card_no": "TEST-001EN",
            },
        )
        db.insert(
            "card_int_id",
            {
                "printing_id": "printing-en",
                "int_id": 20002,
                "allocated_at": "2026-09-29",
            },
        )
        db.insert(
            "printing_face",
            dict(db.rows("printing_face")[0].values) | {"printing_id": "printing-en"},
        )
        db.insert(
            "face_revision",
            dict(db.rows("face_revision")[0].values)
            | {"id": "revision-en", "region": "en"},
        )
        db.insert(
            "face_current",
            {
                "face_id": "face",
                "region": "en",
                "revision_id": "revision-en",
                "basis": "latest_observed_no_errata",
            },
        )
        db.update(
            "region_divergence",
            {"card_id": "card", "region": "en", "field_scope": "name"},
            {"resolved": True},
        )


def dual_project(db: Database, chosen: Decisions) -> Projection:
    return project(
        db,
        regions=("en", "jp"),
        as_of="2026-10-01",
        settings=SETTINGS,
        decisions=chosen,
    )


@pytest.mark.parametrize("aligned", [False, True])
def test_shared_jp_preserves_source_owner_context(
    db: Database, *, aligned: bool
) -> None:
    dual_region(db)
    chosen = replace(
        decisions(),
        aligned_regions=frozenset({("card", "en")}) if aligned else frozenset(),
        display_bindings=(
            DisplayBinding(
                "use", ("face_revision", "revision-en"), "zh-Hant", "shared_jp"
            ),
        ),
    )
    result = dual_project(db, chosen)
    assert one(result, "translation")["source_unit_id"] == TEXT
    assert (
        object_value(
            array(one(result, "face_revision", "revision")["translations"])[0]
        )["basis"]
        == "own_source"
    )
    assert one(result, "face_revision", "revision-en")["translations"] == (
        [
            {
                "field": "name",
                "ordinal": None,
                "target_lang": "zh-Hant",
                "translation_id": "translation",
                "basis": "shared_jp",
            }
        ]
        if aligned
        else []
    )
    with db.transaction():
        db.update(
            "region_divergence",
            {"card_id": "card", "region": "en", "field_scope": "name"},
            {"resolved": False},
        )
    assert (
        one(dual_project(db, chosen), "face_revision", "revision-en")["translations"]
        == []
    )


def test_official_counterpart_uses_direct_id_without_common_selection(
    db: Database,
) -> None:
    dual_region(db)
    with db.transaction():
        db.delete(
            "translation_selection", {"context_id": "context", "target_lang": "zh-Hant"}
        )
        db.update(
            "translation",
            {"id": "translation"},
            {
                "target_lang": "en",
                "origin": "official_sve",
                "authority": "sve_official",
            },
        )
    chosen = replace(
        decisions(),
        aligned_regions=frozenset({("card", "en")}),
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision"),
                "en",
                "official_counterpart",
                "translation",
            ),
        ),
    )
    result = dual_project(db, chosen)
    assert one(result, "face_revision", "revision-en")["translations"] == []
    assert (
        object_value(
            array(one(result, "face_revision", "revision")["translations"])[0]
        )["basis"]
        == "official_counterpart"
    )
    assert one(result, "translation")["source_unit_id"] == TEXT
    wrong_owner = replace(
        chosen,
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision-en"),
                "en",
                "official_counterpart",
                "translation",
            ),
        ),
    )
    with pytest.raises(ValueError, match="source owner"):
        dual_project(db, wrong_owner)
    with db.transaction():
        db.update("translation", {"id": "translation"}, {"authority": "unofficial"})
    with pytest.raises(ValueError, match="origin/authority"):
        dual_project(db, chosen)


def test_partial_mechanic_never_counts_as_fully_annotated(db: Database) -> None:
    result = projected(db)
    coverage = object_value(
        array(object_value(result.metadata["coverage"])["mechanics"])[0]
    )
    assert coverage["fully_annotated_cards"] == 0
    assert coverage["by_keyword"] == [{"keyword_id": "keyword", "complete_cards": 0}]
    assert coverage["unknown_cards"] == 0


def test_wording_candidate_must_be_an_observed_existing_revision(db: Database) -> None:
    with db.transaction():
        db.insert(
            "face_revision",
            dict(db.rows("face_revision")[0].values)
            | {"id": "unobserved", "revision": 2},
        )
    wording = pending() | {
        "display": {"revision_id": "revision", "basis": "current"},
        "candidates": [{"printing_id": "printing", "revision_id": "unobserved"}],
    }
    with pytest.raises(ValueError, match="observ"):
        projected(db, replace(decisions(), wording={"face": (wording,)}))


@pytest.mark.parametrize("status", ["draft", "stale"])
def test_unreviewed_translation_never_ships(db: Database, status: str) -> None:
    with db.transaction():
        db.update("translation", {"id": "translation"}, {"status": status})
    assert projected(db).tables["translation"] == []


def test_empty_mechanic_universe_needs_explicit_complete_all(db: Database) -> None:
    with db.transaction():
        db.delete("mechanic_projection", {"card_id": "card"})
        db.delete("keyword", {"id": "keyword"})
        db.update("text_symbol", {"id": "symbol"}, {"keyword_id": None})
        db.update(
            "card_mechanic_coverage", {"card_id": "card"}, {"partial_mode": "include"}
        )
        db.delete("translation_use", {"id": "use"})
    coverage = object_value(
        array(object_value(projected(db).metadata["coverage"])["mechanics"])[0]
    )
    assert coverage["fully_annotated_cards"] == 0
    assert coverage["unknown_cards"] == 1
    with db.transaction():
        db.update("card_mechanic_coverage", {"card_id": "card"}, {"complete_all": True})
    coverage = object_value(
        array(object_value(projected(db).metadata["coverage"])["mechanics"])[0]
    )
    assert coverage["fully_annotated_cards"] == 1
    assert coverage["unknown_cards"] == 0


def test_counterpart_replaces_only_its_owner_common_selection(db: Database) -> None:
    with db.transaction():
        db.insert(
            "translation",
            dict(db.rows("translation")[0].values)
            | {
                "id": "official",
                "target_lang": "en",
                "origin": "official_sve",
                "authority": "sve_official",
                "text": "Synthetic official",
            },
        )
        db.update("translation", {"id": "translation"}, {"target_lang": "en"})
        db.update(
            "translation_selection",
            {"context_id": "context", "target_lang": "zh-Hant"},
            {"target_lang": "en"},
        )
        db.insert(
            "translation_use",
            dict(db.rows("translation_use")[0].values)
            | {"id": "other-use", "face_revision_id": None, "product_id": "product"},
        )
    chosen = replace(
        decisions(),
        aligned_regions=frozenset({("card", "en")}),
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision"),
                "en",
                "official_counterpart",
                "official",
            ),
        ),
    )
    result = projected(db, chosen)
    assert (
        object_value(array(one(result, "face_revision")["translations"])[0])[
            "translation_id"
        ]
        == "official"
    )
    assert (
        object_value(array(one(result, "product")["translations"])[0])["translation_id"]
        == "translation"
    )
    with db.transaction():
        db.delete("translation_use", {"id": "other-use"})
    result = projected(db, chosen)
    assert [row["id"] for row in result.tables["translation"]] == ["official"]
    assert "Synthetic translation" not in [
        row["text"] for row in result.tables["text_unit"]
    ]


def test_logical_projection_runs_shared_a_semantic_checks(
    db: Database, monkeypatch: pytest.MonkeyPatch
) -> None:
    def broken(source: Source) -> dict[str, list[Record]]:
        view = initial(source)
        view["face_revision"][0]["type_code"] = "missing-vocabulary"
        return view

    monkeypatch.setattr("sve_carddb.snapshot.project.initial", broken)
    with pytest.raises(ValueError, match="Vocabulary reference"):
        projected(db)


def region(result: Projection, code: str = "jp") -> Record:
    return next(
        object_value(raw)
        for raw in array(one(result, "card", "card")["regions"])
        if object_value(raw)["region"] == code
    )


def test_unknown_inclusion_prevents_known_earliest(db: Database) -> None:
    with db.transaction():
        db.update(
            "product",
            {"id": "product"},
            {"date_precision": "day", "released_on": "2020-01-01"},
        )
        db.insert(
            "product",
            dict(db.rows("product")[0].values)
            | {
                "id": "undated-product",
                "date_precision": "unknown",
                "released_on": None,
            },
        )
        db.insert(
            "printing_product",
            dict(db.rows("printing_product")[0].values)
            | {"product_id": "undated-product"},
        )
    result = projected(db)
    assert region(result)["debut_state"] == "unknown"
    assert region(result)["debut_product_ids"] == []
    assert all(
        row["first_inclusion_state"] == "unknown"
        for row in result.tables["printing_product"]
    )


def test_unknown_other_printing_prevents_known_debut(db: Database) -> None:
    with db.transaction():
        db.update(
            "product",
            {"id": "product"},
            {"date_precision": "day", "released_on": "2020-01-01"},
        )
        db.insert(
            "printing",
            dict(db.rows("printing")[0].values)
            | {"id": "undated", "card_no": "TEST-002"},
        )
        db.insert(
            "card_int_id",
            {"printing_id": "undated", "int_id": 20002, "allocated_at": "2026-09-29"},
        )
        db.insert(
            "printing_face",
            dict(db.rows("printing_face")[0].values) | {"printing_id": "undated"},
        )
    result = projected(db)
    assert region(result)["debut_state"] == "unknown"
    assert region(result)["debut_product_ids"] == []
    assert one(result, "printing_product")["first_inclusion_state"] == "unknown"


def test_provisional_identity_with_two_regions_is_not_confirmed(db: Database) -> None:
    dual_region(db)
    with db.transaction():
        db.update("card", {"id": "card"}, {"identity_state": "provisional"})
        db.delete(
            "region_mapping_review",
            {"card_id": "card", "target_region": "en", "as_of": "2026-09-29"},
        )
    result = dual_project(db, decisions())
    assert region(result)["mapping_state"] == "unmapped"
    for code in ("jp", "en"):
        assert "identity_unconfirmed" in array(
            effective_support(one(result, "card_engine_support", "card"), code)[
                "reasons"
            ]
        )


@pytest.mark.parametrize(
    "level", ["proposed", "model_reviewed", "sampled", "confirmed"]
)
def test_confirmed_none_requires_reviewed_mapping(db: Database, level: str) -> None:
    result = projected(db)

    def exercise() -> None:
        with db.transaction():
            db.update("decision", {"id": "mapping_decision"}, {"state": level})
            source = Source(db)
            region_views(
                source, result.tables, "2026-10-01", Dates(source, result.tables), {}
            )
            assert region(result)["mapping_state"] == (
                "confirmed_none" if level == "confirmed" else "pending"
            )
            if level != "confirmed":
                projected(db)

    if level == "confirmed":
        exercise()
    else:
        with pytest.raises(sqlite3.IntegrityError, match="mapping_confirmed_none"):
            exercise()


def test_unreviewed_unlisted_is_not_released(db: Database) -> None:
    with db.transaction():
        db.insert(
            "decision",
            dict(db.rows("decision")[0].values)
            | {"id": "unreviewed", "state": "proposed"},
        )
        db.update(
            "printing",
            {"id": "printing"},
            {
                "catalog_state": "unlisted",
                "decision_id": "unreviewed",
                "decklog_available": False,
            },
        )
    result = projected(db)
    assert one(result, "printing")["review_level"] == "unreviewed"
    assert region(result)["release_state"] == "unknown"


@pytest.mark.parametrize("level", ["proposed", "model_reviewed", "sampled"])
def test_unconfirmed_availability_override_stays_unknown(
    db: Database, level: str
) -> None:
    with db.transaction():
        db.insert(
            "decision",
            dict(db.rows("decision")[0].values)
            | {"id": "availability", "state": level},
        )
        db.update(
            "region_availability_override",
            {"card_id": "card"},
            {"decision_id": "availability"},
        )
    assert region(projected(db), "en")["release_state"] == "unknown"


@pytest.mark.parametrize(
    "level", ["proposed", "model_reviewed", "sampled", "rejected", "disputed"]
)
def test_unconfirmed_role_override_does_not_replace_known_role(
    db: Database, level: str
) -> None:
    with db.transaction():
        db.delete(
            "face_special_kind",
            {"revision_id": "revision", "special_kind_code": "synthetic"},
        )
        db.insert(
            "decision",
            dict(db.rows("decision")[0].values) | {"id": "role", "state": level},
        )
        db.update(
            "deck_role_override",
            {"card_id": "card", "region": "jp"},
            {"decision_id": "role"},
        )
    assert region(projected(db))["deck_role"] == "main"


def test_conflicting_face_roles_are_unknown(db: Database) -> None:
    with db.transaction():
        db.delete("deck_role_override", {"card_id": "card", "region": "jp"})
        db.delete(
            "face_special_kind",
            {"revision_id": "revision", "special_kind_code": "synthetic"},
        )
        db.update("card", {"id": "card"}, {"layout": "double_faced"})
        db.insert(
            "vocabulary",
            {"kind": "type", "code": "leader", "label_unit_id": TEXT, "active": True},
        )
        db.insert(
            "face",
            dict(db.rows("face")[0].values)
            | {"id": "back", "ordinal": 1, "side": "back"},
        )
        db.insert(
            "face_revision",
            dict(db.rows("face_revision")[0].values)
            | {"id": "back-revision", "face_id": "back", "type_code": "leader"},
        )
        db.insert(
            "face_current",
            dict(db.rows("face_current")[0].values)
            | {"face_id": "back", "revision_id": "back-revision"},
        )
        db.insert(
            "printing_face",
            dict(db.rows("printing_face")[0].values)
            | {"face_id": "back", "art_id": None},
        )
    assert region(projected(db))["deck_role"] is None


@pytest.mark.parametrize("unknown", ["special_kind", "type"])
def test_unknown_role_codes_remain_null(db: Database, unknown: str) -> None:
    with db.transaction():
        db.delete("deck_role_override", {"card_id": "card", "region": "jp"})
        if unknown == "type":
            db.delete(
                "face_special_kind",
                {"revision_id": "revision", "special_kind_code": "synthetic"},
            )
            db.insert(
                "vocabulary",
                {
                    "kind": "type",
                    "code": "future-type",
                    "label_unit_id": TEXT,
                    "active": True,
                },
            )
            db.update("face_revision", {"id": "revision"}, {"type_code": "future-type"})
    assert region(projected(db))["deck_role"] is None


@pytest.mark.parametrize(
    ("scope", "resolved"), [("rules", True), ("all", True), ("name", False)]
)
def test_resolved_or_non_rules_divergence_does_not_block(
    db: Database, scope: str, *, resolved: bool
) -> None:
    with db.transaction():
        db.update(
            "region_divergence",
            {"card_id": "card", "region": "en", "field_scope": "name"},
            {"field_scope": scope, "resolved": resolved},
        )
    assert "region_divergence" not in array(
        effective_support(one(projected(db), "card_engine_support", "card"), "en")[
            "reasons"
        ]
    )


def test_qa_current_is_highest_revision_even_when_inserted_last(db: Database) -> None:
    with db.transaction():
        db.insert(
            "qa_version",
            dict(db.rows("qa_version")[0].values) | {"id": "qa-new", "revision": 7},
        )
    result = projected(db)
    assert one(result, "qa")["current_version_id"] == "qa-new"
    assert {row["id"] for row in result.tables["qa_version"]} == {"qa_v", "qa-new"}


def test_unapproved_image_with_variant_is_rejected_by_build_boundary(
    db: Database,
) -> None:
    def exercise() -> None:
        with db.transaction():
            db.update("image_asset", {"id": "image"}, {"publication_state": "pending"})
            projected(db)

    with pytest.raises(sqlite3.IntegrityError, match="image_variant_publishable"):
        exercise()


def test_offline_schema_version_is_verified_before_projection(
    tmp_path: Path, projection_schema: CompiledSchema
) -> None:
    path = tmp_path / "wrong-version.sqlite"
    with create_database(projection_schema, path) as database, database.transaction():
        populate(database)
    with sqlite3.connect(path) as connection:
        connection.execute("PRAGMA user_version = 999")
    with (
        open_database(projection_schema, path) as database,
        pytest.raises(sqlite3.IntegrityError, match="schema version mismatch"),
    ):
        projected(database)


def test_pipeline_checks_public_reference_closure(
    db: Database, monkeypatch: pytest.MonkeyPatch
) -> None:
    def damaged(source: Source) -> dict[str, list[Record]]:
        view = initial(source)
        view["product_family"][0]["name_unit_id"] = "t:ja:" + "e" * 16
        return view

    monkeypatch.setattr(project_module, "initial", damaged)
    with pytest.raises(ValueError, match="Dangling public reference to text_unit"):
        projected(db)


def test_default_projection_uses_native_selector_result_and_rejection(
    db: Database,
) -> None:
    expected = select_defaults(db)
    result = projected(db)
    selected = next(
        row for row in expected if row.card_id == "card" and row.region == "jp"
    )
    assert (
        region(result)["default_printing_id"],
        region(result)["default_method"],
    ) == (selected.printing_id, selected.method)
    with db.transaction():
        db.insert(
            "decision",
            dict(db.rows("decision")[0].values)
            | {"id": "default-unconfirmed", "state": "sampled"},
        )
        db.update(
            "default_printing_override",
            {"card_id": "card", "region": "jp"},
            {"decision_id": "default-unconfirmed"},
        )
    with pytest.raises(ValueError, match="confirmed"):
        projected(db)


def test_single_region_defaults_only_reference_public_printings(db: Database) -> None:
    dual_region(db)
    assert {row.values["region"] for row in db.rows("printing")} == {"jp", "en"}
    assert {item.region for item in select_defaults(db)} == {"jp", "en"}

    result = projected(db)
    assert {row["id"] for row in result.tables["printing"]} == {"printing"}
    assert region(result)["default_printing_id"] == "printing"
    assert region(result, "en")["default_printing_id"] is None
    assert region(result, "en")["default_method"] is None


def test_native_model_adapter_keeps_full_observation_payload(db: Database) -> None:
    payload: Record = {
        "revision_id": "revision",
        "state": "correction_conflict",
        "source_url": "https://example.invalid/unavailable-source",
    }
    chosen = decisions().with_text_views(
        {},
        {("printing", "face"): (RootModel[dict[str, JsonValue]](payload),)},
    )
    assert chosen.active_scopes == decisions().active_scopes
    result = projected(db, chosen)
    assert object_value(array(one(result, "printing")["faces"])[0])["observations"] == [
        payload
    ]


def test_native_missing_effect_without_database_observation_is_preserved(
    db: Database,
) -> None:
    with db.transaction():
        db.delete("face_current", {"face_id": "face", "region": "jp"})
        db.delete(
            "printing_face_observation",
            {"printing_id": "printing", "face_id": "face", "source_id": "source"},
        )
    wording = pending() | {
        "display": {"revision_id": None, "basis": "candidates"},
        "candidates": [{"printing_id": "printing", "revision_id": None}],
    }
    observed: Record = {
        "revision_id": None,
        "state": "missing_effect",
        "source_url": "https://example.invalid/unavailable",
    }
    chosen = decisions().with_text_views(
        {"face": (RootModel[dict[str, JsonValue]](wording),)},
        {("printing", "face"): (RootModel[dict[str, JsonValue]](observed),)},
    )
    result = projected(db, chosen)
    assert one(result, "face")["current"] == []
    assert one(result, "face")["wording"] == [wording]
    assert object_value(array(one(result, "printing")["faces"])[0])["observations"] == [
        observed
    ]
    assert "wording_pending" in array(
        effective_support(one(result, "card_engine_support", "card"), "jp")["reasons"]
    )


@pytest.mark.parametrize(
    "payload",
    [
        {
            "revision_id": None,
            "state": "available",
            "source_url": "https://example.invalid/source",
        },
        {
            "revision_id": "revision",
            "state": "missing_effect",
            "source_url": "https://example.invalid/source",
        },
        {
            "revision_id": "revision",
            "state": "unknown",
            "source_url": "https://example.invalid/source",
        },
        {
            "revision_id": "revision",
            "state": "available",
            "source_url": "https://example.invalid/source",
            "source_id": "private",
        },
    ],
)
def test_native_observation_boundary_rejects_invalid_public_payload(
    db: Database, payload: Record
) -> None:
    chosen = decisions().with_text_views(
        {}, {("printing", "face"): (RootModel[dict[str, JsonValue]](payload),)}
    )
    with pytest.raises(ValueError, match=r"Observation|observation"):
        projected(db, chosen)


def test_direct_own_name_replaces_shared_choice_only_on_its_owner(db: Database) -> None:
    with db.transaction():
        row = dict(db.rows("translation")[0].values)
        db.insert(
            "translation",
            row
            | {
                "id": "official-name",
                "origin": "official_sv1",
                "authority": "digital_official",
                "text": "Synthetic official name",
            },
        )
    chosen = replace(
        decisions(),
        display_bindings=(
            DisplayBinding(
                "use",
                ("face_revision", "revision"),
                "zh-Hant",
                "own_source",
                "official-name",
            ),
        ),
    )
    result = projected(db, chosen)
    bindings = result.tables["face_revision"][0]["translations"]
    assert [
        (object_value(row)["translation_id"], object_value(row)["basis"])
        for row in array(bindings)
    ] == [("official-name", "own_source")]
    assert [row["id"] for row in result.tables["translation"]] == ["official-name"]


def test_direct_own_name_never_transfers_to_another_owner(db: Database) -> None:
    changed = replace(
        decisions(),
        display_bindings=(
            DisplayBinding(
                "use",
                ("printing_face", "printing", "face"),
                "zh-Hant",
                "own_source",
                "translation",
            ),
        ),
    )
    with pytest.raises(
        ValueError, match=r"^Direct own-source binding must keep its exact owner$"
    ):
        projected(db, changed)


def test_names_only_projection_keeps_digital_evidence_private(db: Database) -> None:
    result = projected(db, replace(decisions(), private_digital=True))
    assert all(
        not result.tables[name]
        for name in (
            "digital_card",
            "digital_art",
            "digital_link",
            "digital_art_link",
            "digital_link_coverage",
            "voice",
            "card_voice",
        )
    )
    assert not result.config["digital_endpoints"]

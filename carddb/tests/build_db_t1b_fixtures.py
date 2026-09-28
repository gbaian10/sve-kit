"""Synthetic connected minimum and EN graphs with independent review decisions."""

from typing import TYPE_CHECKING

from sve_carddb.build_db import Json
from sve_carddb.build_db.t1 import TABLES

from .build_db_fixtures import DATE, HASH, INSTANT
from .build_db_fixtures import rows as base_rows
from .build_db_t1_fixtures import populate as populate_a

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value

EN_TABLES = {"art", "region_mapping_review", "region_text_review", "region_divergence"}


def rows(*, include_en: bool = True) -> dict[str, dict[str, Value]]:
    values: dict[str, dict[str, Value]] = {
        "errata": {
            "id": "errata",
            "region": "jp",
            "official_url": "https://example.invalid/errata",
        },
        "errata_version": {
            "id": "errata_v",
            "errata_id": "errata",
            "revision": 0,
            "source_id": "source",
        },
        "errata_change": {
            "id": "change",
            "errata_version_id": "errata_v",
            "face_id": "face",
            "before_revision_id": "revision",
            "after_revision_id": "revision",
            "before_value": Json("Before"),
            "after_value": Json("Synthetic text"),
            "field": "effect",
        },
        "errata_printing": {
            "errata_version_id": "errata_v",
            "printing_id": "printing",
            "scope": "confirmed_applies",
            "decision_id": "errata_decision",
        },
        "source_correction": {
            "id": "correction",
            "printing_id": "printing",
            "face_id": "face",
            "field": "effect",
            "expected_raw_value": Json("Before"),
            "corrected_value": Json("Synthetic text"),
            "expected_source_hash": HASH,
            "reason": "Synthetic correction",
            "decision_id": "correction_decision",
            "reported_to_official": False,
            "state": "active",
        },
        "correction_evidence": {
            "correction_id": "correction",
            "source_id": "source",
            "kind": "card_image",
            "locator": "Synthetic locator",
        },
        "correction_application": {
            "correction_id": "correction",
            "source_id": "source",
            "result_unit_id": "text",
            "face_revision_id": "revision",
            "status": "applied",
        },
        "qa": {
            "id": "qa",
            "region": "jp",
            "official_number": "Q1",
            "stable_source_key": "synthetic:qa",
            "source_url": "https://example.invalid/qa",
        },
        "qa_version": {
            "id": "qa_v",
            "qa_id": "qa",
            "revision": 0,
            "published_on": DATE,
            "observed_at": INSTANT,
            "question_unit_id": "text",
            "answer_unit_id": "text",
            "state": "active",
            "source_id": "source",
        },
        "qa_card": {"qa_version_id": "qa_v", "card_id": "card"},
        "card_related": {
            "id": "related",
            "from_card_id": "card2",
            "to_card_id": "card",
            "relation": "same_rules_reskin",
            "source_kind": "authored",
            "decision_id": "related_decision",
        },
        "art": {
            "id": "art",
            "card_id": "card",
            "face_id": "face",
            "classification": "unclassified",
            "decision_id": "decision",
        },
        "region_mapping_review": {
            "card_id": "card2",
            "target_region": "en",
            "state": "confirmed_none",
            "as_of": DATE,
            "coverage_scope": "Synthetic scope",
            "source_id": "source",
            "decision_id": "mapping_decision",
        },
        "region_text_review": {
            "card_id": "card",
            "region": "en",
            "source_jp_hash": HASH,
            "source_region_hash": HASH,
            "state": "aligned",
            "decision_id": "text_decision",
            "checked_at": INSTANT,
        },
        "region_divergence": {
            "card_id": "card",
            "region": "en",
            "field_scope": "name",
            "reason": "Synthetic divergence",
            "effect": "manual",
            "source_id": "source",
            "decision_id": "divergence_decision",
            "resolved": True,
        },
    }
    for table in TABLES:
        if table.name in values:
            for column in table.columns:
                if column.nullable:
                    values[table.name].setdefault(column.name, None)
    return {
        name: row for name, row in values.items() if include_en or name not in EN_TABLES
    }


def populate(db: Database, *, include_en: bool = True) -> None:
    populate_a(db)
    base = base_rows()
    for prefix in ("errata", "correction", "related", "mapping", "text", "divergence"):
        db.insert("decision", base["decision"] | {"id": prefix + "_decision"})
    db.insert("card", base["card"] | {"id": "card2"})
    db.insert("face", base["face"] | {"id": "face2", "card_id": "card2"})
    db.insert(
        "printing",
        base["printing"]
        | {"id": "printing2", "card_id": "card2", "card_no": "TEST-002"},
    )
    db.insert(
        "printing_face",
        base["printing_face"]
        | {"printing_id": "printing2", "face_id": "face2", "card_id": "card2"},
    )
    db.insert(
        "face_revision", base["face_revision"] | {"id": "revision2", "face_id": "face2"}
    )
    db.insert(
        "face_revision", base["face_revision"] | {"id": "revision_en", "region": "en"}
    )
    if include_en:
        db.insert(
            "printing",
            base["printing"]
            | {"id": "printing_en", "region": "en", "card_no": "TEST-001EN"},
        )
        db.insert(
            "printing_face", base["printing_face"] | {"printing_id": "printing_en"}
        )
    for name, row in reversed(rows(include_en=include_en).items()):
        db.insert(name, row)
    if include_en:
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"art_id": "art"},
        )

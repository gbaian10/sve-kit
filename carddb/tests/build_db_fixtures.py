"""Synthetic rows spanning every T0 table; no official card text or local data."""

from typing import TYPE_CHECKING

from sve_carddb.build_db import Json, Value
from sve_carddb.build_db.t0 import TABLES

if TYPE_CHECKING:
    from sve_carddb.build_db.database import Database

HASH = "sha256:" + "a" * 64
DATE = "2026-09-29"
INSTANT = "2026-09-29T00:00:00Z"


def rows() -> dict[str, dict[str, Value]]:
    """Supply one connected row per table, with explicit synthetic relationships."""
    values: dict[str, dict[str, Value]] = {
        "source_record": {"id": "source", "kind": "authored", "sha256": HASH},
        "decision": {
            "id": "decision",
            "state": "confirmed",
            "scope": "record",
            "category": "synthetic",
            "note": "",
        },
        "decision_source": {
            "decision_id": "decision",
            "source_id": "source",
            "role": "synthetic",
        },
        "language": {
            "code": "ja",
            "fallback_order": Json([]),
            "display_name": "Synthetic language",
        },
        "vocabulary": {
            "kind": "type",
            "code": "follower",
            "label_unit_id": "text",
            "active": True,
        },
        "card": {
            "id": "card",
            "layout": "single",
            "identity_state": "confirmed",
            "home_set_id": "family",
        },
        "face": {"id": "face", "card_id": "card", "ordinal": 0, "side": "front"},
        "identity_change": {
            "id": "change",
            "kind": "merge",
            "old_card_id": "old_card",
            "new_card_id": "card",
            "data_version": "preview-20260929T000000Z-0001",
            "reason": "Synthetic merge",
        },
        "card_int_id": {
            "int_id": 20001,
            "printing_id": "printing",
        },
        "rules_name": {
            "id": "rules_name",
            "region": "jp",
            "official_name": "Synthetic name",
        },
        "face_rules_name": {
            "face_id": "face",
            "region": "jp",
            "rules_name_id": "rules_name",
            "role": "primary",
        },
        "product_family": {
            "id": "family",
            "code": "test",
            "public_code": "TEST",
            "kind": "other",
            "name_unit_id": "text",
        },
        "product": {
            "id": "product",
            "region": "jp",
            "name_unit_id": "text",
            "product_type": "other",
            "date_precision": "unknown",
            "source_id": "source",
        },
        "printing": {
            "id": "printing",
            "card_id": "card",
            "region": "jp",
            "card_no": "TEST-001Ⓢa",
            "card_no_state": "official",
            "catalog_state": "official",
            "decklog_available": True,
            "decklog_verification": "unverified",
            "variant_key": "standard",
            "home_set_id": "family",
            "rarity_raw": "",
            "source_id": "source",
        },
        "printing_product": {
            "printing_id": "printing",
            "product_id": "product",
            "inclusion_kind": "pack",
            "source_id": "source",
        },
        "printing_face": {
            "printing_id": "printing",
            "face_id": "face",
            "card_id": "card",
            "embellishment_state": "unreviewed",
            "printed_text_state": "unknown",
            "source_id": "source",
        },
        "text_unit": {
            "id": "text",
            "lang": "ja",
            "text": "Synthetic text",
            "content_hash": HASH,
        },
        "face_revision": {
            "id": "revision",
            "face_id": "face",
            "region": "jp",
            "revision": 0,
            "temporal_status": "unknown",
            "observed_at": INSTANT,
            "change_kind": "initial",
            "name_unit_id": "text",
            "effect_unit_id": "text",
            "type_code": "follower",
            "source_id": "source",
        },
        "face_text_section": {
            "revision_id": "revision",
            "ordinal": 0,
            "text_unit_id": "text",
            "kind": "rule",
        },
        "printing_text_section": {
            "printing_id": "printing",
            "face_id": "face",
            "ordinal": 0,
            "text_unit_id": "text",
            "kind": "unknown",
            "source_id": "source",
        },
        "face_trait": {"revision_id": "revision", "trait_code": "synthetic"},
        "face_title": {"revision_id": "revision", "title_code": "synthetic"},
        "face_special_kind": {
            "revision_id": "revision",
            "special_kind_code": "synthetic",
        },
        "face_current": {
            "face_id": "face",
            "region": "jp",
            "revision_id": "revision",
            "basis": "latest_observed_no_errata",
        },
        "printing_face_observation": {
            "printing_id": "printing",
            "face_id": "face",
            "source_id": "source",
            "revision_id": "revision",
            "observed_at": INSTANT,
        },
        "source_coverage": {
            "kind": "cardlist",
            "region": "jp",
            "scope_key": "synthetic",
            "from_date": DATE,
            "as_of": DATE,
            "state": "partial",
            "source_id": "source",
        },
        "rules_profile": {
            "id": "profile",
            "region": "jp",
            "format_code": "standard",
            "name_unit_id": "text",
        },
        "rules_profile_revision": {
            "id": "profile_revision",
            "profile_id": "profile",
            "effective_from": DATE,
            "source_id": "source",
        },
        "restriction": {
            "id": "restriction",
            "profile_id": "profile",
            "effective_from": DATE,
            "kind": "copy_limit",
            "state": "confirmed",
            "max_copies": 0,
            "source_id": "source",
        },
        "restriction_member": {
            "restriction_id": "restriction",
            "rules_name_id": "rules_name",
            "choice_option": 0,
            "deck_scope": "all",
        },
        "restriction_coverage": {
            "profile_id": "profile",
            "from_date": DATE,
            "state": "partial",
            "source_id": "source",
        },
        "deck_role_override": {
            "card_id": "card",
            "region": "jp",
            "role": "extra",
            "decision_id": "decision",
        },
        "card_engine_support": {
            "card_id": "card",
            "region": "jp",
            "status": "missing_dsl",
            "validation_state": "not_applicable",
            "reason_codes": Json(["missing_dsl"]),
            "automatic": False,
        },
        "build_issue": {
            "id": "issue",
            "category": "synthetic",
            "severity": "warning",
            "entity_type": "card",
            "entity_id": "card",
            "message": "Synthetic diagnostic",
        },
        "text_symbol": {
            "id": "symbol",
            "code": "synthetic",
            "parameter_schema": Json({"parameters": []}),
            "spellings": Json(
                [
                    {
                        "lang": "ja",
                        "literal_prefix": "{Q}",
                        "literal_suffix": "",
                        "parameter_name": None,
                        "parse_kind": "literal",
                    }
                ]
            ),
            "localizations": Json(
                [
                    {
                        "lang": "ja",
                        "name": "Synthetic symbol",
                        "tooltip": "",
                        "copy_pattern": "{Q}",
                    }
                ]
            ),
            "decision_id": "decision",
        },
        "card_route": {
            "namespace": "official",
            "route_key": "TEST-001%E2%93%88a",
            "printing_id": "printing",
        },
        "card_route_alias": {
            "namespace": "provisional",
            "old_key": "20001",
            "target_namespace": "official",
            "target_key": "TEST-001%E2%93%88a",
            "reason": "provisional_corrected",
            "source_id": "source",
            "decision_id": "decision",
        },
        "route_override": {
            "route_key": "TEST-001%E2%93%88a",
            "printing_id": "printing",
            "decision_id": "decision",
        },
        "default_printing_override": {
            "card_id": "card",
            "region": "jp",
            "printing_id": "printing",
            "decision_id": "decision",
        },
        "search_alias": {
            "kind": "card",
            "code": "card",
            "lang": "ja",
            "text": "Synthetic alias",
            "normalized": "synthetic alias",
            "normalizer_version": "synthetic-v1",
        },
    }
    for table in TABLES:
        for column in table.columns:
            if column.nullable:
                values[table.name].setdefault(column.name, None)
    return values


def seed(db: Database) -> dict[str, dict[str, Value]]:
    """Insert dependents before parents, exercising real deferred foreign keys."""
    values = rows()
    with db.transaction():
        for name, row in reversed(values.items()):
            db.insert(name, row)
        db.insert(
            "card", values["card"] | {"id": "old_card", "identity_state": "retired"}
        )
        for kind in ("trait", "title", "special_kind", "class", "rarity", "frame"):
            db.insert(
                "vocabulary", values["vocabulary"] | {"kind": kind, "code": "synthetic"}
            )
    return values

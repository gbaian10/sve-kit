"""Independent synthetic build rows, without official text, images or credentials."""

from dataclasses import replace
from itertools import starmap
from typing import TYPE_CHECKING

from sve_carddb.build_db import Capability, Column, Json, Kind, Table, compile_schema
from sve_carddb.build_db.registry import Registry
from sve_carddb.build_db.t1 import REGISTRY, compile_minimum
from sve_carddb.snapshot.project import Decisions, Settings
from sve_carddb.snapshot.values import digest, parse

from .build_db_fixtures import rows as core_rows
from .build_db_t1_fixtures import rows as image_rows
from .build_db_t1b_fixtures import rows as ancillary_rows

if TYPE_CHECKING:
    from sve_carddb.build_db import CompiledSchema, Database, Value

TEXT = "t:ja:" + digest(b"Synthetic text")[7:23]
SETTINGS = Settings("https://example.invalid/feedback", "synthetic-v1", "synthetic-v1")


def extras() -> dict[str, dict[str, Value]]:
    return {
        "artist": {
            "id": "artist",
            "display_name": "Synthetic artist",
            "decision_id": "decision",
        },
        "art_artist": {
            "art_id": "art",
            "artist_id": "artist",
            "role": "illustration",
            "source_id": "source",
        },
        "stamp": {
            "id": "stamp",
            "code": "synthetic",
            "series_code": None,
            "text_raw": "Synthetic stamp",
            "kind": "event",
            "displayed_year": None,
            "decision_id": "decision",
        },
        "printing_stamp": {
            "printing_id": "printing",
            "face_id": "face",
            "stamp_id": "stamp",
            "position": None,
            "color": None,
            "decision_id": "decision",
        },
        "printing_reference": {
            "printing_id": "printing",
            "source_id": "source",
            "role": "identity",
            "locator": "private locator",
        },
        "region_availability_override": {
            "card_id": "card",
            "region": "en",
            "state": "announced",
            "as_of": "2026-09-29",
            "source_id": "source",
            "decision_id": "decision",
        },
        "digital_card": {
            "id": "digital",
            "game": "sv1",
            "official_id": "100000001",
            "source_id": "source",
        },
        "digital_face": {
            "id": "digital-face",
            "digital_card_id": "digital",
            "phase": "normal",
            "source_id": "source",
        },
        "digital_art": {
            "id": "digital-art",
            "digital_face_id": "digital-face",
            "style_key": "standard",
            "source_id": "source",
        },
        "digital_link": {
            "id": "digital-link",
            "card_id": "card",
            "face_id": "face",
            "digital_card_id": "digital",
            "digital_face_id": "digital-face",
            "relation": "same_card",
            "effect_similarity": None,
            "decision_id": "decision",
        },
        "digital_art_link": {
            "art_id": "art",
            "digital_art_id": "digital-art",
            "relation": "same_art",
            "decision_id": "decision",
        },
        "digital_link_coverage": {
            "card_id": "card",
            "game": "sv1",
            "state": "reviewed_matches",
            "as_of": "2026-09-29",
            "decision_id": "decision",
        },
        "voice": {
            "id": "voice",
            "digital_card_id": "digital",
            "digital_face_id": "digital-face",
            "lang": "ja",
            "kind_code": "play",
            "variant": None,
            "interaction_target_id": None,
            "label_raw": None,
            "source_url": "https://example.invalid/audio",
            "asset_path": None,
            "mime": None,
            "bytes": None,
            "duration_ms": None,
            "availability": "remote_only",
            "source_id": "source",
        },
        "card_voice": {
            "card_id": "card",
            "voice_id": "voice",
            "usage": "browse",
            "digital_link_id": "digital-link",
            "decision_id": "decision",
        },
        "glossary_term": {
            "id": "term",
            "source_ja": "Synthetic keyword",
            "category": "keyword",
            "concept_key": "synthetic",
            "decision_id": "decision",
        },
        "keyword": {
            "id": "keyword",
            "code": "synthetic",
            "kind": "mechanic",
            "term_id": "term",
            "definition_unit_id": TEXT,
            "decision_id": "decision",
        },
        "mechanic_action": {
            "keyword_id": "keyword",
            "action": "produce",
            "label_unit_id": TEXT,
            "decision_id": "decision",
        },
        "mechanic_projection": {
            "card_id": "card",
            "keyword_id": "keyword",
            "scope": "shared",
            "relations": Json(["grants"]),
            "actions": Json([]),
        },
        "card_mechanic_coverage": {
            "card_id": "card",
            "scope": "shared",
            "complete_all": False,
            "complete_mode": "include",
            "complete_keyword_ids": Json([]),
            "partial_mode": "exclude",
            "partial_keyword_ids": Json([]),
        },
        "ruling_revision": {
            "id": "ruling-rev",
            "ruling_id": "ruling",
            "revision": 1,
            "question_unit_id": TEXT,
            "decision_unit_id": TEXT,
            "strength": "inferred",
            "decided_by": "PRIVATE AUTHOR",
            "decided_on": "2026-09-29",
            "review_state": "current",
        },
        "ruling_evidence": {
            "id": "evidence",
            "ruling_revision_id": "ruling-rev",
            "qa_version_id": "qa_v",
            "cr_clause_id": None,
            "source_id": None,
            "role": "supporting",
            "quote": "Synthetic text",
            "locator": "answer",
        },
        "ruling_card": {"ruling_revision_id": "ruling-rev", "card_id": "card"},
        "ruling_hint": {
            "ruling_revision_id": "ruling-rev",
            "lang": "ja",
            "text_unit_id": TEXT,
            "parameter_schema": Json({"parameters": []}),
        },
        "ruling_supersession": {
            "old_revision_id": "ruling-rev",
            "new_revision_id": "ruling-rev2",
            "scope_unit_id": TEXT,
        },
        "translation_context": {
            "id": "context",
            "source_unit_id": TEXT,
            "semantic_variant": "default",
            "decision_id": None,
        },
        "translation": {
            "id": "translation",
            "context_id": "context",
            "target_lang": "zh-Hant",
            "revision": 1,
            "text": "Synthetic translation",
            "origin": "project",
            "authority": "unofficial",
            "status": "reviewed",
            "source_hash": "sha256:" + "a" * 64,
            "translated_by": "PRIVATE TRANSLATOR",
        },
        "translation_selection": {
            "context_id": "context",
            "target_lang": "zh-Hant",
            "translation_id": "translation",
        },
        "translation_use": {
            "id": "use",
            "context_id": "context",
            "field": "name",
            "ordinal": None,
            "face_revision_id": "revision",
            "printing_id": None,
            "face_id": None,
            "qa_version_id": None,
            "cr_clause_id": None,
            "vocabulary_kind": None,
            "vocabulary_code": None,
            "keyword_id": None,
            "product_family_id": None,
            "product_id": None,
        },
        "digital_endpoint": {
            "game": "sv1",
            "card_url_template": "https://example.invalid/card/{official_id}?lang={provider_lang}",
            "language_map": Json({"ja": "ja", "zh-Hant": "zh-tw"}),
            "status": "unknown",
            "refresh_policy": "frozen",
            "last_checked_at": None,
        },
        "shop_link_template": {
            "id": "shop",
            "url_template": "https://example.invalid/shop/{card_no}?region={region}",
            "parameters": Json(["card_no", "region"]),
            "feature_key": "synthetic",
            "enabled_dev": True,
            "enabled_prod": False,
            "name": "Private config label",
            "region": "jp",
            "decision_id": "decision",
        },
    }


def _column(name: str, value: Value) -> Column:
    if value is None and name in {"ordinal", "bytes", "duration_ms", "displayed_year"}:
        return Column(name, Kind.UINT, nullable=True)
    if isinstance(value, Json):
        return Column(name, Kind.JSON, json_schema="synthetic-any")
    if type(value) is bool:
        return Column(name, Kind.BOOL)
    if type(value) is int:
        return Column(name, Kind.UINT)
    return Column(name, Kind.TEXT, nullable=value is None or name == "face_revision_id")


def schema(*, nullable_observation: bool = False) -> CompiledSchema:
    base = compile_minimum(include_en=True)
    extra = tuple(
        Table(name, tuple(starmap(_column, row.items())), (next(iter(row)),))
        for name, row in extras().items()
    )
    base_tables = tuple(
        replace(
            table,
            columns=tuple(
                replace(column, nullable=True)
                if nullable_observation
                and table.name == "printing_face_observation"
                and column.name == "revision_id"
                else column
                for column in table.columns
            ),
        )
        for table in base.tables
    )
    all_tables = (*base_tables, *extra)
    defined = {table.name for table in all_tables}
    reserved = tuple(
        Capability(
            cap.name,
            tuple(name for name in cap.tables if name not in defined),
            implemented=False,
        )
        for cap in REGISTRY.capabilities
        if any(name not in defined for name in cap.tables)
    )
    registry = Registry(
        all_tables,
        (Capability("synthetic", tuple(table.name for table in all_tables)), *reserved),
    )
    return compile_schema(
        registry,
        ("synthetic",),
        {name: parse(value.encode()) for name, value in base.json_schemas}
        | {"synthetic-any": {}},
        version=base.version,
    )


def populate(db: Database, *, ancillary: bool = True, future: bool = True) -> None:
    values = core_rows() | (image_rows() | ancillary_rows() if ancillary else {})
    for name, row in values.items():
        converted = {
            key: TEXT if value == "text" else value for key, value in row.items()
        }
        if name == "source_record":
            converted["url"] = "https://example.invalid/card"
            converted["raw_locator"] = "PRIVATE RAW LOCATOR"
        if name == "source_coverage":
            converted["scope_key"] = "region:*"
        if name == "card_related":
            converted["from_card_id"] = "old_card"
        if name == "region_mapping_review":
            converted["card_id"] = "card"
        db.insert(name, converted)
    db.insert("card", values["card"] | {"id": "old_card", "identity_state": "retired"})
    for kind in ("trait", "title", "special_kind", "class", "rarity", "frame"):
        db.insert(
            "vocabulary",
            {"kind": kind, "code": "synthetic", "label_unit_id": TEXT, "active": True},
        )
    if ancillary:
        _ancillary(db, values)
    if future:
        _future(db)


def _ancillary(db: Database, values: dict[str, dict[str, Value]]) -> None:
    for prefix in (
        "errata",
        "correction",
        "related",
        "mapping",
        "text",
        "divergence",
    ):
        db.insert("decision", values["decision"] | {"id": prefix + "_decision"})
    db.update(
        "printing_face",
        {"printing_id": "printing", "face_id": "face"},
        {"art_id": "art"},
    )


def _future(db: Database) -> None:
    for name, row in extras().items():
        db.insert(name, row)
    db.insert(
        "ruling_revision",
        extras()["ruling_revision"]
        | {"id": "ruling-rev2", "revision": 2, "review_state": "needs_review"},
    )
    db.insert(
        "language",
        {
            "code": "zh-Hant",
            "display_name": "Synthetic language",
            "fallback_order": Json(["ja"]),
        },
    )


def decisions() -> Decisions:
    return Decisions(
        active_scopes={"ruling-rev": (TEXT,), "ruling-rev2": (TEXT,)},
        related_regions={"related": ("jp",)},
    )

"""Minimal synthetic glossary envelopes and a reusable digital-name database."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.snapshot.values import canonical, digest

from .database_fixtures import DatabaseTemplate

if TYPE_CHECKING:
    from pathlib import Path

HASH = "sha256:" + "a" * 64
INSTANT = "2026-10-02T00:00:00Z"


def reference(
    *, provider: str = "svwb", locator: str = "/data/skill_names/1"
) -> dict[str, JsonValue]:
    return {
        "store_id": "synthetic",
        "batch_id": HASH,
        "source_version_id": "src:v1:" + "b" * 64,
        "parser": "translation-" + provider + "-v1",
        "locator": locator,
        "text_hash": HASH,
    }


def term(
    identifier: str = "rule.test", *, category: str = "rule_term"
) -> dict[str, JsonValue]:
    return {
        "record_key": canonical(["glossary_term", "term:" + identifier]).decode(),
        "kind": "glossary_term",
        "filing_key": "concepts",
        "data": {
            "id": "term:" + identifier,
            "category": category,
            "concept_key": identifier,
            "source_ref": reference(),
            "source_span": None,
        },
        "evidence": [],
    }


def choice(
    identifier: str = "rule.test",
    *,
    number: int = 1,
    predecessor: JsonValue = None,
    value: JsonValue = "同名",
) -> dict[str, JsonValue]:
    return {
        "record_key": canonical(
            ["glossary_choice", "term:" + identifier, "zh-Hant", number]
        ).decode(),
        "kind": "glossary_choice",
        "filing_key": "choices",
        "data": {
            "term_id": "term:" + identifier,
            "lang": "zh-Hant",
            "value": None if value is None else {"kind": "authored", "text": value},
            "origin": "project",
            "concept_evidence": [],
            "adoption_no": number,
            "predecessor": predecessor,
        },
        "evidence": [],
    }


def envelope(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    records.sort(key=lambda r: str(r["record_key"]))
    members: list[JsonValue] = [
        [r["record_key"], digest(canonical(r))] for r in records
    ]
    checksum = digest(canonical(members))
    identifier = "d:" + checksum[7:]
    return {
        "translation_authored_format": 1,
        "kind": "translation_shard",
        "default_decision_id": identifier,
        "records": list[JsonValue](records),
        "decisions": [
            {
                "id": identifier,
                "state": "confirmed",
                "scope": "batch",
                "category": records[0]["kind"],
                "policy_id": "synthetic-glossary-v1",
                "membership_hash": checksum,
                "members": members,
                "sample_ids": [r["record_key"] for r in records],
                "authored_by": "Synthetic author",
                "authored_at": INSTANT,
                "reviewed_by": "Synthetic human",
                "reviewed_at": INSTANT,
                "reviewed_precision": "day",
                "note": "Synthetic decision.",
            }
        ],
    }


def write(root: Path, shards: dict[str, dict[str, JsonValue]]) -> None:
    index: dict[str, JsonValue] = {
        "translation_authored_format": 1,
        "kind": "translation_index",
        "includes": {p: digest(canonical(v)) for p, v in shards.items()},
        "inventories": {},
    }
    for name, value in {"translations/index.yaml": index, **shards}.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(canonical(value))


def template() -> DatabaseTemplate:
    schema = compile_build(("t0", "translation_evidence"))
    with create_database(schema) as db:
        with db.transaction():
            db.insert(
                "source_record", {"id": "source", "kind": "authored", "sha256": HASH}
            )
            db.insert(
                "decision",
                {
                    "id": "decision",
                    "state": "confirmed",
                    "scope": "record",
                    "category": "synthetic",
                    "authored_by": "Synthetic",
                    "authored_at": INSTANT,
                    "reviewed_by": "Synthetic human",
                    "reviewed_at": INSTANT,
                    "note": "",
                },
            )
            for lang in ("ja", "en", "zh-Hant"):
                db.insert(
                    "language",
                    {"code": lang, "display_name": lang, "fallback_order": Json([])},
                )
            db.insert(
                "text_unit",
                {
                    "id": "family-name",
                    "lang": "ja",
                    "text": "Synthetic family",
                    "content_hash": HASH,
                },
            )
            db.insert(
                "product_family",
                {
                    "id": "synthetic",
                    "code": "synthetic",
                    "public_code": "SYNTHETIC",
                    "kind": "other",
                    "name_unit_id": "family-name",
                    "decision_id": "decision",
                },
            )
            db.insert(
                "card",
                {
                    "id": "card",
                    "layout": "double_faced",
                    "identity_state": "provisional",
                    "home_set_id": "synthetic",
                },
            )
            db.insert(
                "face",
                {"id": "front", "card_id": "card", "ordinal": 0, "side": "front"},
            )
            db.insert(
                "face", {"id": "back", "card_id": "card", "ordinal": 1, "side": "back"}
            )
            for game, number in (("sv1", "123456789"), ("svwb", "12345678")):
                db.insert(
                    "digital_card",
                    {
                        "id": game,
                        "game": game,
                        "official_id": number,
                        "source_id": "source",
                    },
                )
                for phase, owner in (("normal", "front"), ("evolved", "back")):
                    face = game + ":" + phase
                    db.insert(
                        "digital_face",
                        {
                            "id": face,
                            "digital_card_id": game,
                            "phase": phase,
                            "source_id": "source",
                        },
                    )
                    for lang in ("ja", "zh-Hant"):
                        unit = face + ":" + lang
                        db.insert(
                            "text_unit",
                            {
                                "id": unit,
                                "lang": lang,
                                "text": face + " " + lang,
                                "content_hash": digest(unit.encode()),
                            },
                        )
                        db.insert(
                            "digital_text",
                            {
                                "digital_face_id": face,
                                "lang": lang,
                                "name_unit_id": unit,
                            },
                        )
                    db.insert(
                        "digital_link",
                        {
                            "id": face,
                            "card_id": "card",
                            "face_id": owner,
                            "digital_card_id": game,
                            "digital_face_id": face,
                            "relation": "same_card",
                            "decision_id": "decision",
                        },
                    )
        return DatabaseTemplate(schema, db._connection.serialize())

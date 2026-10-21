"""Synthetic provenance around independently written official ability examples."""

from pydantic import JsonValue


def source_use() -> dict[str, JsonValue]:
    return {
        "binding_id": "binding:fixture",
        "ability_scope": "ability_0",
        "occurrence": {
            "owner": {"kind": "face_revision", "revision_id": "revision:fixture"},
            "field": "effect",
            "ordinal": None,
            "source_hash": "0" * 64,
            "line_ordinal": 0,
            "role": "body",
            "segments": [{"start": 0, "end": 1}],
        },
    }


def meta() -> dict[str, JsonValue]:
    return {
        "dsl": "1.0",
        "source": "effect-source-v1:sha256:" + "3" * 64,
        "written_by": [
            {"actor": {"who": "synthetic author", "model": None}, "date": "2026-10-21"}
        ],
        "reviewed_by": [],
        "status": "draft",
        "verified_by_exam": [],
        "rulings": [],
    }

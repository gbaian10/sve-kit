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


def dependencies() -> dict[str, JsonValue]:
    return {
        "sources": [source_use()],
        "frame_id": "frame:" + "1" * 64,
        "ability_scope": "ability_0",
        "semantic_variant": {"state": "resolved", "key": "fixture", "scope": None},
        "body_version": "effect-body-v1:sha256:" + "2" * 64,
        "interfaces": [],
        "applicability_scope": None,
        "domain_hash": None,
        "source": "effect-source-v1:sha256:" + "3" * 64,
        "rulings": [],
        "expansion_recipe": {"id": "fixture", "hash": "sha256:" + "4" * 64},
        "adapter_recipe": {"id": "fixture", "hash": "sha256:" + "5" * 64},
        "params": {},
        "imports": {},
    }


def review() -> dict[str, JsonValue]:
    return {
        "kind": "use",
        "reviewer": {"who": "synthetic reviewer", "model": None},
        "date": "2026-10-21",
        "verdict": "ok",
        "reason": None,
        "dependencies": dependencies(),
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

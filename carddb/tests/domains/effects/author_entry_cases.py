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
        "reviewed_by": [],
        "status": "draft",
        "verified_by_exam": [],
        "rulings": [],
    }


EVENT: dict[str, JsonValue] = {"phase": {"which": "end", "whose": "self"}}
TRIGGERS = (
    "fanfare",
    "lastword",
    "evolve_time",
    "super_evolve_time",
    "attack_time",
    "race_time",
    "drive_time",
)
HEADS: dict[str, JsonValue] = {
    **{key: {"do": []} for key in TRIGGERS},
    "keywords": ["quick", "ward"],
    "auto": {"on": EVENT, "do": []},
    "activate": {"cost": {"pp": 1}, "do": []},
    "spell": {"do": []},
    "evolve": {"cost": {"pp": 1}},
    "ride": {"cost": {"pp": 1}},
    "feed": {"cost": {"pp": 1}},
    "static": {"effects": [{"keywords": {"on": "this", "list": ["ward"]}}]},
    "play": {"cost_delta": -1},
}

"""Synthetic current digital links; no real relationships or review receipts."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import canonical, digest

from .translation_fixtures import reference

if TYPE_CHECKING:
    from pathlib import Path

CARD = "c:" + "1" * 32
FACE = "f:" + "2" * 32
PRINTING = "p:" + "3" * 32


def record() -> dict[str, JsonValue]:
    subject: dict[str, JsonValue] = {
        "card_id": CARD,
        "face_id": FACE,
        "game": "svwb",
        "official_id": "22345678",
        "digital_phase": "normal",
    }
    sve = reference(provider="jp", locator="/faces/0/name")
    digital = reference(
        provider="svwb", locator="/data/card_details/22345678/common/name"
    )
    return {
        "subject": subject,
        "value": {
            "relation": "same_card",
            "effect_similarity": None,
            "sve_names": [{"printing_id": PRINTING, "face_id": FACE, "name_ref": sve}],
            "digital_names": [{"phase": "normal", "lang": "ja", "name_ref": digital}],
        },
        "review_level": "confirmed",
        "reason": "Synthetic checked relation.",
    }


def envelope(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    return {
        "digital_link_authored_format": 2,
        "kind": "digital_link_shard",
        "records": list[JsonValue](records),
    }


def write(root: Path, shards: dict[str, dict[str, JsonValue]]) -> None:
    index: dict[str, JsonValue] = {
        "digital_link_authored_format": 2,
        "kind": "digital_link_index",
        "includes": {p: digest(canonical(v)) for p, v in shards.items()},
    }
    for name, value in {"digital-links/index.yaml": index, **shards}.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(canonical(value))

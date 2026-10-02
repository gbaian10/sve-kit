"""Synthetic digital adoption histories; no real relationships or review receipts."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical, digest, object_value

from .translation_fixtures import INSTANT, reference

if TYPE_CHECKING:
    from pathlib import Path

CARD = "c:" + "1" * 32
FACE = "f:" + "2" * 32
PRINTING = "p:" + "3" * 32


def review() -> dict[str, JsonValue]:
    return {
        "context": {
            "program_revision": "a" * 40,
            "dependencies": [{"name": "synthetic.py", "sha256": "sha256:" + "b" * 64}],
            "configuration": "{}",
        },
        "source_batches": [{"store_id": "synthetic", "batch_id": "sha256:" + "a" * 64}],
    }


def record(*, number: int = 1, previous: JsonValue = None) -> dict[str, JsonValue]:
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
        "record_key": canonical(["digital_link_adoption", subject, number]).decode(),
        "kind": "digital_link_adoption",
        "filing_key": "synthetic",
        "data": {
            "subject": subject,
            "adoption_no": number,
            "predecessor": previous,
            "value": {
                "relation": "same_card",
                "effect_similarity": None,
                "sve_names": [
                    {"printing_id": PRINTING, "face_id": FACE, "name_ref": sve}
                ],
                "digital_names": [
                    {"phase": "normal", "lang": "ja", "name_ref": digital}
                ],
            },
            "review_context_hash": digest(canonical(review())),
            "reason": "Synthetic sampled relation.",
        },
        "evidence": sorted(
            [
                {"source_ref": sve, "role": "sve_name"},
                {"source_ref": digital, "role": "digital_name"},
            ],
            key=canonical,
        ),
    }


def envelope(records: list[dict[str, JsonValue]]) -> dict[str, JsonValue]:
    records = sorted(records, key=lambda r: str(r["record_key"]))
    members: list[JsonValue] = [
        [r["record_key"], digest(canonical(r))] for r in records
    ]
    checksum = digest(canonical(members))
    identifier = "d:" + checksum[7:]
    return {
        "digital_link_authored_format": 1,
        "kind": "digital_link_shard",
        "review_context": review(),
        "default_decision_id": identifier,
        "records": list[JsonValue](records),
        "decisions": [
            {
                "id": identifier,
                "state": "sampled",
                "scope": "batch",
                "category": "digital_link",
                "policy_id": "digital-link-v1",
                "membership_hash": checksum,
                "members": members,
                "sample_ids": [records[0]["record_key"]],
                "authored_by": "Synthetic tool",
                "authored_at": INSTANT,
                "reviewed_by": "gbaian10",
                "reviewed_at": INSTANT,
                "reviewed_precision": "day",
                "note": "Synthetic actual sample.",
            }
        ],
    }


def write(root: Path, shards: dict[str, dict[str, JsonValue]]) -> None:
    index: dict[str, JsonValue] = {
        "digital_link_authored_format": 1,
        "kind": "digital_link_index",
        "includes": {p: digest(canonical(v)) for p, v in shards.items()},
    }
    for name, value in {"digital-links/index.yaml": index, **shards}.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(canonical(value))


def decision(shard: dict[str, JsonValue]) -> dict[str, JsonValue]:
    decisions = shard["decisions"]
    assert isinstance(decisions, list)
    return object_value(decisions[0])

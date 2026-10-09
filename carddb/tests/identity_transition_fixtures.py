"""Small independent wire oracles; no archive, card text or production inputs."""

import copy
import hashlib
import json
from itertools import starmap
from operator import itemgetter
from typing import TYPE_CHECKING, Any

import pytest

if TYPE_CHECKING:
    from pathlib import Path

A, B, C = ("c:" + digit * 32 for digit in "abc")
FA, FB, FC = ("f:" + digit * 32 for digit in "abc")
P, Q = ("p:" + digit * 32 for digit in "12")
X, Y = ("a:" + digit * 32 for digit in "12")


def wire(value: object) -> bytes:
    return json.dumps(
        value, sort_keys=True, ensure_ascii=False, separators=(",", ":")
    ).encode()


def record_key(value: dict[str, Any]) -> str:
    if value.get("kind") == "identity_transition":
        return wire([value["kind"], value["sequence"]]).decode()
    if "kind" in value and "data" in value:
        field = (
            "card_id"
            if value["kind"] == "region_mapping_review"
            else "printing_id"
            if value["kind"] == "card_int_id"
            else "id"
        )
        return str(value["kind"]) + ":" + str(value["data"][field])
    return str(value["record_key"])


def semantic(value: object) -> object:
    if isinstance(value, list):
        return [semantic(item) for item in value]
    if isinstance(value, dict):
        result = {key: semantic(item) for key, item in value.items()}
        if "kind" in value and (
            "data" in value or value["kind"] == "identity_transition"
        ):
            result["record_key"] = record_key(value)
        return result
    return value


def checksum(value: object) -> str:
    return (
        "sha256:"
        + hashlib.sha256(
            wire(
                value
                if isinstance(value, dict)
                and value.get("kind") == "identity_transition_shard"
                else semantic(value)
            )
        ).hexdigest()
    )


def reference(shard: dict[str, Any]) -> dict[str, Any]:
    record = shard["records"][0]
    return {"record_key": record_key(record), "record_hash": checksum(record)}


def pack(record: dict[str, Any]) -> dict[str, Any]:
    return {
        "identity_transition_format": 1,
        "kind": "identity_transition_shard",
        "records": [record],
    }


def chain(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    latest: dict[str, dict[str, Any]] = {}
    for sequence, original in enumerate(records, start=1):
        record = copy.deepcopy(original)
        record["sequence"] = sequence

        record["previous"] = reference(result[-1]) if result else None
        for update in record["updates"]:
            if update["target_key"] in latest:
                update["before"] = latest[update["target_key"]]
        shard = pack(record)
        result.append(shard)
        for update in record["updates"]:
            latest[update["target_key"]] = {
                "transition_key": record_key(record),
                "record_key": update["target_key"],
                "record_hash": checksum(update["after"]),
            }
    return result


def write_chain(root: Path, shards: list[dict[str, Any]]) -> None:
    (root / "identity-transitions").mkdir(exist_ok=True)
    for sequence, shard in enumerate(shards, start=1):
        (root / f"identity-transitions/{sequence:03}.yaml").write_bytes(wire(shard))


def entry(kind: str, identifier: str, data: dict[str, Any]) -> dict[str, Any]:
    return {
        "kind": kind,
        "owner": "EXAMPLE",
        "data": {"id": identifier, **data},
    }


def update(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    return {
        "target_key": record_key(before),
        "before": {
            "transition_key": None,
            "record_key": record_key(before),
            "record_hash": checksum(before),
        },
        "after": after,
        "allocation_anchor": None,
    }


@pytest.fixture(scope="module")
def merge_record() -> dict[str, Any]:
    observation = {
        "region": "jp",
        "card_no": "EXAMPLE-001",
        "recipe": "registry-observation-v2",
        "observation_hash": "sha256:" + "1" * 64,
        "rules_hash": "sha256:" + "2" * 64,
    }
    card = entry(
        "card",
        A,
        {"layout": "single", "identity_state": "confirmed", "home_set_id": "EXAMPLE"},
    )
    retired = copy.deepcopy(card)
    retired["data"]["identity_state"] = "retired"
    printing = entry(
        "printing",
        P,
        {
            "card_id": A,
            "region": "jp",
            "card_no": "EXAMPLE-001",
            "variant_key": "standard",
            "home_set_id": "EXAMPLE",
            "source_face_map": [{"source_index": 0, "face_id": FA}],
            "observation": observation,
        },
    )
    moved = copy.deepcopy(printing)
    moved["data"]["card_id"] = B
    moved["data"]["source_face_map"][0]["face_id"] = FB
    art = entry(
        "art",
        X,
        {
            "card_id": A,
            "face_id": FA,
            "classification": "base",
            "uses": [{"printing_id": P, "face_id": FA}],
            "observation": observation,
        },
    )
    unused = copy.deepcopy(art)
    unused["data"]["uses"] = []
    destination = entry(
        "art",
        Y,
        {
            "card_id": B,
            "face_id": FB,
            "classification": "base",
            "uses": [],
            "observation": observation,
        },
    )
    used = copy.deepcopy(destination)
    used["data"]["uses"] = [{"printing_id": P, "face_id": FB}]
    updates = list(
        starmap(
            update,
            ((card, retired), (printing, moved), (art, unused), (destination, used)),
        )
    )
    basis = {
        "authored_revision": "0" * 40,
        "index_path": "ids/index.yaml",
        "index_hash": "sha256:" + "3" * 64,
    }
    batch = {"batch_id": "sha256:" + "4" * 64}
    return {
        "kind": "identity_transition",
        "action": "apply",
        "reverts": None,
        "sequence": 1,
        "previous": None,
        "registry_basis": basis,
        "review_context": {
            "context": {
                "program_revision": "5" * 40,
                "configuration": wire({"registry": basis}).decode(),
            },
            "source_batches": [batch],
        },
        "updates": sorted(updates, key=itemgetter("target_key")),
        "repairs": [
            {
                "id": "repair:synthetic-merge",
                "kind": "merge",
                "old_card_id": A,
                "new_card_ids": [B],
                "retire_old": True,
                "reason": "Synthetic merge",
                "printing_moves": [
                    {
                        "printing_id": P,
                        "from_card_id": A,
                        "to_card_id": B,
                        "faces": [
                            {
                                "source_index": 0,
                                "from_face_id": FA,
                                "to_face_id": FB,
                                "from_art_id": X,
                                "to_art_id": Y,
                            }
                        ],
                    }
                ],
                "face_moves": [{"from_face_id": FA, "to_face_ids": [FB]}],
                "art_moves": [
                    {
                        "from_art_id": X,
                        "targets": [
                            {
                                "to_art_id": Y,
                                "to_face_id": FB,
                                "uses": [{"printing_id": P, "face_id": FB}],
                            }
                        ],
                        "remaining_uses": [],
                    }
                ],
            }
        ],
        "routes": [],
        "evidence": [
            {
                **batch,
                "source_version_id": "src:v1:" + "7" * 64,
                "locator": "synthetic face 0",
                "role": "identity_observation",
            }
        ],
        "reason": "Synthetic merge only; no production adoption",
    }


def renewal(record: dict[str, Any]) -> dict[str, Any]:
    result = copy.deepcopy(record)
    result["repairs"] = []
    result["reason"] = "Synthetic reviewed renewal"
    return result

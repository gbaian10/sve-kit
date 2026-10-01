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


def checksum(value: object) -> str:
    return "sha256:" + hashlib.sha256(wire(value)).hexdigest()


def reference(shard: dict[str, Any]) -> dict[str, Any]:
    record = shard["records"][0]
    return {
        "record_key": record["record_key"],
        "record_hash": checksum(record),
        "decision_id": shard["default_decision_id"],
    }


def pack(record: dict[str, Any]) -> dict[str, Any]:
    members = [[record["record_key"], checksum(record)]]
    membership = checksum(members)
    decision = {
        "id": "d:" + membership.removeprefix("sha256:"),
        "state": "confirmed",
        "scope": "batch",
        "category": "identity_transition",
        "policy_id": "identity-transition-v1",
        "membership_hash": membership,
        "members": members,
        "sample_ids": [record["record_key"]],
        "authored_by": "synthetic-author",
        "authored_at": "2026-10-01T00:00:00Z",
        "reviewed_by": "synthetic-reviewer",
        "reviewed_at": "2026-10-01T00:00:00Z",
        "reviewed_precision": "day",
        "note": "Synthetic receipt only",
    }
    return {
        "identity_transition_format": 1,
        "kind": "identity_transition_shard",
        "default_decision_id": decision["id"],
        "records": [record],
        "decisions": [decision],
    }


def chain(records: list[dict[str, Any]]) -> list[dict[str, Any]]:
    result: list[dict[str, Any]] = []
    latest: dict[str, dict[str, Any]] = {}
    for sequence, original in enumerate(records, start=1):
        record = copy.deepcopy(original)
        record["sequence"] = sequence
        record["record_key"] = wire(["identity_transition", sequence]).decode()
        record["previous"] = reference(result[-1]) if result else None
        for update in record["updates"]:
            if update["target_key"] in latest:
                update["before"] = latest[update["target_key"]]
        shard = pack(record)
        result.append(shard)
        for update in record["updates"]:
            latest[update["target_key"]] = {
                "transition_key": record["record_key"],
                "record_key": update["target_key"],
                "record_hash": checksum(update["after"]),
                "decision_id": shard["default_decision_id"],
            }
    return result


def write_chain(root: Path, shards: list[dict[str, Any]]) -> None:
    directory = root / "identity-transitions"
    directory.mkdir(exist_ok=True)
    includes = {}
    for sequence, shard in enumerate(shards, start=1):
        name = f"identity-transitions/{sequence:03}.yaml"
        (root / name).write_bytes(wire(shard))
        includes[name] = checksum(shard)
    (directory / "index.yaml").write_bytes(
        wire(
            {
                "identity_transition_format": 1,
                "kind": "identity_transition_index",
                "includes": includes,
            }
        )
    )


def entry(kind: str, identifier: str, data: dict[str, Any]) -> dict[str, Any]:
    return {
        "record_key": kind + ":" + identifier,
        "kind": kind,
        "owner": "EXAMPLE",
        "data": {"id": identifier, **data},
    }


def update(before: dict[str, Any], after: dict[str, Any]) -> dict[str, Any]:
    return {
        "target_key": before["record_key"],
        "before": {
            "transition_key": None,
            "record_key": before["record_key"],
            "record_hash": checksum(before),
            "decision_id": "d:" + "0" * 64,
        },
        "after": after,
        "allocation_anchor": None,
    }


@pytest.fixture(scope="module")
def merge_record() -> dict[str, Any]:
    observation = {
        "region": "jp",
        "card_no": "EXAMPLE-001",
        "recipe": "registry-observation-v1",
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
    batch = {"store_id": "example", "batch_id": "sha256:" + "4" * 64}
    return {
        "record_key": '["identity_transition",1]',
        "kind": "identity_transition",
        "action": "apply",
        "reverts": None,
        "sequence": 1,
        "previous": None,
        "registry_basis": basis,
        "review_context": {
            "context": {
                "program_revision": "5" * 40,
                "dependencies": [{"name": "uv.lock", "sha256": "sha256:" + "6" * 64}],
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

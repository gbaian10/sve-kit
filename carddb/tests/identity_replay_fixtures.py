"""Tiny independent synthetic registry/replay inputs; no official sources or data."""

import copy
import operator
from dataclasses import dataclass
from typing import TYPE_CHECKING, Any
from uuid import UUID, uuid5

from sve_carddb.registry.inputs import canonical
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.storage import Entry, plan_files, read_base_files, write_files
from sve_carddb.registry.transitions.routing import RouteFact

from .identity_transition_fixtures import (
    FA,
    FB,
    FC,
    A,
    B,
    C,
    P,
    Q,
    X,
    Y,
    chain,
    checksum,
    entry,
    wire,
    write_chain,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.storage import RegistryFiles
    from sve_carddb.registry.transitions.models import RegistryBasis, Transition

R = "p:" + "3" * 32
S = "p:" + "4" * 32
Z = "a:" + "3" * 32
REVISION = "2" * 40
SOURCE = "src:v1:" + "7" * 64


@dataclass(frozen=True)
class MemoryInputs:
    bases: dict[str, RegistryFiles]
    frames: dict[int, tuple[RouteFact, ...]]

    def registry(self, basis: RegistryBasis) -> RegistryFiles:
        return self.bases[basis.authored_revision]

    def routes(self, transition: Transition | None) -> tuple[RouteFact, ...]:
        return self.frames[0 if transition is None else transition.sequence]


def allocated(kind: str, anchor: str) -> str:
    return (
        kind
        + ":"
        + uuid5(
            UUID("e304714a-f18c-5fb6-a987-222988ffbb7a"),
            kind + "\0identity-transition-v1\0" + anchor,
        ).hex
    )


def observation(number: str) -> dict[str, Any]:
    return {
        "region": "en",
        "card_no": number,
        "recipe": "registry-observation-v1",
        "observation_hash": "sha256:" + "8" * 64,
        "rules_hash": "sha256:" + "9" * 64,
    }


def printing(pid: str, cid: str, fid: str, number: str) -> dict[str, Any]:
    return entry(
        "printing",
        pid,
        {
            "card_id": cid,
            "region": "en",
            "card_no": number,
            "variant_key": "base",
            "home_set_id": "EXAMPLE",
            "source_face_map": [{"source_index": 0, "face_id": fid}],
            "observation": observation(number),
            "cross_region_review": {
                "checked": True,
                "target_jp_card_no": None,
                "target_observation": None,
            },
        },
    )


def review(cid: str, printings: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "record_key": "region_mapping_review:" + cid,
        "kind": "region_mapping_review",
        "owner": "EXAMPLE",
        "data": {
            "card_id": cid,
            "target_region": "jp",
            "state": "confirmed_none",
            "as_of": "2026-10-01",
            "coverage_scope": "synthetic complete JP",
            "coverage_hash": "sha256:" + "6" * 64,
            "observations": sorted(
                [
                    p["data"]["observation"]
                    for p in printings
                    if p["data"]["card_id"] == cid
                ],
                key=lambda x: str(x["card_no"]),
            ),
        },
    }


def seed() -> list[dict[str, Any]]:
    rows = []
    for cid, fid in ((A, FA), (B, FB), (C, FC)):
        rows.extend(
            [
                entry(
                    "card",
                    cid,
                    {
                        "layout": "single",
                        "identity_state": "confirmed",
                        "home_set_id": "EXAMPLE",
                    },
                ),
                entry("face", fid, {"card_id": cid, "ordinal": 0, "side": "front"}),
            ]
        )
    printings = [
        printing(P, A, FA, "TEST-01EN"),
        printing(Q, A, FA, "TEST-02EN"),
        printing(R, B, FB, "TEST-03EN"),
        printing(S, C, FC, "TEST-04EN"),
    ]
    rows.extend(printings)
    for offset, row in enumerate(printings):
        pid = row["data"]["id"]
        rows.append(
            {
                "record_key": "card_int_id:" + pid,
                "kind": "card_int_id",
                "owner": "EXAMPLE",
                "data": {
                    "int_id": 60001 + offset,
                    "printing_id": pid,
                },
            }
        )
    rows.extend(review(cid, printings) for cid in (A, B, C))
    rows.extend(
        [
            entry(
                "art",
                X,
                {
                    "card_id": A,
                    "face_id": FA,
                    "classification": "unclassified",
                    "uses": [
                        {"printing_id": P, "face_id": FA},
                        {"printing_id": Q, "face_id": FA},
                    ],
                    "observation": observation("TEST-01EN"),
                },
            ),
            entry(
                "art",
                Y,
                {
                    "card_id": B,
                    "face_id": FB,
                    "classification": "unclassified",
                    "uses": [{"printing_id": R, "face_id": FB}],
                    "observation": observation("TEST-03EN"),
                },
            ),
        ]
    )
    rows.append(
        entry(
            "art",
            Z,
            {
                "card_id": C,
                "face_id": FC,
                "classification": "unclassified",
                "uses": [{"printing_id": S, "face_id": FC}],
                "observation": observation("TEST-04EN"),
            },
        )
    )
    return rows


def basis(root: Path, rows: list[dict[str, Any]]) -> RegistryFiles:
    entries = [Entry.model_validate(row) for row in rows]
    write_files(plan_files(root, entries, "synthetic-reviewer", "2026-10-01"))
    return read_base_files(root)


def transaction(
    template: dict[str, Any], files: RegistryFiles, *, kind: str = "merge"
) -> dict[str, Any]:
    records = {
        e.record_key: e.model_dump(mode="json")
        for s in files.shards
        for e in s.envelope().records
    }
    decisions = {
        e.record_key: s.envelope().default_decision_id
        for s in files.shards
        for e in s.envelope().records
    }
    result = copy.deepcopy(template)
    result["registry_basis"] = {
        "authored_revision": REVISION,
        "index_path": "ids/index.yaml",
        "index_hash": checksum(files.index().model_dump(mode="json")),
    }
    destinations = (
        {P: B, Q: C}
        if kind == "split"
        else {P: B}
        if kind == "reassign_printing"
        else {P: B, Q: B}
    )
    face_for = {B: FB, C: FC}
    art_for = {B: Y, C: Z}
    after: dict[str, Any] = copy.deepcopy(records)
    moves: list[dict[str, Any]] = []
    for pid, cid in destinations.items():
        after["printing:" + pid]["data"]["card_id"] = cid
        after["printing:" + pid]["data"]["source_face_map"][0]["face_id"] = face_for[
            cid
        ]
        moves.append(
            {
                "printing_id": pid,
                "from_card_id": A,
                "to_card_id": cid,
                "faces": [
                    {
                        "source_index": 0,
                        "from_face_id": FA,
                        "to_face_id": face_for[cid],
                        "from_art_id": X,
                        "to_art_id": art_for[cid],
                    }
                ],
            }
        )
    retired = kind != "reassign_printing"
    if retired:
        after["card:" + A]["data"]["identity_state"] = "retired"
        after["region_mapping_review:" + A] = None
    else:
        after["region_mapping_review:" + A] = review(
            A, [p for p in after.values() if p is not None and p["kind"] == "printing"]
        )
    uses = [
        {"printing_id": pid, "face_id": FA} for pid in (P, Q) if pid not in destinations
    ]
    after["art:" + X]["data"]["uses"] = uses
    if uses:
        after["art:" + X]["data"]["observation"] = records[
            "printing:" + uses[0]["printing_id"]
        ]["data"]["observation"]
    targets = []
    for cid in sorted(set(destinations.values())):
        aid = art_for[cid]
        new_uses = [
            {"printing_id": pid, "face_id": face_for[cid]}
            for pid, dest in destinations.items()
            if dest == cid
        ]
        if "art:" + aid not in after:
            # C's art is independently allocated, not derived from its current rank.
            aid = allocated("a", "synthetic-c-art")
            art_for[cid] = aid
            for move in moves:
                if move["to_card_id"] == cid:
                    move["faces"][0]["to_art_id"] = aid
            after["art:" + aid] = entry(
                "art",
                aid,
                {
                    "card_id": cid,
                    "face_id": face_for[cid],
                    "classification": "unclassified",
                    "uses": [],
                    "observation": records["printing:" + new_uses[0]["printing_id"]][
                        "data"
                    ]["observation"],
                },
            )
        after["art:" + aid]["data"]["uses"] = sorted(
            after["art:" + aid]["data"]["uses"] + new_uses,
            key=operator.itemgetter("printing_id"),
        )
        targets.append(
            {"to_art_id": aid, "to_face_id": face_for[cid], "uses": new_uses}
        )
        after["region_mapping_review:" + cid] = review(
            cid,
            [p for p in after.values() if p is not None and p["kind"] == "printing"],
        )
    updates = []
    for key, value in after.items():
        if value == records.get(key):
            continue
        old = records.get(key)
        updates.append(
            {
                "target_key": key,
                "before": None
                if old is None
                else {
                    "transition_key": None,
                    "record_key": key,
                    "record_hash": checksum(old),
                    "decision_id": decisions[key],
                },
                "after": value,
                "allocation_anchor": "synthetic-c-art" if old is None else None,
            }
        )
    # Reviews cannot be first allocated in transitions; register C's review in the base for split.
    result["updates"] = sorted(updates, key=operator.itemgetter("target_key"))
    result["repairs"] = [
        {
            "id": "synthetic-repair-1",
            "kind": kind,
            "old_card_id": A,
            "new_card_ids": sorted(set(destinations.values())),
            "retire_old": retired,
            "reason": "Synthetic repair",
            "printing_moves": sorted(moves, key=checksum_order),
            "face_moves": [
                {
                    "from_face_id": FA,
                    "to_face_ids": sorted(
                        {face_for[cid] for cid in destinations.values()}
                    ),
                }
            ],
            "art_moves": [
                {
                    "from_art_id": X,
                    "targets": sorted(targets, key=checksum_order),
                    "remaining_uses": uses,
                }
            ],
        }
    ]
    result["routes"] = []
    return result


def checksum_order(value: object) -> bytes:
    return wire(value)


def frames(files: RegistryFiles, count: int) -> dict[int, tuple[RouteFact, ...]]:
    printings = [
        PrintingData.model_validate_json(
            canonical({k: v for k, v in e.data.items() if k != "cross_region_review"})
        )
        for s in files.shards
        for e in s.envelope().records
        if e.kind == "printing"
    ]
    facts = tuple(
        RouteFact(p.id, p.region, "official", p.card_no, SOURCE) for p in printings
    )
    return dict.fromkeys(range(count + 1), facts)


def write_scenario(
    root: Path, files: RegistryFiles, records: list[dict[str, Any]]
) -> MemoryInputs:
    write_chain(root, chain(records))
    return MemoryInputs({REVISION: files}, frames(files, len(records)))


def subsequent(
    template: dict[str, Any],
    state: dict[str, dict[str, Any]],
    pid: str,
    old_card: str,
    new_card: str,
) -> dict[str, Any]:
    result = copy.deepcopy(template)
    after: dict[str, Any] = copy.deepcopy(state)
    source_face = state["printing:" + pid]["data"]["source_face_map"][0]["face_id"]
    target_face = next(
        e["data"]["id"]
        for e in state.values()
        if e["kind"] == "face" and e["data"]["card_id"] == new_card
    )
    old_art = next(
        e["data"]["id"]
        for e in state.values()
        if e["kind"] == "art"
        and any(u["printing_id"] == pid for u in e["data"]["uses"])
    )
    target_art = next(
        e["data"]["id"]
        for e in state.values()
        if e["kind"] == "art" and e["data"]["face_id"] == target_face
    )
    after["printing:" + pid]["data"]["card_id"] = new_card
    after["printing:" + pid]["data"]["source_face_map"][0]["face_id"] = target_face
    remaining = [
        e
        for e in after.values()
        if e["kind"] == "printing" and e["data"]["card_id"] == old_card
    ]
    retire = not remaining
    if retire:
        after["card:" + old_card]["data"]["identity_state"] = "retired"
        after["region_mapping_review:" + old_card] = None
    printings = [e for e in after.values() if e is not None and e["kind"] == "printing"]
    after["region_mapping_review:" + new_card] = review(new_card, printings)
    if not retire:
        after["region_mapping_review:" + old_card] = review(old_card, printings)
    transfers = []
    for row in state.values():
        if row["kind"] != "art" or row["data"]["card_id"] != old_card:
            continue
        aid = row["data"]["id"]
        uses = [u for u in row["data"]["uses"] if u["printing_id"] != pid]
        targets = []
        if aid == old_art:
            targets = [
                {
                    "to_art_id": target_art,
                    "to_face_id": target_face,
                    "uses": [{"printing_id": pid, "face_id": target_face}],
                }
            ]
            after["art:" + target_art]["data"]["uses"].append(
                {"printing_id": pid, "face_id": target_face}
            )
            after["art:" + target_art]["data"]["uses"].sort(
                key=operator.itemgetter("printing_id")
            )
            after["art:" + aid]["data"]["uses"] = uses
            if uses:
                after["art:" + aid]["data"]["observation"] = state[
                    "printing:" + uses[0]["printing_id"]
                ]["data"]["observation"]
        transfers.append(
            {"from_art_id": aid, "targets": targets, "remaining_uses": uses}
        )
    decisions = {
        u["target_key"]: u["before"]["decision_id"]
        for u in template["updates"]
        if u["before"] is not None
    }
    changes = []
    for key, value in after.items():
        if state.get(key) != value:
            changes.append(
                {
                    "target_key": key,
                    "before": {
                        "transition_key": None,
                        "record_key": key,
                        "record_hash": checksum(state[key]),
                        "decision_id": decisions.get(key, "d:" + "0" * 64),
                    },
                    "after": value,
                    "allocation_anchor": None,
                }
            )
    result["updates"] = sorted(changes, key=operator.itemgetter("target_key"))
    result["repairs"] = [
        {
            "id": "synthetic-repair-2",
            "kind": "reassign_printing",
            "old_card_id": old_card,
            "new_card_ids": [new_card],
            "retire_old": retire,
            "reason": "Synthetic subsequent reassign",
            "printing_moves": [
                {
                    "printing_id": pid,
                    "from_card_id": old_card,
                    "to_card_id": new_card,
                    "faces": [
                        {
                            "source_index": 0,
                            "from_face_id": source_face,
                            "to_face_id": target_face,
                            "from_art_id": old_art,
                            "to_art_id": target_art,
                        }
                    ],
                }
            ],
            "face_moves": [{"from_face_id": source_face, "to_face_ids": [target_face]}],
            "art_moves": sorted(transfers, key=checksum_order),
        }
    ]
    result["routes"] = []
    return result

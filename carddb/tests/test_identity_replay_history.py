"""Sequential repair, allocations and inverse-boundary counterexamples."""

import copy
import operator
from typing import TYPE_CHECKING, Any

import pytest

from sve_carddb.registry.transitions.replay import replay

from .identity_replay_fixtures import (
    allocated,
    basis,
    seed,
    subsequent,
    transaction,
    write_scenario,
)
from .identity_transition_fixtures import (
    FA,
    FB,
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
    reference,
    wire,
)
from .identity_transition_fixtures import merge_record as merge_record  # ruff: ignore[useless-import-alias] -- expose the shared synthetic receipt fixture
from .test_identity_replay import copy_base, required
from .test_identity_replay import replay_base as replay_base  # ruff: ignore[useless-import-alias] -- reuse module synthetic registry template
from .test_identity_replay import scenario as scenario  # ruff: ignore[useless-import-alias] -- reuse a copied immutable base

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.storage import RegistryFiles


def effective_state(
    files: RegistryFiles, first: dict[str, Any]
) -> dict[str, dict[str, Any]]:
    state = {
        e.record_key: e.model_dump(mode="json", round_trip=True)
        for s in files.shards
        for e in s.envelope().records
    }
    for update in first["updates"]:
        if update["after"] is None:
            del state[update["target_key"]]
        else:
            state[update["target_key"]] = copy.deepcopy(update["after"])
    return state


def fix_root_refs(second: dict[str, Any], files: RegistryFiles) -> None:
    original = {
        e.record_key: e.model_dump(mode="json", round_trip=True)
        for s in files.shards
        for e in s.envelope().records
    }
    for update in second["updates"]:
        update["before"]["record_hash"] = checksum(original[update["target_key"]])


def test_continuous_reassign_preserves_refs_and_history(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]], merge_record: dict[str, Any]
) -> None:
    root, files, _ = scenario
    first = transaction(merge_record, files, kind="reassign_printing")
    second = subsequent(first, effective_state(files, first), Q, A, C)
    fix_root_refs(second, files)
    result = replay(root, write_scenario(root, files, [first, second]))
    assert required(result, "printing:" + P).data["card_id"] == B
    assert required(result, "printing:" + Q).data["card_id"] == C
    assert required(result, "art:" + X).data["uses"] == []
    assert required(result, "card:" + A).data["identity_state"] == "retired"
    assert result.records["region_mapping_review:" + A].entry() is None
    ref = result.records["printing:" + Q].reference
    assert ref is not None
    assert ref.transition_key == '["identity_transition",2]'


def test_cycle_rejected_after_two_valid_parent_changes(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]], merge_record: dict[str, Any]
) -> None:
    root, files, _ = scenario
    first = transaction(merge_record, files, kind="reassign_printing")
    second = subsequent(first, effective_state(files, first), P, B, A)
    fix_root_refs(second, files)
    with pytest.raises(ValueError, match="graph contains a cycle"):
        replay(root, write_scenario(root, files, [first, second]))


def test_new_art_allocation_is_explicit_and_stable(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    aid = allocated("a", "new-confirmed-art")
    target = next(u for u in record["updates"] if u["target_key"] == "art:" + Y)
    # Keep the original B art's R use and allocate a separate reviewed art for P/Q.
    target["target_key"] = "art:" + aid
    target["before"] = None
    target["allocation_anchor"] = "new-confirmed-art"

    target["after"]["data"]["id"] = aid
    target["after"]["data"]["uses"] = [
        u for u in target["after"]["data"]["uses"] if u["printing_id"] in {P, Q}
    ]
    source = next(
        e
        for s in files.shards
        for e in s.envelope().records
        if e.record_key == "printing:" + P
    )
    target["after"]["data"]["observation"] = source.data["observation"]
    for move in record["repairs"][0]["printing_moves"]:
        move["faces"][0]["to_art_id"] = aid
    record["repairs"][0]["art_moves"][0]["targets"][0]["to_art_id"] = aid
    record["updates"].sort(key=operator.itemgetter("target_key"))
    inputs = write_scenario(root, files, [record])
    result = replay(root, inputs)
    assert required(result, "art:" + aid).data["uses"] == [
        {"printing_id": P, "face_id": FB},
        {"printing_id": Q, "face_id": FB},
    ]
    assert required(result, "art:" + Y).data["uses"] == [
        {"printing_id": "p:" + "3" * 32, "face_id": FB}
    ]
    assert replay(root, inputs) == result


@pytest.fixture(scope="module")
def double_base(tmp_path_factory: pytest.TempPathFactory) -> RegistryFiles:
    rows = seed()
    for row in rows:
        if row["kind"] == "card":
            row["data"]["layout"] = "double_faced"
        elif row["kind"] == "printing":
            front = row["data"]["source_face_map"][0]["face_id"]
            row["data"]["source_face_map"].append(
                {"source_index": 1, "face_id": front[:-1] + "0"}
            )
    for cid, fid in ((A, FA), (B, FB), (C, "f:" + "c" * 32)):
        rows.append(
            {
                "kind": "face",
                "owner": "EXAMPLE",
                "data": {
                    "id": fid[:-1] + "0",
                    "card_id": cid,
                    "ordinal": 1,
                    "side": "back",
                },
            }
        )
    return basis(tmp_path_factory.mktemp("identity-double-base"), rows)


@pytest.mark.parametrize("damage", [None, "omit_back", "unknown_to_known"])
def test_double_face_coverage_and_unknown_art(
    tmp_path: Path,
    double_base: RegistryFiles,
    merge_record: dict[str, Any],
    damage: str | None,
) -> None:

    copy_base(double_base, tmp_path)
    record = transaction(merge_record, double_base, kind="reassign_printing")
    back_source, back_target = FA[:-1] + "0", FB[:-1] + "0"
    printing_after = next(
        u["after"] for u in record["updates"] if u["target_key"] == "printing:" + P
    )
    printing_after["data"]["source_face_map"][1]["face_id"] = back_target
    repair = record["repairs"][0]
    repair["face_moves"].append(
        {"from_face_id": back_source, "to_face_ids": [back_target]}
    )
    repair["face_moves"].sort(key=wire)
    if damage != "omit_back":
        repair["printing_moves"][0]["faces"].append(
            {
                "source_index": 1,
                "from_face_id": back_source,
                "to_face_id": back_target,
                "from_art_id": None,
                "to_art_id": None if damage is None else Y,
            }
        )
    inputs = write_scenario(tmp_path, double_base, [record])
    if damage is None:
        result = replay(tmp_path, inputs)
        assert required(result, "printing:" + P).data["source_face_map"] == [
            {"source_index": 0, "face_id": FB},
            {"source_index": 1, "face_id": back_target},
        ]
        assert required(result, "printing:" + Q).data["source_face_map"] == [
            {"source_index": 0, "face_id": FA},
            {"source_index": 1, "face_id": back_source},
        ]
    else:
        with pytest.raises(
            ValueError,
            match={
                "omit_back": "^Printing move must cover exact complete source face maps$",
                "unknown_to_known": "^Unknown art cannot become known through an identity move$",
            }[damage],
        ):
            replay(tmp_path, inputs)


def test_retired_card_cannot_be_restored_by_apply(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, first = scenario
    second = copy.deepcopy(first)
    old = next(
        e.model_dump(mode="json", round_trip=True)
        for s in files.shards
        for e in s.envelope().records
        if e.record_key == "card:" + A
    )
    second["repairs"] = []
    second["updates"] = [
        copy.deepcopy(
            next(u for u in first["updates"] if u["target_key"] == "card:" + A)
        )
    ]
    second["updates"][0]["after"] = old
    with pytest.raises(ValueError, match="Only named revert"):
        replay(root, write_scenario(root, files, [first, second]))


def test_inactive_review_cannot_change_permanent_owner(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, first = scenario
    second = copy.deepcopy(first)
    raw = next(
        e.model_dump(mode="json", round_trip=True)
        for s in files.shards
        for e in s.envelope().records
        if e.record_key == "region_mapping_review:" + A
    )
    raw["owner"] = "OTHER"
    second["repairs"] = []
    update = copy.deepcopy(
        next(
            u
            for u in first["updates"]
            if u["target_key"] == "region_mapping_review:" + A
        )
    )
    update["after"] = raw
    second["updates"] = [update]
    with pytest.raises(ValueError, match="stable record key, kind or owner"):
        replay(root, write_scenario(root, files, [first, second]))


def test_structurally_valid_named_revert_waits_for_stage_c(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, first = scenario
    second = copy.deepcopy(first)
    second["action"] = "revert"
    second["repairs"] = []
    second["reverts"] = reference(chain([first])[0])
    with pytest.raises(ValueError, match="requires stage C"):
        replay(root, write_scenario(root, files, [first, second]))


@pytest.fixture(scope="module")
def jp_base(tmp_path_factory: pytest.TempPathFactory) -> RegistryFiles:
    rows = [e for e in seed() if e["kind"] not in {"art", "region_mapping_review"}]
    for row in rows:
        if row["kind"] == "printing":
            row["data"]["region"] = "jp"
            row["data"]["observation"]["region"] = "jp"
            del row["data"]["cross_region_review"]
        elif row["kind"] == "card_int_id":
            row["data"]["int_id"] -= 40000
    return basis(tmp_path_factory.mktemp("identity-new-entities-base"), rows)


def jp_split(template: dict[str, Any], files: RegistryFiles) -> dict[str, Any]:
    record = copy.deepcopy(template)
    record["registry_basis"]["authored_revision"] = "2" * 40
    record["registry_basis"]["index_hash"] = checksum(
        files.index().model_dump(mode="json", round_trip=True)
    )
    before = {
        e.record_key: e.model_dump(mode="json", round_trip=True)
        for s in files.shards
        for e in s.envelope().records
    }
    after = copy.deepcopy(before)
    anchors = {}
    destinations = []
    moves: list[dict[str, Any]] = []
    for pid, suffix in ((P, "d"), (Q, "e")):
        cid, fid = allocated("c", suffix), allocated("f", suffix + "-face")
        anchors["card:" + cid] = suffix
        anchors["face:" + fid] = suffix + "-face"
        destinations.append(cid)
        after["card:" + cid] = {
            "kind": "card",
            "owner": "EXAMPLE",
            "data": {
                "id": cid,
                "layout": "single",
                "identity_state": "confirmed",
                "home_set_id": "EXAMPLE",
            },
        }
        after["face:" + fid] = {
            "kind": "face",
            "owner": "EXAMPLE",
            "data": {"id": fid, "card_id": cid, "ordinal": 0, "side": "front"},
        }
        after["printing:" + pid]["data"]["card_id"] = cid
        after["printing:" + pid]["data"]["source_face_map"][0]["face_id"] = fid
        moves.append(
            {
                "printing_id": pid,
                "from_card_id": A,
                "to_card_id": cid,
                "faces": [
                    {
                        "source_index": 0,
                        "from_face_id": FA,
                        "to_face_id": fid,
                        "from_art_id": None,
                        "to_art_id": None,
                    }
                ],
            }
        )
    after["card:" + A]["data"]["identity_state"] = "retired"
    updates = []
    for key, raw in after.items():
        if before.get(key) != raw:
            updates.append(
                {
                    "target_key": key,
                    "before": None
                    if key not in before
                    else {
                        "transition_key": None,
                        "record_key": key,
                        "record_hash": checksum(before[key]),
                    },
                    "after": raw,
                    "allocation_anchor": anchors.get(key),
                }
            )
    record["updates"] = sorted(updates, key=operator.itemgetter("target_key"))
    record["repairs"] = [
        {
            "id": "split-new-entities",
            "kind": "split",
            "old_card_id": A,
            "new_card_ids": sorted(destinations),
            "printing_moves": sorted(moves, key=wire),
            "face_moves": [
                {
                    "from_face_id": FA,
                    "to_face_ids": sorted(m["faces"][0]["to_face_id"] for m in moves),
                }
            ],
            "art_moves": [],
            "retire_old": True,
            "reason": "Synthetic new destinations",
        }
    ]
    record["routes"] = []
    return record


def test_split_allocates_new_cards_and_faces_without_reusing_printing(
    tmp_path: Path, jp_base: RegistryFiles, merge_record: dict[str, Any]
) -> None:
    copy_base(jp_base, tmp_path)
    record = jp_split(merge_record, jp_base)
    inputs = write_scenario(tmp_path, jp_base, [record])
    assert all(f.region == "jp" for facts in inputs.frames.values() for f in facts)
    result = replay(tmp_path, inputs)
    hint = result.resolve_int_id(20001)
    assert hint is not None
    assert hint.status == "choice_required"
    assert hint.printing_id == P
    assert hint.choices == tuple(sorted((allocated("c", "d"), allocated("c", "e"))))
    assert required(result, "printing:" + P).data["card_id"] == allocated("c", "d")
    assert required(result, "printing:" + Q).data["card_id"] == allocated("c", "e")
    assert required(result, "face:" + allocated("f", "d-face")).data[
        "card_id"
    ] == allocated("c", "d")


def test_pure_renewal_cannot_allocate_unrelated_entities(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    record["repairs"] = []
    cid = allocated("c", "unrelated-card")
    record["updates"] = [
        {
            "target_key": "card:" + cid,
            "before": None,
            "after": {
                "kind": "card",
                "owner": "EXAMPLE",
                "data": {
                    "id": cid,
                    "layout": "single",
                    "identity_state": "confirmed",
                    "home_set_id": "EXAMPLE",
                },
            },
            "allocation_anchor": "unrelated-card",
        }
    ]
    fid = allocated("f", "unrelated-face")
    record["updates"].append(
        {
            "target_key": "face:" + fid,
            "before": None,
            "after": {
                "kind": "face",
                "owner": "EXAMPLE",
                "data": {"id": fid, "card_id": cid, "ordinal": 0, "side": "front"},
            },
            "allocation_anchor": "unrelated-face",
        }
    )
    with pytest.raises(ValueError, match="explicit repair destination"):
        replay(root, write_scenario(root, files, [record]))


def test_effective_registry_rejects_reverse_reskin_pair(tmp_path: Path) -> None:
    rows = seed()
    for index, (source, target) in enumerate(((A, B), (B, A))):
        evidence = [
            {
                "role": "from" if row["data"]["card_id"] == source else "to",
                **row["data"]["observation"],
            }
            for row in rows
            if row["kind"] == "printing" and row["data"]["card_id"] in {source, target}
        ]
        rows.append(
            entry(
                "card_related",
                "r:" + str(index + 1) * 32,
                {
                    "from_card_id": source,
                    "to_card_id": target,
                    "relation": "same_rules_reskin",
                    "source_kind": "authored",
                    "target_printing_id": None,
                    "suggested_count": None,
                    "dsl_id": None,
                    "evidence": evidence,
                },
            )
        )
    files = basis(tmp_path, rows)
    with pytest.raises(ValueError, match="Reverse reskin relationship"):
        replay(tmp_path, write_scenario(tmp_path, files, []))

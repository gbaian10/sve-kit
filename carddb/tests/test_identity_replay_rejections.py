"""Isolated typed counterexamples for guards masked by earlier replay validation."""

import copy
import operator
from dataclasses import replace
from typing import TYPE_CHECKING, Any, Literal, cast

import pytest
from pydantic import JsonValue

from sve_carddb.registry.inputs import canonical
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.transitions.models import (
    ArtTransfer,
    Repair,
    Route,
    Transition,
)
from sve_carddb.registry.transitions.ownership import (
    _art_uses,
    _faces,
    _repair,
    _targets,
    _transfer_arts,
    typed,
)
from sve_carddb.registry.transitions.replay import (
    _append,
    _base,
    _closure,
    _stable,
    _updates,
    replay,
)
from sve_carddb.registry.transitions.routing import derive, project
from sve_carddb.registry.transitions.state import EffectiveRegistry, Entity, entity

from .identity_replay_fixtures import (
    basis,
    frames,
    seed,
    subsequent,
    transaction,
    write_scenario,
)
from .identity_transition_fixtures import FA, FB, FC, A, B, C, P, Q, X, Y, chain, entry
from .identity_transition_fixtures import merge_record as merge_record  # ruff: ignore[useless-import-alias] -- share immutable synthetic receipt
from .test_identity_replay import copy_base, renewal_record
from .test_identity_replay import replay_base as replay_base  # ruff: ignore[useless-import-alias] -- share immutable synthetic base
from .test_identity_replay_history import double_base as double_base  # ruff: ignore[useless-import-alias] -- share complete double-face synthetic base
from .test_identity_replay_history import effective_state, fix_root_refs

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.registry.storage import RegistryFiles


def checked(record: dict[str, Any]) -> Transition:
    return Transition.model_validate_json(canonical(chain([record])[0]["records"][0]))


def change(
    records: dict[str, Entity], key: str, **data: JsonValue
) -> dict[str, Entity]:
    result = dict(records)
    old = records[key]
    row = old.entry()
    assert row is not None
    row.data.update(data)
    result[key] = replace(old, content=canonical(row.model_dump(mode="json")))
    return result


@pytest.fixture(scope="module")
def move_state(
    replay_base: RegistryFiles, merge_record: dict[str, Any]
) -> tuple[dict[str, Entity], dict[str, Entity], Transition]:
    before = _base(replay_base)
    record = checked(transaction(merge_record, replay_base))
    after = _updates(before, record, set(before))
    return before, after, record


def test_basis_serialization_must_match_its_content_hash(
    replay_base: RegistryFiles,
) -> None:
    shards = (
        replace(replay_base.shards[0], content_hash="sha256:" + "0" * 64),
        *replay_base.shards[1:],
    )
    with pytest.raises(
        ValueError, match=r"^Registry envelope serialization changed canonical content$"
    ):
        _base(replace(replay_base, shards=shards))


def test_appended_id_cannot_collide_with_effective_history(
    replay_base: RegistryFiles,
) -> None:
    records = {"card:" + A: _base(replay_base)["card:" + A]}
    with pytest.raises(
        ValueError,
        match=r"^Appended registry ID collides with transition allocation history$",
    ):
        _append(None, replay_base, records, set(records))


def test_retired_destination_cannot_be_allocated(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
) -> None:
    before, _, record = move_state
    raw = record.model_dump(mode="json")
    update = next(u for u in raw["updates"] if u["target_key"] == "card:" + A)
    update["target_key"] = "card:" + B
    update["before"] = None
    update["allocation_anchor"] = "synthetic-new-card"
    update["after"]["record_key"] = "card:" + B
    update["after"]["data"]["id"] = B
    raw["updates"].sort(key=operator.itemgetter("target_key"))
    with pytest.raises(
        ValueError, match=r"^Apply cannot allocate a retired destination card$"
    ):
        _updates(before, checked(raw), set(before))


def test_retired_card_cannot_own_printings(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
) -> None:
    before, _, _ = move_state
    with pytest.raises(
        ValueError, match=r"^Retired card must not have effective printings$"
    ):
        _closure(change(before, "card:" + A, identity_state="retired"))


def test_art_use_cannot_be_shared_by_two_art(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
) -> None:
    before, _, _ = move_state
    row = before["art:" + X].entry()
    assert row is not None
    row.data["id"] = "a:" + "d" * 32
    row.record_key = "art:" + str(row.data["id"])
    records = dict(before)
    records[row.record_key] = entity(row, row.record_key, None)
    with pytest.raises(
        ValueError, match=r"^Art uses must be unique, not duplicated across art$"
    ):
        _closure(records)


@pytest.mark.parametrize(
    ("kind", "key", "changes"),
    [
        ("face", "face:" + FA, {"card_id": B}),
        ("art", "art:" + X, {"card_id": B, "face_id": FB}),
        ("art", "art:" + X, {"card_id": B}),
        ("art", "art:" + X, {"face_id": FB}),
    ],
)
def test_parent_fields_are_independently_permanent(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
    kind: str,
    key: str,
    changes: dict[str, Any],
) -> None:
    before, _, _ = move_state
    old = before[key].entry()
    assert old is not None
    assert old.kind == kind
    new = old.model_copy(deep=True)
    new.data.update(changes)
    with pytest.raises(
        ValueError, match=r"^Transition changes a permanent field outside its contract$"
    ):
        _stable(old, new)


@pytest.mark.parametrize(
    ("damage", "message"),
    [
        ("retired", "Retired card cannot be reused by apply"),
        ("partial", "Repair printing move coverage differs from original ownership"),
        ("foreign", "Repair printing move coverage differs from original ownership"),
        ("remaining", "Repair retirement must match remaining printing ownership"),
        (
            "nonretiring_empty",
            "Repair retirement must match remaining printing ownership",
        ),
    ],
)
def test_original_ownership_rejections(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
    damage: str,
    message: str,
) -> None:
    before, after, record = move_state
    repair = record.repairs[0]
    if damage == "retired":
        before = after
    elif damage in {"partial", "foreign"}:
        raw = repair.model_dump(mode="json")
        raw["printing_moves"] = raw["printing_moves"][:1]
        if damage == "foreign":
            raw["printing_moves"][0]["printing_id"] = "p:" + "3" * 32
        repair = Repair.model_validate_json(canonical(raw))
    elif damage == "remaining":
        after = before
    elif damage == "nonretiring_empty":
        raw = repair.model_dump(mode="json")
        raw["kind"] = "reassign_printing"
        raw["printing_moves"] = raw["printing_moves"][:1]
        raw["retire_old"] = False
        repair = Repair.model_validate_json(canonical(raw))
    with pytest.raises(ValueError, match=f"^{message}$"):
        _repair(before, after, repair)


def test_remaining_art_uses_are_exact(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
) -> None:
    before, after, record = move_state
    raw = record.repairs[0].model_dump(mode="json")
    raw["art_moves"][0]["remaining_uses"] = [{"printing_id": Q, "face_id": FA}]
    repair = Repair.model_validate_json(canonical(raw))
    with pytest.raises(
        ValueError, match=r"^Art remaining uses do not match unmoved printing uses$"
    ):
        _transfer_arts(before, after, repair, _art_uses(before))


@pytest.mark.parametrize(
    ("damage", "message"),
    [
        ("duplicate_target", "Art transfer target must be listed once"),
        ("wrong_face", "Art target belongs to another face"),
        ("empty_target", "Art transfer target requires an actual moved use"),
        ("foreign", "Art transfer use partition is missing, duplicated or foreign"),
        (
            "not_original",
            "Art transfer use partition is missing, duplicated or foreign",
        ),
        (
            "duplicate_use",
            "Art transfer use partition is missing, duplicated or foreign",
        ),
        ("wrong_art", "Art move destination disagrees with printing face art"),
        (
            "missing_use",
            "Art transfer targets and remaining uses must partition old uses",
        ),
    ],
)
def test_art_target_rejections(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
    damage: str,
    message: str,
) -> None:
    _, after, record = move_state
    repair = record.repairs[0]
    moved = {
        (m.printing_id, f.from_face_id): f
        for m in repair.printing_moves
        for f in m.faces
    }
    raw = repair.art_moves[0].model_dump(mode="json")
    target = raw["targets"][0]
    original = {(P, FA), (Q, FA)}
    if damage == "duplicate_target":
        extra = copy.deepcopy(target)
        target["uses"] = target["uses"][:1]
        extra["uses"] = extra["uses"][1:]
        raw["targets"].append(extra)
    elif damage == "wrong_face":
        target["to_face_id"] = FC
    elif damage == "empty_target":
        target["uses"] = []
    elif damage == "foreign":
        target["uses"][0]["printing_id"] = "p:" + "3" * 32
        target["uses"].sort(key=canonical)
    elif damage == "not_original":
        original.remove((P, FA))
    elif damage == "duplicate_use":
        extra = copy.deepcopy(target)
        extra["to_art_id"] = "a:" + "e" * 32
        row = after["art:" + Y].entry()
        assert row is not None
        row.data["id"] = extra["to_art_id"]
        row.record_key = "art:" + extra["to_art_id"]
        after = dict(after)
        after[row.record_key] = entity(row, row.record_key, None)
        raw["targets"].append(extra)
    elif damage == "wrong_art":
        moved = {
            key: face.model_copy(update={"to_art_id": X}) for key, face in moved.items()
        }
    else:
        target["uses"].pop()
    raw["targets"].sort(key=canonical)
    transfer = ArtTransfer.model_validate_json(canonical(raw))
    with pytest.raises(ValueError, match=f"^{message}$"):
        _targets(after, transfer, moved, original, set())


@pytest.mark.parametrize(
    ("damage", "message"),
    [
        ("version", "Route source must identify an immutable source version"),
        ("region", "Route source region disagrees with printing"),
        ("number", "Official route requires an exact source number"),
        ("state", "Invalid route source state"),
    ],
)
def test_route_fact_boundary_rejections(
    replay_base: RegistryFiles,
    damage: str,
    message: str,
) -> None:
    facts = list(frames(replay_base, 0)[0])
    fact = facts[0]
    if damage == "version":
        fact = replace(fact, source_version_id="current")
    elif damage == "region":
        fact = replace(fact, region="jp")
    elif damage == "number":
        fact = replace(fact, card_no=None)
    else:
        # RouteFact's annotations are not runtime checks at the adapter boundary.
        fact = replace(
            fact, state=cast("Literal['official', 'provisional', 'unknown']", "invalid")
        )
    facts[0] = fact
    with pytest.raises(ValueError, match=f"^{message}$"):
        derive(_base(replay_base), tuple(facts))


def test_permanent_routes_cannot_disappear() -> None:
    previous = project({P: Route(namespace="official", route_key="TEST-01EN")}, None)
    with pytest.raises(
        ValueError, match=r"^Permanent printing routes cannot disappear$"
    ):
        project({}, previous)


@pytest.mark.parametrize("missing", ["basis", "routes"])
def test_public_replay_reports_incomplete_dependencies(
    tmp_path: Path,
    replay_base: RegistryFiles,
    merge_record: dict[str, Any],
    missing: str,
) -> None:
    copy_base(replay_base, tmp_path)
    record = transaction(merge_record, replay_base)
    inputs = write_scenario(tmp_path, replay_base, [record])
    if missing == "basis":
        inputs.bases.clear()
    else:
        inputs.frames.clear()
    with pytest.raises(
        ValueError, match=r"^Incomplete effective identity dependency closure$"
    ):
        replay(tmp_path, inputs)


def test_browse_hides_descendants_of_retired_card(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
) -> None:
    before, _, _ = move_state
    records = change(before, "card:" + A, identity_state="retired")
    result = EffectiveRegistry(records, (), project({}, None))
    for kind, key in (("card", A), ("face", FA), ("printing", P), ("art", X)):
        assert key not in {e.data["id"] for e in result.browse(kind)}
    assert 60001 not in {e.data["int_id"] for e in result.browse("card_int_id")}


def test_split_hint_removes_a_later_retired_choice(
    tmp_path: Path,
    replay_base: RegistryFiles,
    merge_record: dict[str, Any],
) -> None:
    copy_base(replay_base, tmp_path)
    first = transaction(merge_record, replay_base, kind="split")
    state = effective_state(replay_base, first)
    second = subsequent(first, state, P, B, C)
    fix_root_refs(second, replay_base)
    state = effective_state(replay_base, first)
    for update in second["updates"]:
        if update["after"] is None:
            state.pop(update["target_key"], None)
        else:
            state[update["target_key"]] = update["after"]
    third = subsequent(first, state, "p:" + "3" * 32, B, C)
    fix_root_refs(third, replay_base)
    third["repairs"][0]["id"] = "synthetic-repair-3"
    result = replay(
        tmp_path, write_scenario(tmp_path, replay_base, [first, second, third])
    )
    hint = result.resolve_int_id(60001)
    assert hint is not None
    assert hint.status == "choice_required"
    assert hint.card_id == C
    assert hint.choices == (C,)


def test_post_review_append_needs_new_route_evidence(
    tmp_path: Path,
    replay_base: RegistryFiles,
    merge_record: dict[str, Any],
) -> None:
    copy_base(replay_base, tmp_path)
    cid, fid, pid = "c:" + "d" * 32, "f:" + "d" * 32, "p:" + "5" * 32
    rows = [
        *seed(),
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
        entry(
            "printing",
            pid,
            {
                "card_id": cid,
                "region": "jp",
                "card_no": "TEST-05",
                "variant_key": "base",
                "home_set_id": "EXAMPLE",
                "source_face_map": [{"source_index": 0, "face_id": fid}],
                "observation": {
                    "region": "jp",
                    "card_no": "TEST-05",
                    "recipe": "registry-observation-v1",
                    "observation_hash": "sha256:" + "8" * 64,
                    "rules_hash": "sha256:" + "9" * 64,
                },
            },
        ),
        {
            "record_key": "card_int_id:" + pid,
            "kind": "card_int_id",
            "owner": "EXAMPLE",
            "data": {"int_id": 20001, "printing_id": pid},
        },
    ]
    basis(tmp_path, rows)
    record = transaction(merge_record, replay_base)
    with pytest.raises(
        ValueError,
        match=r"^Unreviewed registry append requires complete new route evidence$",
    ):
        replay(tmp_path, write_scenario(tmp_path, replay_base, [record]))


def test_public_repair_must_list_every_old_face(
    tmp_path: Path,
    double_base: RegistryFiles,
    merge_record: dict[str, Any],
) -> None:
    copy_base(double_base, tmp_path)
    record = transaction(merge_record, double_base)
    back_source, back_target = FA[:-1] + "0", FB[:-1] + "0"
    for update in record["updates"]:
        if update["target_key"] in {"printing:" + P, "printing:" + Q}:
            update["after"]["data"]["source_face_map"][1]["face_id"] = back_target
    for move in record["repairs"][0]["printing_moves"]:
        move["faces"].append(
            {
                "source_index": 1,
                "from_face_id": back_source,
                "to_face_id": back_target,
                "from_art_id": None,
                "to_art_id": None,
            }
        )
    inputs = write_scenario(tmp_path, double_base, [record])
    with pytest.raises(
        ValueError, match=r"^Repair must list every old face, including unused faces$"
    ):
        replay(tmp_path, inputs)


def test_undeclared_printing_cannot_change_face_mapping(
    tmp_path: Path,
    double_base: RegistryFiles,
    merge_record: dict[str, Any],
) -> None:
    copy_base(double_base, tmp_path)
    record = renewal_record(transaction(merge_record, double_base), double_base)
    record["updates"][0]["after"]["data"]["source_face_map"] = [
        {"source_index": 0, "face_id": FA[:-1] + "0"},
        {"source_index": 1, "face_id": FA},
    ]
    with pytest.raises(
        ValueError,
        match=r"^Face mapping change requires a printing parent repair$",
    ):
        replay(tmp_path, write_scenario(tmp_path, double_base, [record]))


@pytest.mark.parametrize("side", ["source", "destination"])
def test_printing_move_parents_match_effective_records(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
    side: str,
) -> None:
    before, after, record = move_state
    old_printings, new_printings = (
        typed(before, PrintingData),
        typed(after, PrintingData),
    )
    printings = old_printings if side == "source" else new_printings
    printings["printing:" + P] = printings["printing:" + P].model_copy(
        update={"card_id": C}
    )
    with pytest.raises(
        ValueError,
        match=r"^Printing move parents disagree with effective records$",
    ):
        _faces(after, record.repairs[0], old_printings, new_printings)


@pytest.mark.parametrize("damage", ["parent", "missing"])
def test_face_transfer_destination_has_parent_and_declaration(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
    damage: str,
) -> None:
    before, after, record = move_state
    repair = record.repairs[0]
    if damage == "parent":
        after = change(after, "face:" + FB, card_id=C)
    else:
        raw = repair.model_dump(mode="json")
        raw["face_moves"][0]["to_face_ids"] = [FC]
        repair = Repair.model_validate_json(canonical(raw))
    with pytest.raises(
        ValueError,
        match=r"^Face transfer destination has wrong parent or is missing$",
    ):
        _faces(after, repair, typed(before, PrintingData), typed(after, PrintingData))


def test_face_transfer_cannot_list_unused_destination(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
) -> None:
    before, after, record = move_state
    raw = record.repairs[0].model_dump(mode="json")
    raw["face_moves"][0]["to_face_ids"] = sorted([FB, FC])
    repair = Repair.model_validate_json(canonical(raw))
    with pytest.raises(
        ValueError,
        match=r"^Face transfer contains unused or missing destinations$",
    ):
        _faces(after, repair, typed(before, PrintingData), typed(after, PrintingData))


def test_unused_face_transfer_stays_within_repair_cards(
    move_state: tuple[dict[str, Entity], dict[str, Entity], Transition],
) -> None:
    before, after, record = move_state
    raw = record.repairs[0].model_dump(mode="json")
    raw["face_moves"].append({"from_face_id": FC, "to_face_ids": [FA]})
    raw["face_moves"].sort(key=canonical)
    repair = Repair.model_validate_json(canonical(raw))
    with pytest.raises(
        ValueError,
        match=r"^Face transfer destination is outside repair cards$",
    ):
        _faces(after, repair, typed(before, PrintingData), typed(after, PrintingData))

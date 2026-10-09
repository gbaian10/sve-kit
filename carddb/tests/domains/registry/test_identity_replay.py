"""Effective synthetic replay, with independently re-signed semantic counterexamples."""

import copy
import operator
from dataclasses import replace
from typing import TYPE_CHECKING, Any

import pytest

from sve_carddb.domains.registry.storage import (
    load,
    plan_files,
    read_registry_files,
    relayout,
)
from sve_carddb.domains.registry.transitions.replay import replay

from ...support.identity_replay_fixtures import (
    allocated,
    basis,
    seed,
    transaction,
    write_scenario,
)
from ...support.identity_transition_fixtures import (
    FA,
    FB,
    A,
    B,
    C,
    P,
    Q,
    X,
    Y,
    checksum,
    record_key,
    wire,
)
from ...support.identity_transition_fixtures import merge_record as merge_record  # ruff: ignore[useless-import-alias] -- expose the shared synthetic receipt fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.storage import Entry, RegistryFiles
    from sve_carddb.domains.registry.transitions.state import EffectiveRegistry


@pytest.fixture(scope="module")
def replay_base(tmp_path_factory: pytest.TempPathFactory) -> RegistryFiles:
    return basis(tmp_path_factory.mktemp("identity-replay-base"), seed())


def copy_base(files: RegistryFiles, root: Path) -> None:
    (root / "ids").mkdir()
    (root / "ids/index.yaml").write_bytes(files.index_content)
    for shard in files.shards:
        path = root / shard.path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(shard.exact_content)


@pytest.fixture
def scenario(
    tmp_path: Path, replay_base: RegistryFiles, merge_record: dict[str, Any]
) -> tuple[Path, RegistryFiles, dict[str, Any]]:
    copy_base(replay_base, tmp_path)
    return tmp_path, replay_base, transaction(merge_record, replay_base)


@pytest.mark.parametrize("kind", ["merge", "split", "reassign_printing"])
def test_effective_replay_and_browsing(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
    merge_record: dict[str, Any],
    kind: str,
) -> None:
    root, files, _ = scenario
    record = transaction(merge_record, files, kind=kind)
    inputs = write_scenario(root, files, [record])
    before = {p: p.read_bytes() for p in root.rglob("*.yaml")}
    result = replay(root, inputs)
    repeated = replay(root, inputs)
    assert result == repeated
    assert before == {p: p.read_bytes() for p in root.rglob("*.yaml")}
    printings = {e.data["id"]: e.data for e in result.browse("printing")}
    assert printings[P]["card_id"] == B
    assert printings[Q]["card_id"] == (
        A if kind == "reassign_printing" else C if kind == "split" else B
    )
    assert printings[P]["source_face_map"] == [{"source_index": 0, "face_id": FB}]
    hint = result.resolve_int_id(60001)
    assert hint is not None
    assert hint.printing_id == P
    assert hint.card_id == B
    assert hint.status == ("choice_required" if kind == "split" else "repaired")
    assert hint.choices == ((B, C) if kind == "split" else ())
    assert result.resolve_int_id(98765) is None
    assert {
        e.data["int_id"]: e.data["printing_id"] for e in result.browse("card_int_id")
    } == {60001: P, 60002: Q, 60003: "p:" + "3" * 32, 60004: "p:" + "4" * 32}
    old = result.records["card:" + A].entry()
    assert old is not None
    assert old.data["identity_state"] == (
        "confirmed" if kind == "reassign_printing" else "retired"
    )
    assert (A in {e.data["id"] for e in result.browse("card")}) == (
        kind == "reassign_printing"
    )
    assert (FA in {e.data["id"] for e in result.browse("face")}) == (
        kind == "reassign_printing"
    )
    assert (X in {e.data["id"] for e in result.browse("art")}) == (
        kind == "reassign_printing"
    )
    assert "face:" + FA in result.records
    assert "art:" + X in result.records
    original = result.records["printing:" + P].entry()
    assert original is not None
    original.data.clear()
    assert (
        result.records["printing:" + P].entry()
        == repeated.records["printing:" + P].entry()
    )


def test_empty_events_keep_identity_and_routes(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, _ = scenario
    result = replay(root, write_scenario(root, files, []))
    assert not result.repairs
    assert len(result.browse("printing")) == 4
    hint = result.resolve_int_id(60001)
    assert hint is not None
    assert hint.status == "unchanged"
    route = result.routes.resolve("official", "TEST-01EN")
    assert route is not None
    assert route.printing_id == P


@pytest.mark.parametrize("damage", ["hash", "key", "missing"])
def test_original_before_refs_are_exact(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]], damage: str
) -> None:
    root, files, record = scenario
    before = record["updates"][0]["before"]
    if damage == "hash":
        before["record_hash"] = "sha256:" + "0" * 64
    elif damage == "key":
        before["record_key"] = "card:" + B
    else:
        record["updates"][0]["target_key"] = "art:a:" + "f" * 32
        before["record_key"] = record["updates"][0]["target_key"]

        record["updates"][0]["after"]["data"]["id"] = "a:" + "f" * 32
        record["updates"].sort(key=operator.itemgetter("target_key"))
    with pytest.raises(
        ValueError,
        match={
            "hash": "^Transition before must match exact effective registry reference$",
            "key": "^Invalid identity transition authored fields$",
            "missing": "^Transition before must match exact effective registry reference$",
        }[damage],
    ):
        replay(root, write_scenario(root, files, [record]))


@pytest.mark.parametrize(
    "damage",
    [
        "owner",
        "region",
        "number",
        "variant",
        "face_parent",
        "art_parent",
        "art_class",
        "card_layout",
        "int_id",
    ],
)
def test_permanent_fields_and_allocations_never_change(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]], damage: str
) -> None:
    root, files, record = scenario
    updates = {u["target_key"]: u for u in record["updates"]}
    if damage == "owner":
        updates["printing:" + P]["after"]["owner"] = "OTHER"
    elif damage in {"region", "number", "variant"}:
        data = updates["printing:" + P]["after"]["data"]
        data[
            {"region": "region", "number": "card_no", "variant": "variant_key"}[damage]
        ] = "jp" if damage == "region" else "OTHER"
    elif damage in {"art_parent", "art_class"}:
        updates["art:" + Y]["after"]["data"][
            "card_id" if damage == "art_parent" else "classification"
        ] = A if damage == "art_parent" else "base"
    elif damage == "card_layout":
        updates["card:" + A]["after"]["data"]["layout"] = "double_faced"
    else:
        raw = next(
            e.model_dump(mode="json", round_trip=True)
            for s in files.shards
            for e in s.envelope().records
            if e.record_key
            == ("face:" + FA if damage == "face_parent" else "card_int_id:" + P)
        )
        changed = copy.deepcopy(raw)
        changed["data"]["card_id" if damage == "face_parent" else "int_id"] = (
            B if damage == "face_parent" else 60009
        )
        record["updates"].append(
            {
                "target_key": record_key(raw),
                "before": {
                    "transition_key": None,
                    "record_key": record_key(raw),
                    "record_hash": checksum(raw),
                },
                "after": changed,
                "allocation_anchor": None,
            }
        )
        record["updates"].sort(key=operator.itemgetter("target_key"))
    with pytest.raises(
        ValueError,
        match={
            "owner": "^Transition cannot change stable record key, kind or owner$",
            "region": "^Invalid identity transition authored fields$",
            "number": "^Transition changes a permanent field outside its contract$",
            "variant": "^Transition changes a permanent field outside its contract$",
            "face_parent": "^Transition changes a permanent field outside its contract$",
            "art_parent": "^Transition changes a permanent field outside its contract$",
            "art_class": "^Transition changes a permanent field outside its contract$",
            "card_layout": "^Transition changes a permanent field outside its contract$",
            "int_id": "^Invalid identity transition authored fields$",
        }[damage],
    ):
        replay(root, write_scenario(root, files, [record]))


@pytest.mark.parametrize(
    "damage",
    [
        "no_repairs",
        "missing_printing",
        "missing_face",
        "wrong_face",
        "missing_art",
        "discard_art",
        "wrong_art",
        "art_partition",
        "old_use",
        "target_use",
        "retirement",
        "missing_after",
        "confirmed_none",
    ],
)
def test_complete_ownership_is_required(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]], damage: str
) -> None:
    root, files, record = scenario
    if damage in {
        "missing_art",
        "discard_art",
        "wrong_art",
        "art_partition",
        "old_use",
        "target_use",
    }:
        damage_art(record, damage)
    else:
        damage_ownership(record, damage)
    with pytest.raises(
        ValueError,
        match={
            "no_repairs": "^Repairs must exactly explain every changed printing parent$",
            "missing_printing": "^Repairs must exactly explain every changed printing parent$",
            "missing_face": "^Invalid identity transition authored fields$",
            "wrong_face": "^Invalid identity transition authored fields$",
            "missing_art": "^Repair must list all old art, including unused candidates$",
            "discard_art": "^Invalid identity transition authored fields$",
            "wrong_art": "^Invalid identity transition authored fields$",
            "art_partition": "^Art transfer targets and remaining uses must partition old uses$",
            "old_use": "^English original art attached to the wrong printing$",
            "target_use": "^Art observation must refer to one actual current use$",
            "retirement": "^Repairs must exactly explain card retirement$",
            "missing_after": "^Art uses change is not exactly explained by transfers$",
            "confirmed_none": "^Printing evidence coverage mismatch; requires re\\-review: versioned review/relation decisions are not supported$",
        }[damage],
    ):
        replay(root, write_scenario(root, files, [record]))


def test_legacy_entry_points_still_refuse_nonempty(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    write_scenario(root, files, [record])
    for reader in (load, read_registry_files):
        with pytest.raises(
            ValueError,
            match=r"^Nonempty identity transitions require effective projection support$",
        ):
            reader(root)
    with pytest.raises(
        ValueError,
        match=r"^Nonempty identity transitions require effective projection support$",
    ):
        plan_files(root, [], loaded=(files.index(), {}))
    with pytest.raises(
        ValueError,
        match=r"^Nonempty identity transitions require effective projection support$",
    ):
        relayout(root, [])


def renewal_record(record: dict[str, Any], files: RegistryFiles) -> dict[str, Any]:
    result = copy.deepcopy(record)
    result["repairs"] = []
    original = next(
        e.model_dump(mode="json", round_trip=True)
        for s in files.shards
        for e in s.envelope().records
        if e.record_key == "printing:" + P
    )
    result["updates"] = [
        {
            "target_key": record_key(original),
            "before": {
                "transition_key": None,
                "record_key": record_key(original),
                "record_hash": checksum(original),
            },
            "after": original,
            "allocation_anchor": None,
        }
    ]
    result["routes"] = []
    return result


def route_state(key: str, aliases: tuple[str, ...] = ()) -> dict[str, Any]:
    return {
        "canonical": {"namespace": "official", "route_key": key},
        "aliases": sorted(
            [
                {"namespace": "official", "route_key": old, "reason": "renumbered"}
                for old in aliases
            ],
            key=wire,
        ),
    }


def renumber_pair(record: dict[str, Any], files: RegistryFiles) -> list[dict[str, Any]]:
    first = renewal_record(record, files)
    second = copy.deepcopy(first)
    first["routes"] = [
        {
            "printing_id": P,
            "before": route_state("TEST-01EN"),
            "after": route_state("TEST-NEWEN", ("TEST-01EN",)),
        }
    ]
    second["routes"] = [
        {
            "printing_id": P,
            "before": first["routes"][0]["after"],
            "after": route_state("TEST-FINALEN", ("TEST-01EN", "TEST-NEWEN")),
        }
    ]
    return [first, second]


def test_two_renumbers_flatten_all_old_entries(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:

    root, files, record = scenario
    inputs = write_scenario(root, files, renumber_pair(record, files))
    inputs.frames[1] = tuple(
        replace(f, card_no="TEST-NEWEN") if f.printing_id == P else f
        for f in inputs.frames[0]
    )
    inputs.frames[2] = tuple(
        replace(f, card_no="TEST-FINALEN") if f.printing_id == P else f
        for f in inputs.frames[0]
    )
    result = replay(root, inputs)
    for key in ("TEST-01EN", "TEST-NEWEN", "TEST-FINALEN"):
        route = result.routes.resolve("official", key)
        assert route is not None
        assert route.printing_id == P
        assert route.key == "TEST-FINALEN"
    assert len(result.routes.states[P].aliases) == 2
    assert result.routes.resolve("official", "missing") is None
    assert not result.repairs
    hint = result.resolve_int_id(60001)
    assert hint is not None
    assert hint.printing_id == P
    assert required(result, "printing:" + P).data["card_no"] == "TEST-01EN"


@pytest.mark.parametrize(
    "damage",
    [
        "missing_alias",
        "foreign_alias",
        "wrong_before",
        "omitted",
        "unrequested",
        "unknown",
        "foreign_source",
        "source_collision",
        "uncovered",
    ],
)
def test_routes_require_independent_sources_and_permanent_keys(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]], damage: str
) -> None:

    root, files, record = scenario
    records = renumber_pair(record, files)
    if damage == "missing_alias":
        records[1]["routes"][0]["after"]["aliases"].pop()
    elif damage == "foreign_alias":
        records[1]["routes"][0]["after"]["aliases"].append(
            {"namespace": "official", "route_key": "foreign", "reason": "merged"}
        )
        records[1]["routes"][0]["after"]["aliases"].sort(key=wire)
    elif damage == "wrong_before":
        records[1]["routes"][0]["before"] = route_state("OTHER")
    elif damage == "omitted":
        records[1]["routes"] = []
    elif damage == "unrequested":
        records[1]["routes"][0]["after"] = route_state(
            "OTHER", ("TEST-01EN", "TEST-NEWEN")
        )
    inputs = write_scenario(root, files, records)
    inputs.frames[1] = tuple(
        replace(f, card_no="TEST-NEWEN") if f.printing_id == P else f
        for f in inputs.frames[0]
    )
    inputs.frames[2] = tuple(
        replace(f, card_no="TEST-FINALEN") if f.printing_id == P else f
        for f in inputs.frames[0]
    )
    if damage == "unknown":
        inputs.frames[2] = tuple(
            replace(f, state="unknown") if f.printing_id == P else f
            for f in inputs.frames[2]
        )
    elif damage == "foreign_source":
        inputs.frames[2] = tuple(
            replace(f, source_version_id="src:v1:" + "0" * 64) for f in inputs.frames[2]
        )
    elif damage == "source_collision":
        inputs.frames[2] = tuple(
            replace(f, card_no="TEST-02EN") if f.printing_id == P else f
            for f in inputs.frames[2]
        )
    elif damage == "uncovered":
        inputs.frames[2] = inputs.frames[2][1:]
    with pytest.raises(
        ValueError,
        match={
            "missing_alias": "^Transition routes differ from complete source\\-derived changes$",
            "foreign_alias": "^Transition routes differ from complete source\\-derived changes$",
            "wrong_before": "^Transition routes differ from complete source\\-derived changes$",
            "omitted": "^Transition routes differ from complete source\\-derived changes$",
            "unrequested": "^Transition routes differ from complete source\\-derived changes$",
            "unknown": "^Unknown route source cannot prove an official number$",
            "foreign_source": "^Route sources must be pinned in transition evidence$",
            "source_collision": "^Exact route collision requires a confirmed route override$",
            "uncovered": "^Route source facts must cover each printing exactly once$",
        }[damage],
    ):
        replay(root, inputs)


def test_basis_hash_and_exact_original_bytes(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    record["registry_basis"]["index_hash"] = "sha256:" + "0" * 64
    with pytest.raises(
        ValueError, match=r"^Historical registry basis index hash mismatch$"
    ):
        replay(root, write_scenario(root, files, [record]))
    record["registry_basis"]["index_hash"] = checksum(
        files.index().model_dump(mode="json", round_trip=True)
    )
    inputs = write_scenario(root, files, [record])
    path = root / files.shards[0].path
    path.write_bytes(b"# changed original bytes\n" + path.read_bytes())
    with pytest.raises(
        ValueError,
        match=r"^Registry bases must retain exact old shards$",
    ):
        replay(root, inputs)


def test_unknown_art_cannot_be_inferred(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    record["repairs"][0]["printing_moves"][0]["faces"][0]["from_art_id"] = None
    record["repairs"][0]["printing_moves"].sort(key=wire)
    with pytest.raises(
        ValueError,
        match=r"^Printing move art does not match actual old use$",
    ):
        replay(root, write_scenario(root, files, [record]))


def test_original_id_allocation_collision(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:

    root, files, record = scenario
    # Reassign the proposed new ID to an original registered art with the same recipe.
    aid = allocated("a", "synthetic-original-art")
    original = copy.deepcopy(seed())
    for row in original:
        if record_key(row) == "art:" + Y:
            row["data"]["id"] = aid
    other_root = root / "other-base"
    other_root.mkdir()
    collision_files = basis(other_root, original)
    write_scenario(root, files, [])
    # Use the collision registry as the complete current/base closure.
    for path in root.rglob("*.yaml"):
        if not path.is_relative_to(other_root):
            path.unlink()
    for old in (root / "registry").rglob("*"):
        if old.is_file():
            old.unlink()
    (root / "ids/index.yaml").write_bytes(collision_files.index_content)
    for shard in collision_files.shards:
        path = root / shard.path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(shard.exact_content)
    record["registry_basis"]["index_hash"] = checksum(
        collision_files.index().model_dump(mode="json", round_trip=True)
    )
    collision_update(record, aid, collision_files)
    with pytest.raises(
        ValueError,
        match=r"^Permanent ID cannot reuse original or historical allocation$",
    ):
        replay(root, write_scenario(root, collision_files, [record]))


def required(result: EffectiveRegistry, key: str) -> Entry:
    row = result.records[key].entry()
    assert row is not None
    return row


def damage_ownership(record: dict[str, Any], damage: str) -> None:
    repair = record["repairs"][0]
    if damage == "no_repairs":
        record["repairs"] = []
    elif damage == "missing_printing":
        repair["printing_moves"].pop()
    elif damage == "missing_face":
        repair["face_moves"] = []
    elif damage == "wrong_face":
        repair["printing_moves"][0]["faces"][0]["from_face_id"] = FB
    elif damage == "retirement":
        next(u for u in record["updates"] if u["target_key"] == "card:" + A)["after"][
            "data"
        ]["identity_state"] = "confirmed"
    elif damage == "confirmed_none":
        next(
            u
            for u in record["updates"]
            if u["target_key"] == "region_mapping_review:" + B
        )["after"]["data"]["observations"].pop()
    else:
        record["updates"] = [
            u for u in record["updates"] if u["target_key"] != "art:" + Y
        ]


def damage_art(record: dict[str, Any], damage: str) -> None:
    repair = record["repairs"][0]
    if damage == "missing_art":
        repair["art_moves"] = []
    elif damage == "discard_art":
        repair["printing_moves"][0]["faces"][0]["to_art_id"] = None
    elif damage == "wrong_art":
        repair["printing_moves"][0]["faces"][0]["from_art_id"] = Y
    elif damage == "art_partition":
        missing = repair["art_moves"][0]["targets"][0]["uses"].pop()
        target = next(u for u in record["updates"] if u["target_key"] == "art:" + Y)
        target["after"]["data"]["uses"] = [
            u for u in target["after"]["data"]["uses"] if u != missing
        ]
    elif damage in {"old_use", "target_use"}:
        key = "art:" + (X if damage == "old_use" else Y)
        data = next(
            u["after"]["data"] for u in record["updates"] if u["target_key"] == key
        )
        if damage == "old_use":
            data["uses"] = [{"printing_id": P, "face_id": FA}]
        else:
            data["uses"].pop()


def collision_update(
    record: dict[str, Any], aid: str, collision_files: RegistryFiles
) -> None:
    for update in record["updates"]:
        if update["target_key"] == "art:" + Y:
            update["target_key"] = "art:" + aid
            update["before"] = None
            update["allocation_anchor"] = "synthetic-original-art"

            update["after"]["data"]["id"] = aid
    for move in record["repairs"][0]["printing_moves"]:
        move["faces"][0]["to_art_id"] = aid
    record["repairs"][0]["art_moves"][0]["targets"][0]["to_art_id"] = aid
    for update in record["updates"]:
        if update["before"] is not None:
            for shard in collision_files.shards:
                for raw in shard.envelope().records:
                    if raw.record_key == update["target_key"]:
                        update["before"]["record_hash"] = checksum(
                            raw.model_dump(mode="json", round_trip=True)
                        )
    record["updates"].sort(key=operator.itemgetter("target_key"))


def test_original_int_id_mapping_cannot_be_rewritten(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    inputs = write_scenario(root, files, [record])
    for loaded in files.shards:
        raw = loaded.envelope().model_dump(mode="json", round_trip=True)
        if raw["records"][0]["kind"] == "card_int_id":
            raw["records"][0]["data"]["int_id"], raw["records"][1]["data"]["int_id"] = (
                raw["records"][1]["data"]["int_id"],
                raw["records"][0]["data"]["int_id"],
            )
            (root / loaded.path).write_bytes(wire(raw))
            break
    with pytest.raises(
        ValueError,
        match=r"^Registry bases must retain exact old shards$",
    ):
        replay(root, inputs)


def test_canonical_cannot_hijack_another_printings_old_alias(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    first, second = renumber_pair(record, files)
    second["routes"] = [
        {
            "printing_id": Q,
            "before": route_state("TEST-02EN"),
            "after": route_state("TEST-01EN", ("TEST-02EN",)),
        }
    ]
    inputs = write_scenario(root, files, [first, second])
    inputs.frames[1] = tuple(
        replace(f, card_no="TEST-NEWEN") if f.printing_id == P else f
        for f in inputs.frames[0]
    )
    inputs.frames[2] = tuple(
        replace(f, card_no="TEST-01EN") if f.printing_id == Q else f
        for f in inputs.frames[1]
    )
    with pytest.raises(
        ValueError,
        match=r"^Route canonical/alias cannot hijack a permanent entry$",
    ):
        replay(root, inputs)


def test_provisional_to_official_retains_permanent_int_entry(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    renewal = renewal_record(record, files)
    provisional = {
        "canonical": {"namespace": "provisional", "route_key": "60001"},
        "aliases": [],
    }
    official = route_state("TEST-01EN")
    official["aliases"] = [
        {
            "namespace": "provisional",
            "route_key": "60001",
            "reason": "provisional_corrected",
        }
    ]
    renewal["routes"] = [{"printing_id": P, "before": provisional, "after": official}]
    inputs = write_scenario(root, files, [renewal])
    inputs.frames[0] = tuple(
        replace(f, state="provisional", card_no=None) if f.printing_id == P else f
        for f in inputs.frames[0]
    )
    result = replay(root, inputs)
    route = result.routes.resolve("provisional", "60001")
    assert route is not None
    assert route.printing_id == P
    assert route.key == "TEST-01EN"


def test_official_cannot_be_replaced_with_provisional_by_apply(
    scenario: tuple[Path, RegistryFiles, dict[str, Any]],
) -> None:
    root, files, record = scenario
    renewal = renewal_record(record, files)
    inputs = write_scenario(root, files, [renewal])
    inputs.frames[1] = tuple(
        replace(f, state="provisional", card_no=None) if f.printing_id == P else f
        for f in inputs.frames[1]
    )
    with pytest.raises(
        ValueError,
        match=r"^Apply cannot replace an official route with provisional$",
    ):
        replay(root, inputs)

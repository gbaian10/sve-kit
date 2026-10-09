"""Independent counterexamples for the read-only transition boundary."""

import copy
from typing import TYPE_CHECKING, Any
from uuid import UUID, uuid5

import pytest
from pydantic import field_validator

from sve_carddb.core.models import RecordData
from sve_carddb.domains.registry.storage import Index as RegistryIndex
from sve_carddb.domains.registry.storage import (
    load,
    plan_files,
    read_registry_files,
    relayout,
)
from sve_carddb.domains.registry.transitions.files import (
    _inventory,
    checked_model,
    read_transition_files,
)
from sve_carddb.domains.registry.transitions.loader import _revert, load_transitions
from sve_carddb.domains.registry.transitions.models import Reference, Shard

from .identity_transition_fixtures import (
    FA,
    FC,
    A,
    C,
    P,
    Q,
    X,
    Y,
    chain,
    checksum,
    entry,
    pack,
    record_key,
    reference,
    renewal,
    wire,
    write_chain,
)
from .identity_transition_fixtures import merge_record as merge_record  # ruff: ignore[useless-import-alias] -- expose module-scoped synthetic fixture

if TYPE_CHECKING:
    from pathlib import Path


def test_missing_and_empty_directories_have_no_transitions(tmp_path: Path) -> None:
    assert load_transitions(tmp_path).shards == ()
    assert load(tmp_path) == (RegistryIndex(), {})
    write_chain(tmp_path, [])
    assert load_transitions(tmp_path).shards == ()
    assert load(tmp_path) == (RegistryIndex(), {})


@pytest.mark.parametrize("kind", ["merge", "split", "reassign_printing", "renewal"])
def test_read_only_supported_shapes(
    tmp_path: Path,
    merge_record: dict[str, Any],
    kind: str,
) -> None:
    record = copy.deepcopy(merge_record)
    repair = record["repairs"][0]
    if kind == "renewal":
        record = renewal(record)
    else:
        repair["kind"] = kind
    if kind == "reassign_printing":
        repair["retire_old"] = False
    elif kind == "split":
        repair["new_card_ids"].append(C)
        move = copy.deepcopy(repair["printing_moves"][0])
        move["printing_id"] = Q
        move["to_card_id"] = C
        move["faces"][0]["to_face_id"] = FC
        repair["printing_moves"].append(move)
        repair["face_moves"][0]["to_face_ids"].append(FC)
    shards = chain([record])
    write_chain(tmp_path, shards)
    paths = list(tmp_path.rglob("*.yaml"))
    before = {path: path.read_bytes() for path in paths}
    files = load_transitions(tmp_path)
    assert files.shards[0].content_hash == checksum(shards[0])
    assert files.shards[0].content == wire(shards[0])
    assert files.shards[0].exact_content == wire(shards[0])
    assert (
        files.shards[0].envelope().model_dump(mode="json", round_trip=True) == shards[0]
    )
    detached = files.shards[0].envelope().records[0].updates[0].after
    assert detached is not None
    detached.data.clear()
    original = files.shards[0].envelope().records[0].updates[0].after
    assert original is not None
    assert original.data
    assert before == {path: path.read_bytes() for path in paths}


def test_readers_do_not_silently_ignore_nonempty_transitions(
    tmp_path: Path,
    merge_record: dict[str, Any],
) -> None:
    write_chain(tmp_path, chain([merge_record]))
    for reader in (load, read_registry_files):
        with pytest.raises(ValueError, match="effective projection support"):
            reader(tmp_path)


@pytest.mark.parametrize(
    "damage",
    [
        "missing_middle",
        "extra_shard",
        "extra_yml",
        "short_sequence",
        "zero",
        "gap",
        "duplicate_sequence",
        "wrong_sequence",
    ],
)
def test_file_inventory_and_paths(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str,
) -> None:
    write_chain(tmp_path, chain([merge_record]))
    directory = tmp_path / "identity-transitions"
    shard_path = directory / "001.yaml"
    if damage == "missing_middle":
        write_chain(tmp_path, chain([merge_record, renewal(merge_record)]))
        shard_path.unlink()
    elif damage in {"extra_shard", "extra_yml"}:
        (directory / ("002.yml" if damage == "extra_yml" else "002.yaml")).write_text(
            "{}"
        )
    elif damage in {"short_sequence", "zero", "gap"}:
        name = {"short_sequence": "01", "zero": "000", "gap": "002"}[damage]
        shard_path.rename(directory / (name + ".yaml"))
    elif damage == "duplicate_sequence":
        (directory / "0001.yaml").write_bytes(shard_path.read_bytes())
    elif damage == "wrong_sequence":
        record = copy.deepcopy(merge_record)
        record["sequence"] = 2
        record["record_key"] = '["identity_transition",2]'
        write_chain(tmp_path, [pack(record)])
    with pytest.raises(
        ValueError,
        match=r"identity transition|Identity transition|Symlinks|Unexpected|Duplicate|Route|Invalid",
    ):
        load_transitions(tmp_path)


@pytest.mark.parametrize("count", [0, 1])
@pytest.mark.parametrize(
    "name",
    ["003.yaml.tmp-abc", "index.yaml.tmp", ".DS_Store", "README.txt", "nested/raw"],
)
def test_every_unexpected_file_is_rejected(
    tmp_path: Path, merge_record: dict[str, Any], count: int, name: str
) -> None:
    write_chain(tmp_path, chain([merge_record] * count))
    extra = tmp_path / "identity-transitions" / name
    extra.parent.mkdir(parents=True, exist_ok=True)
    extra.write_bytes(b"unfinished input")
    with pytest.raises(ValueError, match="Unexpected identity transition input path"):
        load_transitions(tmp_path)


def test_sequence_gap_is_rejected(tmp_path: Path) -> None:
    directory = tmp_path / "identity-transitions"
    directory.mkdir()
    for sequence in (1, 3):
        (directory / f"{sequence:03}.yaml").write_bytes(b"{}")
    with pytest.raises(ValueError, match="contiguous from 1"):
        _inventory(tmp_path)


def test_extra_zero_padding_remains_accepted(
    tmp_path: Path, merge_record: dict[str, Any]
) -> None:
    shards = chain([merge_record])
    write_chain(tmp_path, shards)
    directory = tmp_path / "identity-transitions"
    (directory / "001.yaml").rename(directory / "0001.yaml")
    assert load_transitions(tmp_path).shards[0].path == "identity-transitions/0001.yaml"


def test_after_key_must_match_inner_identity(
    tmp_path: Path, merge_record: dict[str, Any]
) -> None:
    record = copy.deepcopy(merge_record)
    record["updates"][0]["after"]["data"]["id"] = Y
    write_chain(tmp_path, chain([record]))
    with pytest.raises(ValueError, match="Invalid identity transition authored fields"):
        load_transitions(tmp_path)


@pytest.mark.parametrize("damage", ["directory", "file", "broken", "extra"])
def test_all_symlink_inputs_are_rejected(
    tmp_path: Path, merge_record: dict[str, Any], damage: str
) -> None:
    write_chain(tmp_path, chain([merge_record]))
    directory = tmp_path / "identity-transitions"
    if damage == "directory":
        directory.rename(tmp_path / "elsewhere")
        directory.symlink_to(tmp_path / "elsewhere", target_is_directory=True)
    elif damage == "file":
        path = directory / "001.yaml"
        backup = tmp_path / "copy"
        path.rename(backup)
        path.symlink_to(backup)
    elif damage == "broken":
        (directory / "001.yaml").unlink()
        (directory / "001.yaml").symlink_to(tmp_path / "absent")
    else:
        (directory / "extra").symlink_to(tmp_path / "absent")
    with pytest.raises(ValueError, match="Symlinks"):
        load_transitions(tmp_path)


@pytest.mark.parametrize(
    "value",
    [
        "format: 1\nidentity_transition_format: 1\n",
        "a: &a [*a]\n",
        "a: !custom text\n",
        "a: 1\n---\nb: 2\n",
        "a: .nan\n",
        "a: 1.0\n",
        "? [a, b]\n: value\n",
        "a: [\n",
    ],
)
def test_yaml_restrictions_and_sanitized_errors(tmp_path: Path, value: str) -> None:
    directory = tmp_path / "identity-transitions"
    directory.mkdir()
    (directory / "001.yaml").write_text(value)
    with pytest.raises(ValueError, match="Invalid identity transition"):
        load_transitions(tmp_path)


@pytest.mark.parametrize("size", [1_048_575, 1_048_576])
def test_strict_size_boundary(
    tmp_path: Path, merge_record: dict[str, Any], size: int
) -> None:
    write_chain(tmp_path, chain([merge_record]))
    path = tmp_path / "identity-transitions/001.yaml"
    content = path.read_bytes()
    path.write_bytes(content + b" " * (size - len(content)))
    if size == 1_048_575:
        assert len(load_transitions(tmp_path).shards) == 1
    else:
        with pytest.raises(ValueError, match="smaller than 1 MiB"):
            load_transitions(tmp_path)


@pytest.mark.parametrize(
    "damage",
    [
        "missing_format",
        "bool_format",
        "float_format",
        "unknown_format",
        "missing_kind",
        "extra",
    ],
)
def test_explicit_envelope_fields(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str,
) -> None:
    raw = pack(copy.deepcopy(merge_record))
    if damage in {"missing_format", "missing_kind"}:
        raw.pop("format" if damage == "missing_format" else "kind")
    elif damage == "extra":
        raw["extra"] = None
    else:
        raw["format"] = {
            "bool_format": True,
            "float_format": 1.0,
            "unknown_format": 2,
        }[damage]
    write_chain(tmp_path, [raw])
    error = (
        "Invalid identity transition YAML"
        if damage == "float_format"
        else "authored fields"
    )
    with pytest.raises(ValueError, match=error):
        load_transitions(tmp_path)


@pytest.mark.parametrize(
    "edits",
    [
        pytest.param([("records", "duplicate", None)], id="two_records"),
        pytest.param([("records", "set", [])], id="empty_records"),
        pytest.param([("decisions", "set", [])], id="decisions"),
        pytest.param([("default_decision_id", "set", "d:" + "0" * 64)], id="default"),
    ],
)
def test_one_transition_per_shard_without_decision_envelope(
    tmp_path: Path,
    merge_record: dict[str, Any],
    edits: list[tuple[str, str, Any]],
) -> None:
    shard = pack(copy.deepcopy(merge_record))
    mutate(shard, edits)
    write_chain(tmp_path, [shard])
    with pytest.raises(ValueError, match="authored fields"):
        load_transitions(tmp_path)


@pytest.mark.parametrize(
    "path",
    [
        ["kind"],
        ["action"],
        ["reverts"],
        ["sequence"],
        ["previous"],
        ["registry_basis"],
        ["review_context"],
        ["updates"],
        ["repairs"],
        ["routes"],
        ["evidence"],
        ["reason"],
        ["updates", 0, "before"],
        ["updates", 0, "after"],
        ["updates", 0, "allocation_anchor"],
        ["updates", 0, "before", "transition_key"],
        ["updates", 0, "before", "record_key"],
        ["repairs", 0, "printing_moves", 0, "faces", 0, "from_art_id"],
        ["repairs", 0, "printing_moves", 0, "faces", 0, "to_art_id"],
    ],
)
def test_required_nullable_and_complete_record_fields(
    tmp_path: Path,
    merge_record: dict[str, Any],
    path: list[str | int],
) -> None:
    record = copy.deepcopy(merge_record)
    target: Any = record
    for component in path[:-1]:
        target = target[component]
    del target[path[-1]]
    shard = pack(merge_record)
    shard["records"] = [record]
    write_chain(tmp_path, [shard])
    with pytest.raises(ValueError, match="authored fields"):
        load_transitions(tmp_path)


def mutate(value: dict[str, Any], edits: list[tuple[str, str, Any]]) -> None:
    for path, action, replacement in edits:
        components = [
            int(part) if part.isdecimal() else part for part in path.split(".")
        ]
        target: Any = value
        for component in components[:-1]:
            target = target[component]
        key = components[-1]
        if action == "set":
            target[key] = copy.deepcopy(replacement)
        elif action == "duplicate":
            target[key].append(copy.deepcopy(target[key][0]))
        elif action == "append_sorted":
            target[key].append(copy.deepcopy(replacement))
            target[key].sort(key=wire)
        elif action == "append":
            target[key].append(copy.deepcopy(replacement))
        elif action == "reverse":
            target[key].reverse()
        else:
            del target[key]


@pytest.mark.parametrize(
    "edits",
    [
        pytest.param([("extra", "set", "private input sentinel")], id="unknown_field"),
        pytest.param([("record_key", "set", '[ "identity_transition", 1 ]')], id="key"),
        pytest.param([("sequence", "set", True)], id="bool_sequence"),
        pytest.param([("updates", "set", [])], id="empty_updates"),
        pytest.param([("updates", "reverse", None)], id="updates_order"),
        pytest.param([("updates", "duplicate", None)], id="duplicate_update"),
        pytest.param(
            [("updates.0.before.record_key", "set", "other")], id="before_key"
        ),
        pytest.param(
            [("updates.0.after.record_key", "set", "card:" + A)], id="after_key"
        ),
        pytest.param(
            [
                ("updates.3.before", "set", None),
                ("updates.3.allocation_anchor", "set", "new-printing"),
            ],
            id="new_printing",
        ),
        pytest.param([("updates.2.after", "set", None)], id="deactivate_card"),
        pytest.param(
            [("updates.0.allocation_anchor", "set", "reallocated")],
            id="existing_anchor",
        ),
        pytest.param([("updates.0.before", "set", None)], id="new_no_anchor"),
        pytest.param([("updates.0.after.data.extra", "set", None)], id="after_extra"),
        pytest.param(
            [("updates.0.after.data.classification", "set", "invented")],
            id="after_bad_enum",
        ),
        pytest.param(
            [
                (
                    "reverts",
                    "set",
                    {"record_key": "other", "record_hash": "sha256:" + "0" * 64},
                )
            ],
            id="apply_reverts",
        ),
        pytest.param(
            [("action", "set", "revert"), ("repairs", "set", [])], id="revert_no_target"
        ),
        pytest.param(
            [
                ("action", "set", "revert"),
                (
                    "reverts",
                    "set",
                    {"record_key": "other", "record_hash": "sha256:" + "0" * 64},
                ),
            ],
            id="revert_repairs",
        ),
        pytest.param([("evidence", "set", [])], id="empty_evidence"),
        pytest.param([("evidence", "duplicate", None)], id="duplicate_evidence"),
        pytest.param(
            [
                (
                    "evidence",
                    "append",
                    {
                        "batch_id": "sha256:" + "4" * 64,
                        "source_version_id": "src:v1:" + "7" * 64,
                        "locator": "a-first",
                        "role": "identity_observation",
                    },
                )
            ],
            id="evidence_order",
        ),
        pytest.param(
            [("evidence.0.batch_id", "set", "sha256:" + "0" * 64)], id="missing_batch"
        ),
        pytest.param(
            [("review_context.source_batches", "duplicate", None)],
            id="duplicate_batches",
        ),
        pytest.param([("evidence.0.store_id", "set", "/private")], id="private_store"),
        pytest.param(
            [("review_context.context.dependencies", "set", [])],
            id="empty_dependencies",
        ),
        pytest.param(
            [("review_context.context.configuration", "set", "{ }")],
            id="bad_configuration",
        ),
        pytest.param([("repairs.0.printing_moves", "set", [])], id="empty_moves"),
        pytest.param([("repairs.0.retire_old", "set", False)], id="merge_not_retired"),
        pytest.param([("repairs.0.new_card_ids", "append", C)], id="merge_two_targets"),
        pytest.param([("repairs.0.kind", "set", "split")], id="split_one_target"),
        pytest.param(
            [
                ("repairs.0.kind", "set", "split"),
                ("repairs.0.new_card_ids", "append", C),
                ("repairs.0.retire_old", "set", False),
            ],
            id="split_not_retired",
        ),
        pytest.param(
            [
                ("repairs.0.kind", "set", "split"),
                ("repairs.0.new_card_ids", "append", C),
            ],
            id="split_empty_destination",
        ),
        pytest.param(
            [
                ("repairs.0.kind", "set", "reassign_printing"),
                ("repairs.0.printing_moves", "duplicate", None),
            ],
            id="reassign_two_printings",
        ),
        pytest.param(
            [("repairs.0.printing_moves.0.from_card_id", "set", C)], id="move_old"
        ),
        pytest.param(
            [("repairs.0.printing_moves.0.to_card_id", "set", C)], id="move_destination"
        ),
        pytest.param(
            [("repairs.0.printing_moves.0.to_card_id", "set", A)], id="move_self"
        ),
        pytest.param(
            [("repairs.0.printing_moves.0.faces.0.to_art_id", "set", None)],
            id="known_art_null",
        ),
        pytest.param(
            [("repairs.0.printing_moves.0.faces.0.source_index", "set", 1)],
            id="source_index",
        ),
        pytest.param(
            [("repairs.0.face_moves.0.to_face_ids", "set", [FA])], id="face_self"
        ),
        pytest.param(
            [("repairs.0.face_moves.0.to_face_ids", "duplicate", None)],
            id="face_duplicate_target",
        ),
        pytest.param(
            [
                (
                    "repairs.0.face_moves",
                    "append_sorted",
                    {"from_face_id": FA, "to_face_ids": [FC]},
                )
            ],
            id="duplicate_face",
        ),
        pytest.param(
            [
                (
                    "repairs.0.art_moves",
                    "append_sorted",
                    {"from_art_id": X, "targets": [], "remaining_uses": []},
                )
            ],
            id="duplicate_art",
        ),
    ],
)
def test_resigned_structural_counterexamples(
    tmp_path: Path,
    merge_record: dict[str, Any],
    edits: list[tuple[str, str, Any]],
) -> None:
    record = copy.deepcopy(merge_record)
    mutate(record, edits)
    write_chain(tmp_path, [pack(record)])
    with pytest.raises(
        ValueError, match=r"authored fields|Transaction|transfers"
    ) as caught:
        load_transitions(tmp_path)
    assert "private input sentinel" not in str(caught.value)


@pytest.mark.parametrize(
    "damage",
    ["duplicate_printing", "duplicate_old", "duplicate_repair_id", "repair_order"],
)
def test_transaction_uniqueness(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str,
) -> None:
    record = copy.deepcopy(merge_record)
    repair = record["repairs"][0]
    if damage == "duplicate_printing":
        move = copy.deepcopy(repair["printing_moves"][0])
        move["faces"][0]["to_art_id"] = X
        repair["printing_moves"].append(move)
        repair["printing_moves"].sort(key=wire)
    else:
        extra = copy.deepcopy(repair)
        if damage == "duplicate_old":
            extra["id"] = "repair:second"
        elif damage == "repair_order":
            extra["id"] = "repair:aaa"
        record["repairs"].append(extra)
    write_chain(tmp_path, [pack(record)])
    with pytest.raises(ValueError, match=r"authored fields|Transaction"):
        load_transitions(tmp_path)


@pytest.mark.parametrize("damage", ["key", "hash", "missing", "self", "first"])
def test_exact_previous_chain(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str,
) -> None:
    shards = chain([merge_record, renewal(merge_record)])
    record = shards[1]["records"][0]
    if damage == "first":
        record = shards[0]["records"][0]
        record["previous"] = reference(shards[1])
        shards[0] = pack(record)
    else:
        if damage == "missing":
            record["previous"] = None
        elif damage == "self":
            record["previous"] = reference(shards[1])
        else:
            field = {"key": "record_key", "hash": "record_hash"}[damage]
            record["previous"][field] = {
                "key": "other",
                "hash": "sha256:" + "0" * 64,
            }[damage]
        shards[1] = pack(record)
    write_chain(tmp_path, shards)
    with pytest.raises(ValueError, match="exact chain tail"):
        load_transitions(tmp_path)


@pytest.mark.parametrize("damage", ["key", "hash", "future", "root", "stale"])
def test_before_matches_latest_exact_producer(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str,
) -> None:
    shards = chain([merge_record, renewal(merge_record), renewal(merge_record)])
    record = shards[2]["records"][0]
    before = record["updates"][0]["before"]
    if damage == "stale":
        record["updates"][0]["before"] = shards[1]["records"][0]["updates"][0]["before"]
    elif damage == "future":
        before["transition_key"] = '["identity_transition",4]'
    elif damage == "root":
        before["transition_key"] = None
    else:
        before[{"key": "transition_key", "hash": "record_hash"}[damage]] = {
            "key": "other",
            "hash": "sha256:" + "0" * 64,
        }[damage]
    shards[2] = pack(record)
    write_chain(tmp_path, shards)
    with pytest.raises(ValueError, match=r"stale|producer"):
        load_transitions(tmp_path)


def test_repair_id_cannot_be_reused_across_transactions(
    tmp_path: Path,
    merge_record: dict[str, Any],
) -> None:
    write_chain(tmp_path, chain([merge_record, merge_record]))
    with pytest.raises(ValueError, match="repair ID cannot be reused"):
        load_transitions(tmp_path)


def new_card(anchor: str) -> dict[str, Any]:
    identifier = (
        "c:"
        + uuid5(
            UUID("e304714a-f18c-5fb6-a987-222988ffbb7a"),
            "c\0identity-transition-v1\0" + anchor,
        ).hex
    )
    card = entry(
        "card",
        identifier,
        {"layout": "single", "identity_state": "confirmed", "home_set_id": "EXAMPLE"},
    )
    return {
        "target_key": record_key(card),
        "before": None,
        "after": card,
        "allocation_anchor": anchor,
    }


@pytest.mark.parametrize("damage", [None, "id", "anchor", "key"])
def test_allocation_recipe_and_never_reuse(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str | None,
) -> None:
    record = renewal(merge_record)
    record["updates"] = [new_card("synthetic-card")]
    if damage == "id":
        record["updates"][0]["after"]["data"]["id"] = A

        record["updates"][0]["target_key"] = "card:" + A
    shards = chain([record])
    if damage in {"anchor", "key"}:
        later = copy.deepcopy(record)
        if damage == "key":
            later["updates"][0]["allocation_anchor"] = "another-anchor"
        shards = chain([record, later])
        shards[1]["records"][0]["updates"][0]["before"] = None
        shards[1] = pack(shards[1]["records"][0])
    write_chain(tmp_path, shards)
    if damage is None:
        assert len(load_transitions(tmp_path).shards) == 1
    else:
        with pytest.raises(ValueError, match="allocation"):
            load_transitions(tmp_path)


def test_allocation_anchor_cannot_be_reused_for_a_different_kind(
    tmp_path: Path, merge_record: dict[str, Any]
) -> None:
    anchor = "shared-synthetic-anchor"
    card_record = renewal(merge_record)
    card_record["updates"] = [new_card(anchor)]
    face_id = (
        "f:"
        + uuid5(
            UUID("e304714a-f18c-5fb6-a987-222988ffbb7a"),
            "f\0identity-transition-v1\0" + anchor,
        ).hex
    )
    face_record = renewal(merge_record)
    face = entry("face", face_id, {"card_id": A, "ordinal": 0, "side": "front"})
    face_record["updates"] = [
        {
            "target_key": record_key(face),
            "before": None,
            "after": face,
            "allocation_anchor": anchor,
        }
    ]
    write_chain(tmp_path, chain([card_record, face_record]))
    with pytest.raises(ValueError, match="allocation anchor or key cannot be reused"):
        load_transitions(tmp_path)


def test_new_allocation_key_cannot_reuse_a_previously_updated_key(
    tmp_path: Path, merge_record: dict[str, Any]
) -> None:
    allocation = new_card("unused-synthetic-anchor")
    existing = copy.deepcopy(allocation)
    existing["allocation_anchor"] = None
    existing["before"] = {
        "transition_key": None,
        "record_key": existing["target_key"],
        "record_hash": checksum(existing["after"]),
    }
    original = renewal(merge_record)
    original["updates"] = [existing]
    later = renewal(merge_record)
    later["updates"] = [allocation]
    shards = chain([original, later])
    shards[1]["records"][0]["updates"][0]["before"] = None
    shards[1] = pack(shards[1]["records"][0])
    write_chain(tmp_path, shards)
    with pytest.raises(ValueError, match="allocation anchor or key cannot be reused"):
        load_transitions(tmp_path)


def test_revert_action_guard_independently_of_repairs(
    merge_record: dict[str, Any],
) -> None:
    target = Shard.model_validate_json(wire(pack(merge_record)))
    # Retain repairs to isolate the action guard from the independent repairs guard.
    target_record = target.records[0].model_copy(update={"action": "revert"})
    target = target.model_copy(update={"records": (target_record,)})
    target_ref = Reference(
        record_key=target_record.record_key,
        record_hash=checksum(target_record.model_dump(mode="json", round_trip=True)),
    )
    revert = target_record.model_copy(update={"repairs": (), "reverts": target_ref})
    with pytest.raises(
        ValueError, match="exact earlier, unreverted repair transaction"
    ):
        _revert(revert, {target_record.record_key: target}, set())


@pytest.mark.parametrize(
    "damage",
    [None, "hash", "future", "renewal", "partial", "twice", "revert"],
)
def test_revert_reference_structure_only(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str | None,
) -> None:
    original = (
        renewal(merge_record) if damage == "renewal" else copy.deepcopy(merge_record)
    )
    shards = chain([original])
    revert = renewal(merge_record)
    revert["action"] = "revert"
    revert["reverts"] = reference(shards[0])
    if damage in {"hash", "future"}:
        field = {"hash": "record_hash", "future": "record_key"}[damage]
        revert["reverts"][field] = {
            "hash": "sha256:" + "0" * 64,
            "future": '["identity_transition",3]',
        }[damage]
    elif damage == "partial":
        revert["updates"].pop()
    records = [original, revert]
    if damage in {"twice", "revert"}:
        later = copy.deepcopy(revert)
        if damage == "revert":
            later["reverts"] = reference(chain(records)[1])
        records.append(later)
    write_chain(tmp_path, chain(records))
    if damage is None:
        assert len(load_transitions(tmp_path).shards) == 2
    else:
        with pytest.raises(ValueError, match="Revert"):
            load_transitions(tmp_path)


def route_state(key: str, aliases: list[dict[str, str]]) -> dict[str, Any]:
    return {
        "canonical": {"namespace": "official", "route_key": key},
        "aliases": aliases,
    }


@pytest.mark.parametrize(
    "damage",
    [
        None,
        "unchanged",
        "canonical_alias",
        "repeated_key",
        "order",
        "duplicate_printing",
        "reserved",
        "provisional",
        "reason",
    ],
)
def test_complete_route_state_shapes(
    tmp_path: Path,
    merge_record: dict[str, Any],
    damage: str | None,
) -> None:
    record = renewal(merge_record)
    alias = {
        "namespace": "official",
        "route_key": "EXAMPLE-001",
        "reason": "renumbered",
    }
    change: dict[str, Any] = {
        "printing_id": P,
        "before": route_state("EXAMPLE-001", []),
        "after": route_state("EXAMPLE-002", [alias]),
    }
    record["routes"] = [change]
    if damage == "unchanged":
        change["after"] = copy.deepcopy(change["before"])
    elif damage == "canonical_alias":
        alias["route_key"] = "EXAMPLE-002"
    elif damage == "repeated_key":
        change["after"]["aliases"].append({**alias, "reason": "merged"})
        change["after"]["aliases"].sort(key=wire)
    elif damage == "order":
        change["after"]["aliases"].append({**alias, "route_key": "EXAMPLE-000"})
    elif damage == "duplicate_printing":
        record["routes"].append(
            {**copy.deepcopy(change), "before": route_state("EXAMPLE-003", [])}
        )
        record["routes"].sort(key=wire)
    elif damage == "reserved":
        change["after"]["canonical"]["route_key"] = "_provisional"
    elif damage == "provisional":
        change["after"]["canonical"] = {"namespace": "provisional", "route_key": "01"}
    elif damage == "reason":
        alias["reason"] = "provisional_to_official"
    write_chain(tmp_path, [pack(record)])
    if damage is None:
        loaded = load_transitions(tmp_path).shards[0].envelope().records[0]
        assert loaded.routes[0].after.aliases[0].reason == "renumbered"
    else:
        with pytest.raises(
            ValueError,
            match=r"authored fields|Transaction",
        ):
            load_transitions(tmp_path)


def test_exact_yaml_bytes_are_preserved_separately(
    tmp_path: Path,
    merge_record: dict[str, Any],
) -> None:
    write_chain(tmp_path, [pack(copy.deepcopy(merge_record))])
    path = tmp_path / "identity-transitions/001.yaml"
    exact = b"# synthetic comment\n" + path.read_bytes() + b"\n"
    path.write_bytes(exact)
    files = read_transition_files(tmp_path)
    assert files.shards[0].exact_content == exact
    assert files.shards[0].content != exact
    assert files.shards[0].content_hash == checksum(pack(merge_record))


@pytest.mark.parametrize("planner", ["append", "relayout"])
def test_legacy_planners_cannot_bypass_guard_with_loaded_inputs(
    tmp_path: Path,
    merge_record: dict[str, Any],
    planner: str,
) -> None:
    write_chain(tmp_path, [pack(copy.deepcopy(merge_record))])
    if planner == "append":
        with pytest.raises(ValueError, match="effective projection support"):
            plan_files(tmp_path, [], loaded=(RegistryIndex(), {}))
    else:
        with pytest.raises(ValueError, match="effective projection support"):
            relayout(tmp_path, [])


def test_before_can_reference_deactivated_record_hash(
    tmp_path: Path,
    merge_record: dict[str, Any],
) -> None:
    first = renewal(merge_record)
    key = "region_mapping_review:" + A
    first["updates"] = [
        {
            "target_key": key,
            "before": {
                "transition_key": None,
                "record_key": key,
                "record_hash": "sha256:" + "0" * 64,
            },
            "after": None,
            "allocation_anchor": None,
        }
    ]
    shards = chain([first, copy.deepcopy(first)])
    write_chain(tmp_path, shards)
    loaded = load_transitions(tmp_path)
    before = loaded.shards[1].envelope().records[0].updates[0].before
    assert before is not None
    assert (
        before.record_hash
        == "sha256:74234e98afe7498fb5daf1f36ac2d78acc339464f950703b8c019892f982b90b"
    )


def test_sequence_uses_numeric_order_past_three_digits(
    tmp_path: Path,
) -> None:
    directory = tmp_path / "identity-transitions"
    directory.mkdir()
    for sequence in range(1, 1002):
        (directory / f"{sequence:03}.yaml").touch()
    ordered = _inventory(tmp_path)
    assert ordered[998:1001] == [
        "identity-transitions/999.yaml",
        "identity-transitions/1000.yaml",
        "identity-transitions/1001.yaml",
    ]


def test_checked_model_cannot_normalize_canonical_input() -> None:

    class NormalizingRecord(RecordData):
        value: str

        @field_validator("value")
        @classmethod
        def normalize(cls, value: str) -> str:
            return value.strip()

    with pytest.raises(ValueError, match="normalization changed canonical input"):
        checked_model(NormalizingRecord, {"value": " synthetic "})
    assert checked_model(NormalizingRecord, {"value": "synthetic"}).value == "synthetic"

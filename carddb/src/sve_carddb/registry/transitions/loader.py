"""Validate transition chains and historical references without replay."""

from typing import TYPE_CHECKING

from sve_carddb.registry.build import permanent_id
from sve_carddb.registry.inputs import digest
from sve_carddb.registry.transitions.files import TransitionFiles, read_transition_files
from sve_carddb.registry.transitions.models import Before, Reference, Shard, Transition

if TYPE_CHECKING:
    from pathlib import Path


def _reference(shard: Shard) -> Reference:
    record = shard.records[0]
    return Reference(
        record_key=record.record_key,
        record_hash=digest(record.model_dump(mode="json")),
    )


def _repair_uniqueness(record: Transition, known: set[str]) -> None:
    olds = [repair.old_card_id for repair in record.repairs]
    printings = [
        move.printing_id for repair in record.repairs for move in repair.printing_moves
    ]
    if len(set(olds)) != len(olds) or len(set(printings)) != len(printings):
        raise ValueError("Transaction may repair each old card and printing only once")
    for repair in record.repairs:
        if repair.id in known:
            raise ValueError("Identity repair ID cannot be reused")
        known.add(repair.id)
        faces = [move.from_face_id for move in repair.face_moves]
        arts = [move.from_art_id for move in repair.art_moves]
        if len(set(faces)) != len(faces) or len(set(arts)) != len(arts):
            raise ValueError("Repair transfers must list each old face/art only once")
    routes = [update.printing_id for update in record.routes]
    if len(set(routes)) != len(routes):
        raise ValueError("Transaction may change each printing route only once")


def _before_refs(record: Transition, latest: dict[str, Before]) -> None:
    for update in record.updates:
        before = update.before
        if before is None:
            continue
        if before.transition_key is None and update.target_key not in latest:
            continue
        if latest.get(update.target_key) != before:
            raise ValueError(
                "Transition before reference is stale or has no exact producer"
            )


def _allocations(record: Transition, anchors: set[str], keys: set[str]) -> None:
    for update in record.updates:
        if update.before is not None:
            continue
        anchor = update.allocation_anchor
        assert anchor is not None
        if anchor in anchors or update.target_key in keys:
            raise ValueError("Transition allocation anchor or key cannot be reused")
        assert update.after is not None
        prefix = {"card": "c", "face": "f", "art": "a"}[update.after.kind]
        if update.after.data["id"] != permanent_id(
            prefix, "identity-transition-v1\0" + anchor
        ):
            raise ValueError("Transition allocation ID disagrees with its fixed recipe")
        anchors.add(anchor)
    keys.update(update.target_key for update in record.updates)


def _revert(record: Transition, history: dict[str, Shard], reverted: set[str]) -> None:
    if record.reverts is None:
        return
    target = history.get(record.reverts.record_key)
    if (
        target is None
        or record.reverts != _reference(target)
        or target.records[0].action != "apply"
        or not target.records[0].repairs
        or record.reverts.record_key in reverted
    ):
        raise ValueError(
            "Revert must name an exact earlier, unreverted repair transaction"
        )
    if tuple(update.target_key for update in record.updates) != tuple(
        update.target_key for update in target.records[0].updates
    ):
        raise ValueError("Revert must cover the entire target transaction")
    reverted.add(record.reverts.record_key)


def load_transitions(root: Path) -> TransitionFiles:
    """Read the whole sequence, validating its chain and internal historical references.

    This boundary does not verify Git registry bases, archived evidence, effective
    ownership, move completeness, or revert dependencies. It never writes data or
    authorizes a build/publication. Replay must perform those checks separately.
    """
    files = read_transition_files(root)
    history: dict[str, Shard] = {}
    previous: Reference | None = None
    repairs: set[str] = set()
    anchors: set[str] = set()
    keys: set[str] = set()
    reverted: set[str] = set()
    latest: dict[str, Before] = {}
    for loaded in files.shards:
        shard = loaded.envelope()
        record = shard.records[0]
        if record.previous != previous:
            raise ValueError(
                "Identity transition previous must match the exact chain tail"
            )
        _before_refs(record, latest)
        _allocations(record, anchors, keys)
        _repair_uniqueness(record, repairs)
        _revert(record, history, reverted)
        history[record.record_key] = shard
        previous = _reference(shard)
        for update in record.updates:
            latest[update.target_key] = Before(
                transition_key=record.record_key,
                record_key=update.target_key,
                record_hash=digest(
                    None
                    if update.after is None
                    else update.after.model_dump(mode="json")
                ),
            )
    return files

"""Validate complete face/art moves and the effective repair graph before commit."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.domains.registry.records import (
    ArtData,
    CardData,
    FaceData,
    PrintingData,
)

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.domains.registry.transitions.models import (
        ArtTransfer,
        FaceMove,
        Repair,
        Transition,
    )
    from sve_carddb.domains.registry.transitions.state import Entity


def typed[T](records: Mapping[str, Entity], model: type[T]) -> dict[str, T]:
    """Select typed effective data, never treating inactive history as current."""
    return {
        key: data
        for key, item in records.items()
        if isinstance(data := item.data(), model)
    }


def check_graph(repairs: tuple[Repair, ...]) -> None:
    """Check the complete apply graph, including earlier nonretiring reassign edges."""
    edges: dict[str, set[str]] = defaultdict(set)
    for repair in repairs:
        edges[repair.old_card_id].update(repair.new_card_ids)
    pending = set(edges) | {target for targets in edges.values() for target in targets}
    while pending:
        leaves = {node for node in pending if not (edges[node] & pending)}
        if not leaves:
            raise ValueError("Effective identity repair graph contains a cycle")
        pending -= leaves


def validate_moves(
    before: Mapping[str, Entity], after: Mapping[str, Entity], record: Transition
) -> None:
    """Every parent/face change and retirement must be explained by exact moves."""
    old_printings = typed(before, PrintingData)
    new_printings = typed(after, PrintingData)
    declared = {
        "printing:" + move.printing_id: move
        for repair in record.repairs
        for move in repair.printing_moves
    }
    changed = {
        key
        for key, old in old_printings.items()
        if old.card_id != new_printings[key].card_id
    }
    if set(declared) != changed:
        raise ValueError("Repairs must exactly explain every changed printing parent")
    old_cards = typed(before, CardData)
    new_cards = typed(after, CardData)
    retired = {
        key
        for key, old in old_cards.items()
        if old.identity_state != "retired"
        and new_cards[key].identity_state == "retired"
    }
    if retired != {"card:" + r.old_card_id for r in record.repairs if r.retire_old}:
        raise ValueError("Repairs must exactly explain card retirement")
    for key, old in old_printings.items():
        new = new_printings[key]
        if key not in declared and old.source_face_map != new.source_face_map:
            raise ValueError("Face mapping change requires a printing parent repair")
    expected_arts = _art_uses(before)
    for repair in record.repairs:
        _repair(before, after, repair)
        _transfer_arts(before, after, repair, expected_arts)
    for key, data in typed(after, ArtData).items():
        actual = {(u.printing_id, u.face_id) for u in data.uses}
        if actual != expected_arts.get(key, set()):
            raise ValueError("Art uses change is not exactly explained by transfers")


def _repair(
    before: Mapping[str, Entity], after: Mapping[str, Entity], repair: Repair
) -> None:
    old_card = before["card:" + repair.old_card_id].data()
    assert isinstance(old_card, CardData)
    if old_card.identity_state == "retired":
        raise ValueError("Retired card cannot be reused by apply")
    old_printings = typed(before, PrintingData)
    new_printings = typed(after, PrintingData)
    owned = {p.id for p in old_printings.values() if p.card_id == repair.old_card_id}
    moved = {m.printing_id for m in repair.printing_moves}
    if not moved <= owned or (repair.kind != "reassign_printing" and moved != owned):
        raise ValueError(
            "Repair printing move coverage differs from original ownership"
        )
    remaining = {
        p.id for p in new_printings.values() if p.card_id == repair.old_card_id
    }
    if repair.retire_old != (not remaining):
        raise ValueError("Repair retirement must match remaining printing ownership")
    faces = typed(before, FaceData)
    old_faces = {f.id for f in faces.values() if f.card_id == repair.old_card_id}
    if {m.from_face_id for m in repair.face_moves} != old_faces:
        raise ValueError("Repair must list every old face, including unused faces")
    _faces(after, repair, old_printings, new_printings)


def _faces(
    after: Mapping[str, Entity],
    repair: Repair,
    old_printings: dict[str, PrintingData],
    new_printings: dict[str, PrintingData],
) -> None:
    transfers = {m.from_face_id: set(m.to_face_ids) for m in repair.face_moves}
    used: dict[str, set[str]] = defaultdict(set)
    for move in repair.printing_moves:
        key = "printing:" + move.printing_id
        old, new = old_printings[key], new_printings[key]
        if (old.card_id, new.card_id) != (move.from_card_id, move.to_card_id):
            raise ValueError("Printing move parents disagree with effective records")
        expected = tuple((m.source_index, m.from_face_id) for m in move.faces)
        actual = tuple((m.source_index, m.face_id) for m in old.source_face_map)
        destinations = tuple((m.source_index, m.to_face_id) for m in move.faces)
        if expected != actual or destinations != tuple(
            (m.source_index, m.face_id) for m in new.source_face_map
        ):
            raise ValueError("Printing move must cover exact complete source face maps")
        for face in move.faces:
            used[face.from_face_id].add(face.to_face_id)
            target = after["face:" + face.to_face_id].data()
            assert isinstance(target, FaceData)
            if (
                target.card_id != move.to_card_id
                or face.to_face_id not in transfers[face.from_face_id]
            ):
                raise ValueError(
                    "Face transfer destination has wrong parent or is missing"
                )
    for source, targets in transfers.items():
        if used[source] and targets != used[source]:
            raise ValueError("Face transfer contains unused or missing destinations")
        for target_id in targets:
            target = after["face:" + target_id].data()
            assert isinstance(target, FaceData)
            if target.card_id not in repair.new_card_ids:
                raise ValueError("Face transfer destination is outside repair cards")


def _art_uses(records: Mapping[str, Entity]) -> dict[str, set[tuple[str, str]]]:
    return {
        key: {(u.printing_id, u.face_id) for u in data.uses}
        for key, data in typed(records, ArtData).items()
    }


def _transfer_arts(
    before: Mapping[str, Entity],
    after: Mapping[str, Entity],
    repair: Repair,
    expected: dict[str, set[tuple[str, str]]],
) -> None:
    arts = typed(before, ArtData)
    old_arts = {a.id for a in arts.values() if a.card_id == repair.old_card_id}
    if {m.from_art_id for m in repair.art_moves} != old_arts:
        raise ValueError("Repair must list all old art, including unused candidates")
    moved_faces = {
        (move.printing_id, face.from_face_id): face
        for move in repair.printing_moves
        for face in move.faces
    }
    by_use = {(u.printing_id, u.face_id): a.id for a in arts.values() for u in a.uses}
    for old_use, face in moved_faces.items():
        if face.from_art_id != by_use.get(old_use):
            raise ValueError("Printing move art does not match actual old use")
        if face.from_art_id is None and face.to_art_id is not None:
            raise ValueError("Unknown art cannot become known through an identity move")
    for transfer in repair.art_moves:
        key = "art:" + transfer.from_art_id
        old = arts[key]
        original = {(u.printing_id, u.face_id) for u in old.uses}
        remaining = original - moved_faces.keys()
        if {(u.printing_id, u.face_id) for u in transfer.remaining_uses} != remaining:
            raise ValueError("Art remaining uses do not match unmoved printing uses")
        targets = _targets(after, transfer, moved_faces, original, remaining)
        expected[key].difference_update(original - remaining)
        for identifier, uses in targets.items():
            expected.setdefault("art:" + identifier, set()).update(uses)


def _targets(
    after: Mapping[str, Entity],
    transfer: ArtTransfer,
    moved_faces: dict[tuple[str, str], FaceMove],
    original: set[tuple[str, str]],
    remaining: set[tuple[str, str]],
) -> dict[str, set[tuple[str, str]]]:
    targets: dict[str, set[tuple[str, str]]] = {}
    partition: set[tuple[str, str]] = set()
    for target in transfer.targets:
        if target.to_art_id in targets:
            raise ValueError("Art transfer target must be listed once")
        target_data = after["art:" + target.to_art_id].data()
        assert isinstance(target_data, ArtData)
        if target_data.face_id != target.to_face_id:
            raise ValueError("Art target belongs to another face")
        source_uses: set[tuple[str, str]] = set()
        for use in target.uses:
            source = next(
                (
                    s
                    for s, f in moved_faces.items()
                    if s[0] == use.printing_id and f.to_face_id == use.face_id
                ),
                None,
            )
            if source is None or source not in original or source in partition:
                raise ValueError(
                    "Art transfer use partition is missing, duplicated or foreign"
                )
            face = moved_faces[source]
            if (face.to_art_id, face.to_face_id) != (
                target.to_art_id,
                target.to_face_id,
            ):
                raise ValueError(
                    "Art move destination disagrees with printing face art"
                )
            partition.add(source)
            source_uses.add((use.printing_id, use.face_id))
        if not source_uses:
            raise ValueError("Art transfer target requires an actual moved use")
        targets[target.to_art_id] = source_uses
    if partition != original - remaining:
        raise ValueError(
            "Art transfer targets and remaining uses must partition old uses"
        )
    return targets

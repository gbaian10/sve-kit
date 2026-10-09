"""Read-only effective apply replay; never allocate, persist or authorize publication."""

from dataclasses import replace
from typing import TYPE_CHECKING, Protocol

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.registry.records import ArtData, CardData, PrintingData
from sve_carddb.domains.registry.snapshot import _printing_evidence
from sve_carddb.domains.registry.storage import Entry, RegistryFiles, read_base_files
from sve_carddb.domains.registry.transitions.loader import load_transitions
from sve_carddb.domains.registry.transitions.ownership import (
    check_graph,
    typed,
    validate_moves,
)
from sve_carddb.domains.registry.transitions.routing import (
    RouteFact,
    RouteProjection,
    check_updates,
    derive,
    project,
)
from sve_carddb.domains.registry.transitions.state import (
    EffectiveRegistry,
    Entity,
    entity,
    frozen_records,
)
from sve_carddb.domains.registry.validate import (
    check_cursors,
    validate_collections,
    validate_structure,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.transitions.models import (
        RegistryBasis,
        Repair,
        Transition,
    )


class ReplayInputs(Protocol):
    def registry(self, basis: RegistryBasis) -> RegistryFiles:
        """Read original files from the exact immutable authored Git revision.

        The adapter verifies revision identity and safe Git blob paths, and returns
        the complete ids/registry closure. Never serve the current checkout as an
        earlier basis. This core independently checks hashes and append-only bases.
        """
        ...

    def routes(self, transition: Transition | None) -> tuple[RouteFact, ...]:
        """Extract complete exact route facts from independent frozen source inputs.

        None selects the first basis's source routes; a transition selects its
        pinned review_context/evidence. Must not copy authored expected routes.
        Historical F1/archive reconstruction belongs to the production adapter.
        """
        ...


_ALLOWED = {
    "card": {"identity_state"},
    "face": set(),
    "printing": {"card_id", "source_face_map", "observation", "cross_region_review"},
    "art": {"uses", "observation"},
    "region_mapping_review": {
        "as_of",
        "coverage_scope",
        "coverage_hash",
        "observations",
    },
    "card_related": {"evidence"},
}


def _base(files: RegistryFiles) -> dict[str, Entity]:
    records: dict[str, Entity] = {}
    entries: list[Entry] = []
    for loaded in files.shards:
        shard = loaded.envelope()
        if (
            digest(canonical(shard.model_dump(mode="json", round_trip=True)))
            != loaded.content_hash
        ):
            raise ValueError(
                "Registry envelope serialization changed canonical content"
            )
        for entry in shard.records:
            entries.append(entry)
            records[entry.record_key] = entity(entry, entry.record_key, None)
            records[entry.record_key].data()
    validate_structure(entries)
    check_cursors(files.index().next_int_id, entries)
    return records


def _append(
    previous: RegistryFiles | None,
    files: RegistryFiles,
    records: dict[str, Entity],
    reserved: set[str],
) -> None:
    base = _base(files)
    if previous is not None:
        old_shards = {s.path: s for s in previous.shards}
        new_shards = {s.path: s for s in files.shards}
        if any(
            path not in new_shards
            or (new_shards[path].content, new_shards[path].exact_content)
            != (shard.content, shard.exact_content)
            for path, shard in old_shards.items()
        ):
            raise ValueError("Registry bases must retain exact old shards")
        old_entries = {
            e.record_key for s in previous.shards for e in s.envelope().records
        }
        base = {key: item for key, item in base.items() if key not in old_entries}
    if base.keys() & records.keys():
        raise ValueError(
            "Appended registry ID collides with transition allocation history"
        )
    records.update(base)
    reserved.update(base)


def _stable(old: Entry, new: Entry) -> None:
    if (old.record_key, old.kind, old.owner) != (new.record_key, new.kind, new.owner):
        raise ValueError("Transition cannot change stable record key, kind or owner")
    allowed = _ALLOWED[old.kind]
    if {k: v for k, v in old.data.items() if k not in allowed} != {
        k: v for k, v in new.data.items() if k not in allowed
    }:
        raise ValueError("Transition changes a permanent field outside its contract")
    if old.kind == "card" and old.data["identity_state"] == "retired" and new != old:
        raise ValueError("Only named revert may restore a retired card")


def _allocation_dependencies(record: Transition) -> None:
    destinations = {"card:" + cid for r in record.repairs for cid in r.new_card_ids}
    destinations.update(
        "face:" + fid
        for r in record.repairs
        for move in r.face_moves
        for fid in move.to_face_ids
    )
    destinations.update(
        "art:" + target.to_art_id
        for r in record.repairs
        for move in r.art_moves
        for target in move.targets
    )
    for update in record.updates:
        if update.before is None:
            if update.target_key not in destinations:
                raise ValueError(
                    "New identity allocation requires an explicit repair destination"
                )
            assert update.after is not None
            if (
                update.after.kind == "card"
                and update.after.data["identity_state"] == "retired"
            ):
                raise ValueError("Apply cannot allocate a retired destination card")


def _updates(
    records: dict[str, Entity], record: Transition, reserved: set[str]
) -> dict[str, Entity]:
    candidate = dict(records)
    _allocation_dependencies(record)
    for update in record.updates:
        old = records.get(update.target_key)
        if update.before is None:
            if update.target_key in reserved:
                raise ValueError(
                    "Permanent ID cannot reuse original or historical allocation"
                )
        elif old is None or old.reference != update.before:
            raise ValueError(
                "Transition before must match exact effective registry reference"
            )
        after = (
            None
            if update.after is None
            else Entry.model_validate_json(
                canonical(update.after.model_dump(mode="json", round_trip=True))
            )
        )
        if old and old.permanent_content is not None and after is not None:
            _stable(
                old.entry() or Entry.model_validate_json(old.permanent_content), after
            )
        candidate[update.target_key] = entity(
            after, update.target_key, record.record_key
        )
        if old is not None:
            candidate[update.target_key] = replace(
                candidate[update.target_key], permanent_content=old.permanent_content
            )
    return candidate


def _closure(records: dict[str, Entity]) -> None:
    entries = [
        entry for item in records.values() if (entry := item.entry()) is not None
    ]
    validate_structure(entries, historical_art=True)
    validate_collections(entries)
    printings = typed(records, PrintingData)
    jp = {p.card_no: p for p in printings.values() if p.region == "jp"}
    cards = typed(records, CardData)
    for printing in printings.values():
        if cards["card:" + printing.card_id].identity_state == "retired":
            raise ValueError("Retired card must not have effective printings")
        _printing_evidence(printing, jp)
    uses: set[tuple[str, str]] = set()
    for art in typed(records, ArtData).values():
        actual = {(u.printing_id, u.face_id) for u in art.uses}
        if len(actual) != len(art.uses) or uses & actual:
            raise ValueError("Art uses must be unique, not duplicated across art")
        uses.update(actual)
        if actual and art.observation not in {
            printings["printing:" + pid].observation for pid, _ in actual
        }:
            raise ValueError("Art observation must refer to one actual current use")


def _route_step(
    inputs: ReplayInputs,
    record: Transition,
    candidate: dict[str, Entity],
    routes: RouteProjection,
) -> RouteProjection:
    facts = inputs.routes(record)
    evidence_ids = {e.source_version_id for e in record.evidence}
    if any(f.source_version_id not in evidence_ids for f in facts):
        raise ValueError("Route sources must be pinned in transition evidence")
    result = project(derive(candidate, facts), routes)
    check_updates(routes, result, record.routes)
    return result


def _replay(root: Path, inputs: ReplayInputs) -> EffectiveRegistry:
    """Validate apply transactions against historical bases and return detached state.

    Supply a stable checkout or hold the global registry lock for this read. This
    is a structural/effective replay core, not an F1 evidence verifier, publisher
    or replacement for legacy build readers. Revert fails closed until stage C.
    """
    transitions = load_transitions(root)
    current = read_base_files(root)
    current_records = _base(current)
    reserved = set(current_records)
    records: dict[str, Entity] = {}
    previous: RegistryFiles | None = None
    repairs: tuple[Repair, ...] = ()
    routes: RouteProjection | None = None
    for loaded in transitions.shards:
        shard = loaded.envelope()
        record = shard.records[0]
        if record.action != "apply":
            raise ValueError("Named revert effective replay requires stage C")
        basis = inputs.registry(record.registry_basis)
        if (
            digest(canonical(basis.index().model_dump(mode="json")))
            != record.registry_basis.index_hash
        ):
            raise ValueError("Historical registry basis index hash mismatch")
        _append(previous, basis, records, reserved)
        previous = basis
        if routes is None:
            routes = project(derive(records, inputs.routes(None)), None)
        before = dict(records)
        candidate = _updates(records, record, reserved)
        _closure(candidate)
        validate_moves(before, candidate, record)
        repairs += record.repairs
        check_graph(repairs)
        routes = _route_step(inputs, record, candidate, routes)
        records = candidate
        reserved.update(records)
    if previous is None:
        records = current_records
    else:
        _append(previous, current, records, reserved)
    _closure(records)
    if routes is None:
        routes = project(derive(records, inputs.routes(None)), None)
    else:
        # A post-review append needs a new reviewed route/source plan, not guessed entries.
        printings = typed(records, PrintingData)
        if set(routes.states) != {p.id for p in printings.values()}:
            raise ValueError(
                "Unreviewed registry append requires complete new route evidence"
            )
    return EffectiveRegistry(frozen_records(records), repairs, routes)


def replay(root: Path, inputs: ReplayInputs) -> EffectiveRegistry:
    """Replay into a detached candidate, failing closed on incomplete dependencies."""
    try:
        return _replay(root, inputs)
    except KeyError, TypeError:
        raise ValueError("Incomplete effective identity dependency closure") from None

"""Effective confirmed-none review renewal, without enabling identity repairs."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.provenance import SourceUse, uses_sorted
from sve_carddb.domains.registry.records import ArtData, MappingReviewData
from sve_carddb.domains.registry.snapshot import RegistrySnapshot, from_files
from sve_carddb.domains.registry.storage import Entry, RegistryFiles, read_base_files
from sve_carddb.domains.registry.transitions.history import check_append
from sve_carddb.domains.registry.transitions.loader import load_transitions
from sve_carddb.domains.registry.transitions.models import (
    After,
    Before,
    Reference,
    Transition,
)

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.registry.transitions.files import LoadedShard
    from sve_carddb.domains.registry.transitions.review_sources import ReviewSources


def reference(entry: Entry, transition_key: str | None) -> Before:
    """Pin the entire semantic record and its latest effective producer."""
    return Before(
        record_key=entry.record_key,
        record_hash=digest(canonical(entry.model_dump(mode="json"))),
        transition_key=transition_key,
    )


def tail(record: Transition) -> Reference:
    """Pin an entire transition, including its context and complete after values."""
    return Reference(
        record_key=record.record_key,
        record_hash=digest(canonical(record.model_dump(mode="json"))),
    )


def _append(
    previous: RegistryFiles | None,
    current: RegistryFiles,
    state: dict[str, tuple[Entry, str, Before]],
) -> None:
    check_append(previous, current)
    old = {
        entry.record_key
        for shard in (() if previous is None else previous.shards)
        for entry in shard.envelope().records
    }
    for shard in current.shards:
        for entry in shard.envelope().records:
            if entry.record_key not in old:
                if entry.record_key in state:
                    raise ValueError("Appended registry record reuses an existing key")
                state[entry.record_key] = entry, shard.path, reference(entry, None)


def _review(old: Entry, after: After) -> MappingReviewData:
    previous = MappingReviewData.model_validate_json(canonical(old.data))
    data = MappingReviewData.model_validate_json(canonical(after.data))
    if (previous.card_id, previous.target_region) != (
        data.card_id,
        data.target_region,
    ) or data.target_region != "jp":
        raise ValueError("Renewal cannot change stable review identity or conclusion")
    return data


def _art(old: Entry, after: After) -> ArtData:
    previous = ArtData.model_validate_json(canonical(old.data))
    data = ArtData.model_validate_json(canonical(after.data))
    if (
        previous.model_dump(exclude={"uses"}) != data.model_dump(exclude={"uses"})
        or data.uses[: len(previous.uses)] != previous.uses
        or len(set(data.uses)) != len(data.uses)
        or len(data.uses) <= len(previous.uses)
    ):
        raise ValueError(
            "Art extension may only append unique uses to the same immutable art"
        )
    return data


def _apply(
    record: Transition,
    path: str,
    state: dict[str, tuple[Entry, str, Before]],
) -> tuple[MappingReviewData | ArtData, ...]:
    if record.action != "apply" or record.repairs or record.routes:
        raise ValueError(
            "Only review renewals and art extensions have effective projection support"
        )
    reviewed: list[MappingReviewData | ArtData] = []
    for update in record.updates:
        old, after = state.get(update.target_key), update.after
        if old is None or after is None or update.before != old[2]:
            raise ValueError(
                "Renewal requires an exact existing record before reference"
            )
        if (old[0].kind, old[0].owner) != (after.kind, after.owner):
            raise ValueError("Renewal must retain record kind and owner")
        if after.kind == "region_mapping_review":
            reviewed.append(_review(old[0], after))
        elif after.kind == "art":
            reviewed.append(_art(old[0], after))
        else:
            raise ValueError("Only review renewal and art use extension are supported")
        entry = Entry.model_validate_json(
            canonical(after.model_dump(mode="json", round_trip=True))
        )
        state[update.target_key] = entry, path, reference(entry, record.record_key)
    return tuple(reviewed)


def _unique_art_uses(state: dict[str, tuple[Entry, str, Before]]) -> None:
    assigned: set[tuple[str, str]] = set()
    for entry, _, _ in state.values():
        if entry.kind != "art":
            continue
        art = ArtData.model_validate_json(canonical(entry.data))
        uses = {(use.printing_id, use.face_id) for use in art.uses}
        if assigned & uses:
            raise ValueError("Multiple adopted art groups for one printing face")
        assigned.update(uses)


def load_renewed_registry(
    root: Path,
    sources: ReviewSources,
    *,
    candidate: LoadedShard | None = None,
) -> RegistrySnapshot:
    """Reconstruct historical bases, complete evidence and the current effective reviews.

    Callers hold the global registry lock or supply a stable authored checkout.
    Unsupported transitions still fail closed; this never changes permanent IDs.
    """
    shards = load_transitions(root).shards + (() if candidate is None else (candidate,))
    current = read_base_files(root)
    state: dict[str, tuple[Entry, str, Before]] = {}
    previous: RegistryFiles | None = None
    previous_ref: Reference | None = None
    uses: list[SourceUse] = []
    for sequence, shard in enumerate(shards, start=1):
        record = shard.envelope().records[0]
        if record.sequence != sequence or record.previous != previous_ref:
            raise ValueError(
                "Renewal must extend the exact contiguous transition chain"
            )
        basis = sources.history.registry(record.registry_basis)
        _append(previous, basis, state)
        reviews = _apply(record, shard.path, state)
        verified = sources.transition(record, basis, reviews)
        uses.extend(verified.uses)
        _unique_art_uses(state)
        from_files(
            basis,
            replacements=tuple(
                (entry, path)
                for entry, path, _ in state.values()
                if entry.kind in {"region_mapping_review", "art"}
            ),
        )
        previous, previous_ref = basis, tail(record)
    _append(previous, current, state)
    if shards:
        _unique_art_uses(state)
    return from_files(
        current,
        replacements=tuple(
            (entry, path)
            for entry, path, _ in state.values()
            if entry.kind in {"region_mapping_review", "art"}
        ),
        transitions=shards,
        transition_uses=uses_sorted(uses),
    )

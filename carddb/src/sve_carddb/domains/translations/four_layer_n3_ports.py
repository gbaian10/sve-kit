"""Projection interfaces for source-local pools, Stack holders and ability objects."""

import re
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    AbilitySetRef,
    CardNameReference,
    ObjectGeneration,
    ObjectSetRef,
    PlayerRef,
    PoolContext,
    ReceiptRef,
    Span,
    hash_payload,
)
from sve_carddb.domains.translations.four_layer_context import (
    ImportSignature,
    typed_import,
)
from sve_carddb.domains.translations.four_layer_event_preparation import (
    occurrence,
    origins,
    source_identity,
)
from sve_carddb.domains.translations.four_layer_n3_forms import POOL_FIRST, POOL_SECOND
from sve_carddb.domains.translations.four_layer_n3_shapes import ABILITY_REPLAY

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import Frame, Port
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_n3_sources import Builder


def holder(builder: Builder) -> ObjectGeneration:
    """A source symbol denotes the holder; it is never an evaluated game object."""
    return ObjectGeneration(
        object_id="source-holder:" + hash_payload(source_identity(builder.source)),
        generation=0,
    )


def _leaf_at(builder: Builder, span: Span, type_: str) -> str | None:
    for item in builder.additions:
        if item.slot.type == type_ and span in item.slot.occurrences:
            return item.slot.name
    positions = origins(builder.original, span)
    names = {
        o.slot
        for o in builder.found.occurrences
        if any(
            a.start < b.end and b.start < a.end for a in o.raw_spans for b in positions
        )
    }
    slots = [
        s.name
        for s in builder.found.schema.slots
        if s.type == type_ and s.name in names
    ]
    return slots[0] if len(slots) == 1 else None


def pool_interface(builder: Builder, family: str) -> bool:
    """Connect an actual source pool, first placement and remainder once."""
    first = POOL_FIRST.search(builder.text)
    second = POOL_SECOND.search(builder.text)
    if first is None or second is None or first.end() >= second.start():
        return False
    if builder.scope(first.start()) != builder.scope(second.start()):
        return False
    first_span = Span(start=first.start(), end=first.end())
    second_span = Span(start=second.start(), end=second.end())
    source = _pool_source(builder, family, first.start())
    if source is None:
        return False
    pool, provider = source
    first_role = (
        "pool_fill_battlefield"
        if family == "P03"
        else "pool_allocate_exact"
        if first[0].startswith("N")
        else "pool_allocate_any"
    )
    pool_name = builder.port(
        first_role,
        pool,
        "selection",
        provider,
        first.start(),
        tuple(n for pair in pool.members for n in pair),
    )
    builder.exports.append(
        builder.ports[-1].model_copy(update={"source_role": "pool_source"})
    )
    if family == "P03":
        builder.port(
            "source_actor",
            PlayerRef(kind="PlayerRef", player="self"),
            "live",
            first_span,
            first.start(),
        )
    placed = builder.port(
        first_role,
        ReceiptRef(
            kind="ReceiptId",
            receipt_id=builder.identity("placement", first_span),
            result="objects",
        ),
        "receipt",
        first_span,
        first.start(),
    )
    builder.exports.append(builder.ports.pop())
    second_role = "pool_move_remainder" if family == "P03" else first_role
    builder.port(
        second_role,
        ObjectSetRef(
            kind="ObjectSet",
            origin="pool_remainder",
            objects=(
                ObjectGeneration(
                    object_id=builder.identity("remaining", first_span), generation=0
                ),
            ),
            pool=pool_name,
            placement=placed,
        ),
        "receipt",
        Span(start=first_span.start, end=second_span.end),
        second.start(),
    )
    builder.exports.append(
        builder.ports[-1].model_copy(
            update={"source_role": "pool_remaining_after_first"}
        )
    )
    return True


def ability_interface(builder: Builder) -> bool:
    """Preserve the current holder and evolve ability source selection."""
    match = ABILITY_REPLAY.search(builder.text)
    if match is None:
        return False
    span = Span(start=match.start(), end=match.end())
    builder.port(
        "ability_source",
        AbilitySetRef(
            kind="AbilitySetRef",
            holder_generation=holder(builder),
            ability_kind="term:event.evolve",
            source_selector=occurrence(builder.source, builder.original).model_copy(
                update={"segments": origins(builder.original, span)}
            ),
        ),
        "selection",
        span,
        match.start(),
    )
    return True


def stack_interface(builder: Builder) -> None:
    """Bind counter additions and reads to their source holder."""
    for item in builder.additions:
        if item.slot.role not in {"counter_amount", "threshold"}:
            continue
        span = item.slot.occurrences[0]
        builder.port(
            "read_subject",
            ObjectSetRef(kind="ObjectSet", objects=(holder(builder),), origin="holder"),
            "live",
            span,
            span.start,
        )
    for match in re.finditer(r"スタックカウンターすべて", builder.text):
        span = Span(start=match.start(), end=match.end())
        builder.port(
            "read_subject",
            ObjectSetRef(kind="ObjectSet", objects=(holder(builder),), origin="holder"),
            "live",
            span,
            span.start,
        )


def verify_counter_witness(
    candidates: Mapping[ObjectGeneration, int],
    first: ObjectGeneration | None,
    second: ObjectGeneration | None,
    thresholds: tuple[int, int] = (4, 10),
) -> tuple[bool, bool]:
    """K5 records a common witness for the highest satisfied sealed threshold."""
    if thresholds != (4, 10) or any(
        type(n) is not int or n < 0 for n in candidates.values()
    ):
        raise ValueError("Invalid sealed counter snapshot")
    outcomes = tuple(any(n >= t for n in candidates.values()) for t in thresholds)
    if first != second:
        raise ValueError("Counter reads require the same holder generation")
    if not outcomes[0]:
        if first is not None:
            raise ValueError("No holder witnesses an unmet threshold")
        return False, False
    if (
        first is None
        or first not in candidates
        or any(
            (candidates[first] >= t) != result
            for t, result in zip(thresholds, outcomes, strict=True)
        )
    ):
        raise ValueError("Holder does not witness the highest satisfied threshold")
    return outcomes[0], outcomes[1]


def _pool_source(
    builder: Builder, family: str, start: int
) -> tuple[PoolContext, Span] | None:
    scope = builder.scope(start)
    if family == "P02":
        search = re.search(r"N枚まで探す。", builder.text[:start])
        if search is None or builder.scope(search.start()) != scope:
            return None
        provider = Span(start=search.start(), end=search.end())
        receipt = builder.port(
            "pool_search_result",
            ReceiptRef(
                kind="ReceiptId",
                receipt_id=builder.identity("search", provider),
                result="objects",
            ),
            "receipt",
            provider,
            start,
        )
        builder.exports.append(builder.ports.pop())
        return PoolContext(
            kind="PoolContext",
            pool_id=builder.identity("pool", provider),
            source_kind="search_result",
            receipt=receipt,
            scope=scope,
        ), provider
    members = []
    matches = tuple(re.finditer(r"(?P<name>『X』)(?P<count>N)枚", builder.text[:start]))
    for match in matches:
        name = _leaf_at(
            builder, Span(start=match.start("name"), end=match.end("name")), "CardName"
        )
        count = _leaf_at(
            builder, Span(start=match.start("count"), end=match.end("count")), "Nat"
        )
        if name is None or count is None:
            return None
        members.append((name, count))
    if len(members) not in {1, 2} or builder.scope(matches[0].start()) != scope:
        return None
    provider = Span(start=matches[0].start(), end=start)
    return PoolContext(
        kind="PoolContext",
        pool_id=builder.identity("pool", provider),
        source_kind="generated",
        members=tuple(members),
        scope=scope,
    ), provider


def verify_pool_allocation(frame: Frame, binding: SourceBinding) -> None:
    """Check finite source connections; actual allocation remains an engine concern."""
    pools = [p for p in frame.projection.imports if p.type == "PoolContext"]
    if not pools:
        return
    if len(pools) != 1:
        raise ValueError("One finite sequence requires one pool")
    port = pools[0]
    typed_import(
        frame,
        binding,
        port.name,
        port.scope,
        ImportSignature("pool", "PoolContext", "selection"),
    )
    assert port.evidence is not None
    pool = port.evidence.value
    if not isinstance(pool, PoolContext) or pool.scope != port.scope:
        raise ValueError("Pool scope differs from its source provider")
    _paired_export(frame, port, "pool_source")
    if pool.source_kind == "generated":
        for name, count in pool.members:
            if (
                not isinstance(binding.values.get(name), CardNameReference)
                or type(binding.values.get(count)) is not int
            ):
                raise ValueError("Generated pool requires source name/count pairs")
    else:
        _objects_receipt(frame, pool.receipt, port.scope, "pool_search_result")
    _verify_remainder(frame, binding, port)
    if port.source_role == "pool_fill_battlefield":
        actors = [
            p for p in frame.projection.imports if p.source_role == "source_actor"
        ]
        if len(actors) != 1:
            raise ValueError("Capacity placement requires one live source actor")
        typed_import(
            frame,
            binding,
            actors[0].name,
            port.scope,
            ImportSignature("actor", "PlayerRef", "live"),
        )


def _paired_export(frame: Frame, port: Port, role: str) -> None:
    expected = port.model_copy(update={"source_role": role})
    if tuple(p for p in frame.projection.exports if p.name == port.name) != (expected,):
        raise ValueError("Pool export/import pair differs in identity or evidence")


def _objects_receipt(frame: Frame, name: str | None, scope: str, role: str) -> Port:
    ports = tuple(
        p
        for p in frame.projection.exports
        if p.name == name and p.source_role == role and p.scope == scope
    )
    if len(ports) != 1 or ports[0].evidence is None:
        raise ValueError("Pool requires its same-operation receipt export")
    evidence = ports[0].evidence
    assert evidence is not None
    if (
        evidence.capture != "receipt"
        or not isinstance(evidence.value, ReceiptRef)
        or evidence.value.result != "objects"
    ):
        raise ValueError("Pool requires an actual objects receipt")
    return ports[0]


def _verify_remainder(frame: Frame, binding: SourceBinding, pool: Port) -> None:
    candidates = [
        p
        for p in frame.projection.imports
        if p.evidence
        and isinstance(p.evidence.value, ObjectSetRef)
        and p.evidence.value.origin == "pool_remainder"
    ]
    if len(candidates) != 1:
        raise ValueError("Second placement consumes exactly one remainder set")
    port = candidates[0]
    typed_import(
        frame,
        binding,
        port.name,
        pool.scope,
        ImportSignature("remainder", "ObjectSet", "receipt", origin="pool_remainder"),
    )
    assert port.evidence is not None
    value = port.evidence.value
    assert isinstance(value, ObjectSetRef)
    if value.pool != pool.name:
        raise ValueError("Remainder refers to another pool")
    receipt = _objects_receipt(frame, value.placement, pool.scope, pool.source_role)
    assert receipt.evidence is not None
    if not all(
        any(
            a.start <= b.start and b.end <= a.end for a in port.evidence.source.segments
        )
        for b in receipt.evidence.source.segments
    ):
        raise ValueError("Remainder evidence must contain the actual first placement")
    _paired_export(frame, port, "pool_remaining_after_first")

"""Compose the sealed N2 providers without enabling them in production."""

from collections import Counter
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    EventRef,
    LinkedCondition,
    ObjectGeneration,
    ObjectSetRef,
    Port,
    PredicateExpr,
    Scope,
    hash_payload,
)
from sve_carddb.domains.translations.four_layer_event_preparation import occurrence
from sve_carddb.domains.translations.four_layer_history import HISTORY_FAMILY
from sve_carddb.domains.translations.four_layer_mode_context import FAMILY as MODES
from sve_carddb.domains.translations.four_layer_mode_context import mode_sources
from sve_carddb.domains.translations.four_layer_n2_read_sources import regions
from sve_carddb.domains.translations.four_layer_n2_reads import FAMILY as READS
from sve_carddb.domains.translations.four_layer_names import FAMILY as NAMES
from sve_carddb.domains.translations.four_layer_preparation import PreparationFamily
from sve_carddb.domains.translations.four_layer_provenance import PROVENANCE_FAMILY
from sve_carddb.domains.translations.four_layer_total_sources import FAMILY as TOTALS

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import ModeCountRule, Projection
    from sve_carddb.contracts.source_binding import TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_preparation import PreparedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

PROVIDERS = (HISTORY_FAMILY, PROVENANCE_FAMILY, NAMES, TOTALS, READS, MODES)


def _claims(
    prepared: PreparedField,
) -> Counter[tuple[int, str, tuple[tuple[int, int], ...]]]:
    return Counter(
        (
            i,
            value.expression_id,
            tuple(
                (s.start, s.end)
                for o in found.occurrences
                if o.slot == name
                for s in o.raw_spans
            ),
        )
        for i, found in enumerate(prepared.derived.recognized)
        for name, value in found.values.items()
        if isinstance(value, (ContextExpression, PredicateExpr))
    )


def verify_claims(before: PreparedField, after: PreparedField) -> None:
    """A consumer may add uses, but cannot replace or duplicate another provider's node."""
    old, new = _claims(before), _claims(after)
    for (index, identifier, positions), count in old.items():
        matching = sum(
            n
            for (i, d, spans), n in new.items()
            if i == index and d == identifier and set(positions) <= set(spans)
        )
        if matching != count:
            raise ValueError(
                "Preparation families overlap an owned source construction"
            )
    anchors: set[tuple[int, tuple[tuple[int, int], ...]]] = set()
    for (index, _, positions), count in new.items():
        anchor = (index, positions)
        if positions and anchor in anchors:
            raise ValueError("Preparation families duplicate a source construction")
        anchors.add(anchor)
        if positions and count != 1:
            raise ValueError("Preparation families duplicate a source construction")


def _remap_ports(before: PreparedField, after: PreparedField) -> PreparedField:
    interfaces = list(after.interfaces)
    for i, (prior, found) in enumerate(
        zip(before.derived.recognized, after.derived.recognized, strict=True)
    ):
        previous, current = before.interfaces[i], interfaces[i]
        if previous is None or current is None:
            continue
        ports = {p.name: p for p in previous.imports}
        updated = []
        for port in current.imports:
            old = ports.get(port.name)
            if (
                old is None
                or old.evidence is None
                or not old.evidence.source_leaves
                or port.evidence is None
            ):
                updated.append(port)
                continue
            names = []
            for name in old.evidence.source_leaves:
                positions = tuple(
                    o.raw_spans for o in prior.occurrences if o.slot == name
                )
                candidates = [
                    s.name
                    for s in found.schema.slots
                    if tuple(o.raw_spans for o in found.occurrences if o.slot == s.name)
                    == positions
                    and found.values[s.name] == prior.values[name]
                ]
                if len(candidates) != 1:
                    raise ValueError("Preparation lost a provider's source leaf")
                names.append(candidates[0])
            updated.append(
                port.model_copy(
                    update={
                        "evidence": port.evidence.model_copy(
                            update={"source_leaves": tuple(names)}
                        )
                    }
                )
            )
        interfaces[i] = current.model_copy(update={"imports": tuple(updated)})
    return replace(after, interfaces=tuple(interfaces))


def _scope_at(raw: str, position: int) -> str:
    return min(
        (r for r in regions(raw) if r.start <= position < r.end),
        key=lambda r: r.end - r.start,
    ).id


def _unify_holder(port: Port, scopes: dict[str, Scope]) -> Port:
    evidence = port.evidence
    if (
        evidence is None
        or not isinstance(evidence.value, ObjectSetRef)
        or evidence.value.origin != "holder"
    ):
        return port
    scope = scopes[port.scope]
    while scope.kind != "ability" and scope.parent is not None:
        scope = scopes[scope.parent]
    ability = scope.id
    identity = hash_payload(
        {
            "owner": evidence.source.owner.model_dump(mode="json"),
            "field": evidence.source.field,
            "ordinal": evidence.source.ordinal,
            "source_hash": evidence.source.source_hash,
            "ability": ability,
        }
    )
    value = evidence.value.model_copy(
        update={
            "objects": (ObjectGeneration(object_id="source:" + identity, generation=0),)
        }
    )
    return port.model_copy(
        update={"evidence": evidence.model_copy(update={"value": value})}
    )


def _ports(
    raw: str, interface: Projection, names: dict[str, str], scopes: dict[str, Scope]
) -> tuple[Port, ...]:
    ports = tuple(
        _unify_holder(
            p.model_copy(
                update={
                    "scope": names.get(
                        p.scope,
                        _scope_at(
                            raw,
                            p.evidence.source.segments[0].start if p.evidence else 0,
                        ),
                    )
                }
            ),
            scopes,
        )
        for p in interface.imports
    )
    subjects = {
        old.evidence.value.objects[0]: new.evidence.value.objects[0]
        for old, new in zip(interface.imports, ports, strict=True)
        if old.evidence
        and new.evidence
        and isinstance(old.evidence.value, ObjectSetRef)
        and isinstance(new.evidence.value, ObjectSetRef)
        and len(old.evidence.value.objects) == 1
    }
    return tuple(
        p.model_copy(
            update={
                "evidence": p.evidence.model_copy(
                    update={
                        "value": p.evidence.value.model_copy(
                            update={"subject": subjects[p.evidence.value.subject]}
                        )
                    }
                )
            }
        )
        if p.evidence
        and isinstance(p.evidence.value, EventRef)
        and p.evidence.value.subject in subjects
        else p
        for p in ports
    )


def _scoped_values(
    raw: str, found: Recognized, interface: Projection, position: int
) -> tuple[dict[str, TypedValue], dict[str, str]]:
    values = dict(found.values)
    names: dict[str, str] = {}
    for name, value in values.items():
        if not isinstance(value, (ContextExpression, PredicateExpr)):
            continue
        positions = [
            s.start for o in found.occurrences if o.slot == name for s in o.raw_spans
        ]
        at = min(positions, default=position)
        if value.expression_id == "mode.unused_this_turn.v1":
            port = next(
                p for p in interface.imports if p.name == value.imports["choice"]
            )
            if port.evidence:
                at = port.evidence.source.segments[0].start
        scope = _scope_at(raw, at)
        consumer = value.scope.rsplit(".", maxsplit=1)[-1]
        if consumer in {
            "body_condition",
            "ability_availability",
            "play_availability",
            "mode_count",
        }:
            scope += "." + consumer
        if value.scope in names and names[value.scope] != scope:
            raise ValueError("One provider scope crosses lexical abilities or branches")
        names[value.scope] = scope
        values[name] = value.model_copy(update={"scope": scope})
    return values, names


def _scopes(raw: str, prepared: PreparedField) -> PreparedField:
    lexical = regions(raw)
    scopes = {
        r.id: Scope(
            id=r.id,
            parent=r.parent,
            kind="ability" if r.kind == "ability" else "branch",
        )
        for r in lexical
    }
    founds: list[Recognized] = []
    interfaces: list[Projection | None] = []
    rules: list[tuple[ModeCountRule, ...]] = []
    for i, found in enumerate(prepared.derived.recognized):
        interface = prepared.interfaces[i]
        current_rules = prepared.mode_rules[i] if prepared.mode_rules else ()
        if interface is None or (
            not current_rules
            and not any(
                isinstance(v, (ContextExpression, PredicateExpr))
                for v in found.values.values()
            )
        ):
            founds.append(found)
            interfaces.append(interface)
            rules.append(current_rules)
            continue
        values, names = _scoped_values(
            raw,
            found,
            interface,
            prepared.derived.field.parts[i].source_span.segments[0].start,
        )
        for port in interface.imports:
            if port.scope not in names and port.evidence:
                scope = _scope_at(raw, port.evidence.source.segments[0].start)
                consumer = port.scope.rsplit(".", maxsplit=1)[-1]
                if consumer in {
                    "body_condition",
                    "ability_availability",
                    "play_availability",
                    "mode_count",
                }:
                    scope += "." + consumer
                names[port.scope] = scope
        for scope in names.values():
            if scope not in scopes:
                scopes[scope] = Scope(
                    id=scope, parent=scope.rsplit(".", maxsplit=1)[0], kind="sequence"
                )
        imports = _ports(raw, interface, names, scopes)
        exports = tuple(
            next(
                (p for p in imports if p.name == old.name),
                old.model_copy(
                    update={"scope": names.get(old.scope, _scope_at(raw, 0))}
                ),
            )
            for old in interface.exports
        )
        founds.append(
            replace(
                found,
                values=values,
                schema=found.schema.model_copy(
                    update={
                        "slots": tuple(
                            s.model_copy(update={"expression": values[s.name]})
                            if s.expression is not None
                            else s
                            for s in found.schema.slots
                        )
                    }
                ),
            )
        )
        interfaces.append(
            interface.model_copy(
                update={
                    "scopes": tuple(sorted(scopes.values(), key=lambda s: s.id)),
                    "imports": imports,
                    "exports": exports,
                }
            )
        )
        rules.append(
            tuple(
                rule.model_copy(
                    update={
                        "scope": names.get(
                            rule.scope,
                            next(
                                p.scope
                                for p in imports
                                if p.name == rule.choice_context
                            ),
                        )
                    }
                )
                for rule in current_rules
            )
        )
    return replace(
        prepared,
        derived=replace(prepared.derived, recognized=tuple(founds)),
        interfaces=tuple(interfaces),
        mode_rules=tuple(rules),
    )


def prepare_integrated(
    raw: str,
    prepared: PreparedField,
    classifier: Classifier,
    context: CardContext | None,
) -> PreparedField:
    """Run real providers in dependency order and reject competing source ownership."""
    for family in PROVIDERS:
        updated = family.prepare(raw, prepared, classifier, context)
        verify_claims(prepared, updated)
        prepared = _remap_ports(prepared, updated)
    return _link_conditions(raw, _scopes(raw, prepared))


def _link_conditions(raw: str, prepared: PreparedField) -> PreparedField:
    rules = list(prepared.mode_rules)
    for group in mode_sources(raw, prepared):
        if group.condition is None:
            continue
        providers = [
            i
            for i in group.members
            if any(
                s.start <= group.count.start < group.count.end <= s.end
                for s in prepared.derived.field.parts[i].source_span.segments
            )
        ]
        if len(providers) != 1:
            raise ValueError("Mode condition requires a unique source intro")
        index = providers[0]
        if len(rules[index]) != 1:
            raise ValueError("Mode condition requires a unique source rule")
        condition = rules[index][0].condition
        if isinstance(condition, LinkedCondition):
            raise TypeError("Mode condition source cannot link another source")
        source = occurrence(
            prepared.derived.field.source, prepared.derived.field.parts[index]
        )
        for member in group.members:
            if member != index:
                rules[member] = tuple(
                    rule.model_copy(
                        update={
                            "condition": LinkedCondition(
                                state="linked", source=source, condition=condition
                            )
                        }
                    )
                    for rule in rules[member]
                )
    return replace(prepared, mode_rules=tuple(rules))


FAMILY = PreparationFamily(
    "N2-integrated",
    tuple(d for family in PROVIDERS for d in family.descriptors),
    prepare_integrated,
)

"""Reconstructed source candidates and conservative author operand compatibility."""

from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl
from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Constant,
    Expression,
    Frame,
    QuantitySpec,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.contracts.source_binding import SourceBinding
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_expressions import definition

if TYPE_CHECKING:
    from collections.abc import Mapping

    from pydantic import JsonValue

    from sve_carddb.contracts.four_layer import LeafSlot
    from sve_carddb.contracts.source_binding import ClosedDomain, SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


PLAYER_VALUES = {"self": "self", "opponent": "opp"}
ZONE_VALUES = {
    "battlefield": "field",
    "hand": "hand",
    "deck": "deck",
    "evolve_deck": "evolve_deck",
    "graveyard": "cemetery",
    "ex": "ex",
}
# These descriptors retain every supported filter and operand in the sibling count.
OFFSET_DESCRIPTORS = {
    "count.current_zone.owner.zone.subtract_offset.v1": "minus",
    "count.current_zone.owner.zone.name.add_offset.v1": "plus",
}
SOURCE_SITES: dict[tuple[str, tuple[str | int, ...]], tuple[str, str, str, str]] = {
    ("A.act.draw", ("n",)): ("Nat", "Nat", "count", "value"),
    ("A.act.discard.exact", ("n",)): ("Nat", "Nat", "count", "value"),
    ("A.act.create", ("n",)): ("Nat", "Nat", "count", "value"),
    ("A.act.damage", ("n",)): ("Nat", "Nat", "damage_amount", "value"),
    ("A.act.life", ("delta",)): ("Nat", "Nat", "recovery_amount", "value"),
    ("A.effect.cost-mod", ("set",)): ("Nat", "Nat", "cost_value", "value"),
    ("A.act.draw", ("who",)): ("Player", "Player", "destination_owner", "value"),
    ("A.expr.count-objects", ("who",)): (
        "Player",
        "Player",
        "counted_owner",
        "value",
    ),
    ("A.expr.count-objects", ("zone",)): (
        "Zone",
        "ZoneSet",
        "counted_zone",
        "value",
    ),
    ("A.sel.zone-objects", ("who",)): ("Player", "Player", "source_owner", "value"),
    ("A.sel.zone-objects", ("zone",)): ("Zone", "ZoneSet", "source_zone", "value"),
    **dict.fromkeys(
        (
            ("A.expr.minus", ("minus", 1)),
            ("A.expr.plus", ("plus", 0)),
            ("A.expr.plus", ("plus", 1)),
        ),
        ("Int", "Nat", "arithmetic_offset", "value"),
    ),
    **{
        (owner, (path,)): ("Nat", "QuantitySpec", "selection_count", "constant")
        for owner, path in (
            ("A.act.choose.exact", "n"),
            ("A.act.search.exact", "n"),
            ("A.target.exact", "n"),
            ("A.act.choose.up-to", "upto"),
            ("A.act.search.up-to", "upto"),
            ("A.target.up-to", "upto"),
        )
    },
    **{
        (owner, ("to",)): ("Zone", "ZoneSet", "destination_zone", "value")
        for owner in (
            "A.act.create",
            "A.act.move.ref",
            "A.act.move.top",
            "A.act.search.exact",
            "A.act.search.up-to",
        )
    },
    **{
        (owner, ("name",)): ("CardName", "CardName", "name_filter", "value")
        for owner in ("A.sel.in-objects", "A.sel.zone-objects")
    },
    ("A.act.create", ("token",)): ("CardName", "CardName", "created_name", "value"),
    ("A.act.create", ("token", "*")): ("CardName", "CardName", "created_name", "value"),
}
BINARY_ARITY = 2

ADAPTER_RECIPE = digest(
    canonical(
        {
            "recipe": "author-source-adapter-v1",
            "players": dict(PLAYER_VALUES),
            "zones": dict(ZONE_VALUES),
            "offset_descriptors": dict(OFFSET_DESCRIPTORS),
            "names": "classifier-card-name-source-ja-v1",
            "other_operand_mappings": "unsupported",
            "scalar_sites": [
                {"owner": owner, "path": list(path), "signature": list(signature)}
                for (owner, path), signature in sorted(SOURCE_SITES.items())
            ],
            "ports": "unsupported",
        }
    )
)


class SourceError(ValueError):
    """Diagnostics expose reason codes without copying candidate source wording."""

    def __init__(self, reason: str) -> None:
        self.reason = reason
        super().__init__(reason)


@dataclass(frozen=True)
class SourceCandidate:
    raw: str = field(repr=False)
    descriptor: SourceDescriptor
    classifier: Classifier = field(repr=False)
    context: CardContext | None = None


@dataclass(frozen=True)
class SourceContext:
    frame: Frame
    binding: SourceBinding
    canonical_source: str = field(repr=False)
    classifier: Classifier | None = field(default=None, repr=False)

    source_proof: Literal["none", "derived_self_consistent"] = field(
        default="none", init=False
    )


def _structure(frame: Frame, binding: SourceBinding) -> None:
    if frame.semantic_variant.state != "resolved":
        raise SourceError("source_pending")
    if frame.role != "body" or frame.projection.projection_kind != "ability_body":
        raise SourceError("source_not_ability_body")
    if not any(
        scope.kind == "ability" and scope.parent is None
        for scope in frame.projection.scopes
    ):
        raise SourceError("source_ability_scope_missing")
    if binding.frame_id != frame.id:
        raise SourceError("source_frame_mismatch")
    if frame.projection.imports or frame.projection.exports:
        raise SourceError("unsupported_port_source_mapping")


def check_source_compatibility(
    frame: Frame,
    binding: SourceBinding,
    canonical_source: str,
    domains: Mapping[str, ClosedDomain],
    *,
    classifier: Classifier | None = None,
) -> SourceContext:
    """Self-consistent synthetic contracts never acquire reconstructed-source proof."""
    try:
        frame = Frame.model_validate(frame.model_dump(mode="python"))
        binding = SourceBinding.model_validate(binding.model_dump(mode="python"))
        frame.verify(canonical_source)
        binding.verify(frame, domains)
    except ValueError:
        raise SourceError("source_compatibility_mismatch") from None
    _structure(frame, binding)
    return SourceContext(frame, binding, canonical_source, classifier)


def resolve_source(candidate: SourceCandidate, use: SourceUse) -> SourceContext:
    """Rebuild candidate evidence; registry currentness remains a separate boundary."""
    try:
        use = SourceUse.model_validate(use.model_dump(mode="python"))
        derived = derive_source(
            candidate.raw,
            candidate.descriptor,
            candidate.classifier,
            context=candidate.context,
        )
    except ValueError:
        raise SourceError("source_reconstruction_failed") from None
    indices = [
        index
        for index, part in enumerate(derived.field.parts)
        if part.line_ordinal == use.occurrence.line_ordinal
        and part.source_span.role == use.occurrence.role
        and part.source_span.segments == use.occurrence.segments
    ]
    if len(indices) != 1:
        raise SourceError("source_occurrence_mismatch")
    index = indices[0]
    if derived.recognized[index].issues:
        raise SourceError("source_unresolved_issue")
    if derived.pending_causes[index]:
        raise SourceError("source_pending")
    try:
        frame, binding = derived.bind(index)
        derived.verify_binding(candidate.raw, candidate.classifier, frame, binding)
    except ValueError:
        raise SourceError("source_reconstruction_failed") from None
    _structure(frame, binding)
    if binding.id != use.binding_id or binding.occurrence_key() != use.occurrence:
        raise SourceError("source_occurrence_mismatch")
    if not any(
        scope.id == use.ability_scope
        and scope.kind == "ability"
        and scope.parent is None
        for scope in frame.projection.scopes
    ):
        raise SourceError("source_ability_scope_mismatch")
    result = SourceContext(
        frame,
        binding,
        derived.field.parts[index].canonical_source,
        candidate.classifier,
    )
    object.__setattr__(result, "source_proof", "derived_self_consistent")  # ruff: ignore[unnecessary-dunder-call] -- only reconstruction sets frozen proof; dataclass replacement must reset it
    return result


def _slot(source: SourceContext, declaration: ParamDecl) -> LeafSlot:
    slots = {slot.name: slot for slot in source.frame.leaf_schema.slots}
    name = declaration.source.slot
    if name not in slots or name not in source.binding.values:
        raise SourceError("parameter_source_slot_missing")
    return slots[name]


def _name(source: SourceContext, value: object) -> str:
    if not isinstance(value, CardNameReference) or source.classifier is None:
        raise SourceError("unsupported_source_construct")
    term = source.classifier.terms.get(value.term_id)
    if term is None or term.category != "card_name":
        raise SourceError("parameter_source_name_mismatch")
    return term.source_ja


def _zone(value: object) -> str:
    if (
        not isinstance(value, tuple)
        or len(value) != 1
        or not isinstance(value[0], str)
        or value[0] not in ZONE_VALUES
    ):
        raise SourceError("unsupported_source_construct")
    return ZONE_VALUES[value[0]]


def _value(source: SourceContext, declaration: ParamDecl, slot: LeafSlot) -> int | str:
    value: object = source.binding.values[slot.name]
    component = declaration.source.component
    if component == "negated_magnitude" or declaration.type in {
        "Concept",
        "CardKind",
        "Phase",
    }:
        raise SourceError("unsupported_source_construct")
    if declaration.type in {"Int", "Nat"}:
        if component == "constant" and isinstance(value, QuantitySpec):
            value = value.expr
        if component == "constant" and isinstance(value, Constant):
            value = value.value
        if type(value) is not int or not 0 <= value <= SAFE_INTEGER:
            raise SourceError("parameter_source_value_mismatch")
        return value
    if component != "value":
        raise SourceError("parameter_source_component_mismatch")
    if declaration.type == "CardName":
        return _name(source, value)
    if declaration.type == "Zone":
        return _zone(value)
    if (
        declaration.type == "Player"
        and isinstance(value, str)
        and value in PLAYER_VALUES
    ):
        return PLAYER_VALUES[value]
    raise SourceError("unsupported_source_construct")


def _site(owner: str, path: tuple[str | int, ...]) -> tuple[str, str, str, str] | None:
    if owner == "A.act.create" and _token_path(path) and len(path) == BINARY_ARITY:
        path = ("token", "*")
    return SOURCE_SITES.get((owner, path))


def _token_path(path: tuple[str | int, ...]) -> bool:
    match path:
        case ("token",):
            return True
        case ("token", int(index)) if type(index) is int and index >= 0:
            return True
        case _:
            return False


def _offset_arguments(
    source: SourceContext, slot: LeafSlot
) -> tuple[str, dict[str, LeafSlot]]:
    matches = []
    for value in source.binding.values.values():
        expression = value.expr if isinstance(value, QuantitySpec) else value
        if not isinstance(expression, Expression) or slot.name not in expression.leaves:
            continue
        try:
            descriptor = definition(expression)
            arguments = descriptor.arguments(expression, source.frame.leaf_schema)
        except ValueError:
            raise SourceError("parameter_source_operand_mismatch") from None
        if arguments.get("offset") == slot:
            matches.append((descriptor, arguments))
    if len(matches) != 1:
        raise SourceError("parameter_source_operand_mismatch")
    descriptor, arguments = matches[0]
    operator = OFFSET_DESCRIPTORS.get(descriptor.id)
    if operator is None:
        raise SourceError("unsupported_source_construct")
    return operator, arguments


def _offset(
    source: SourceContext,
    slot: LeafSlot,
    owner: str,
    path: tuple[str | int, ...],
    fragment: JsonValue,
) -> None:
    operator, arguments = _offset_arguments(source, slot)
    if owner != "A.expr." + operator or path[0] != operator:
        raise SourceError("parameter_source_operand_mismatch")
    if not isinstance(fragment, dict) or set(fragment) != {operator}:
        raise SourceError("parameter_source_operand_mismatch")
    operands = fragment[operator]
    if (
        not isinstance(operands, list)
        or len(operands) != BINARY_ARITY
        or type(path[1]) is not int
    ):
        raise SourceError("parameter_source_operand_mismatch")
    target = operands[path[1]]
    if (
        not isinstance(target, dict)
        or set(target) != {"param"}
        or not isinstance(target["param"], str)
    ):
        raise SourceError("parameter_source_operand_mismatch")
    values = source.binding.values
    who = values[arguments["owner"].name]
    if not isinstance(who, str) or who not in PLAYER_VALUES:
        raise SourceError("unsupported_source_construct")
    count: dict[str, JsonValue] = {
        "who": PLAYER_VALUES[who],
        "zone": _zone(values[arguments["zone"].name]),
    }
    if "name" in arguments:
        count["name"] = _name(source, values[arguments["name"].name])
    if operands[1 - path[1]] != {"count": count}:
        raise SourceError("parameter_source_operand_mismatch")


def parameter_expected(
    source: SourceContext,
    declaration: ParamDecl,
    owner: str,
    relative_path: tuple[str | int, ...],
    fragment: JsonValue,
) -> int | str:
    """A matching scalar is insufficient without a registered operand construction."""
    try:
        declaration = ParamDecl.model_validate(declaration.model_dump(mode="python"))
    except ValueError:
        raise SourceError("parameter_source_declaration_invalid") from None
    slot = _slot(source, declaration)
    if declaration.source.component == "negated_magnitude" or declaration.type in {
        "Concept",
        "CardKind",
        "Phase",
    }:
        raise SourceError("unsupported_source_construct")
    site = _site(owner, relative_path)
    if site is None or site != (
        declaration.type,
        slot.type,
        slot.role,
        declaration.source.component,
    ):
        raise SourceError("parameter_source_role_mismatch")
    value = source.binding.values[slot.name]
    if isinstance(value, QuantitySpec) and value.mode != (
        "exact" if relative_path == ("n",) else "up_to"
    ):
        raise SourceError("parameter_source_mode_mismatch")
    expected = _value(source, declaration, slot)
    if slot.role != "arithmetic_offset":
        raise SourceError("unsupported_source_construct")
    _offset(source, slot, owner, relative_path, fragment)
    return expected

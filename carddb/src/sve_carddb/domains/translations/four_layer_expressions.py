"""Closed source expressions and positional, typed presentation arguments."""

from dataclasses import dataclass, replace
from functools import cache
from types import MappingProxyType
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Bound,
    ContextExpression,
    Domain,
    Expression,
    FormNode,
    LeafRef,
    LeafSchema,
    LeafSlot,
    LiteralNode,
    PredicateExpr,
    QuantitySpec,
    Signature,
    SlotRef,
    Target,
)
from sve_carddb.contracts.source_binding import QuantityDomain
from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Registry,
    verify_mode_count,
    visible,
)
from sve_carddb.domains.translations.four_layer_errors import MissingValueError
from sve_carddb.domains.translations.four_layer_n3_shapes import CHOICE_DOMAIN
from sve_carddb.domains.translations.four_layer_normalizer import VERSION

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from sve_carddb.contracts.four_layer import (
        FormDefinition,
        Frame,
        LeafType,
        Node,
        Projection,
    )
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_render import SelectedTarget

_ARGUMENTS: Mapping[str, tuple[LeafType, str]] = {
    "owner": ("Player", "player.relative.v1"),
    "zone": ("ZoneSet", "zone.single.v1"),
    "kind": ("CardKind", "card_kind.selection.v1"),
    "trait": ("Concept", "concept.trait.v1"),
    "class": ("Concept", "vocabulary.class.v1"),
    "name": ("CardName", "card_name.any.v1"),
    "keyword": ("Concept", "concept.keyword.v1"),
    "card": ("Concept", "concept.rule_term.v1"),
    "token_status": ("TokenStatus", "token_status.token.v1"),
    "offset": ("Nat", ""),
}


@dataclass(frozen=True)
class Definition:
    id: str
    parameters: tuple[str, ...]
    qualifiers: tuple[str, ...] = ()
    operator: str | None = None
    result_measure: str = "card_count"
    read_structure: str = "current_zone_count"

    @property
    def signature(self) -> tuple[Signature, ...]:
        """Expose the ordered type and role contract used by source family producers."""
        return tuple(
            Signature(
                name=name,
                type=_ARGUMENTS[name][0],
                role="arithmetic_offset" if name == "offset" else "read_" + name,
            )
            for name in self.parameters
        )

    def arguments(self, expr: Expression, schema: LeafSchema) -> dict[str, LeafSlot]:
        """Bind ordered actuals to rule formals; equal values never identify operands."""
        if len(expr.leaves) != len(self.parameters):
            raise ValueError("Expression arguments differ from its finite signature")
        slots = {slot.name: slot for slot in schema.slots}
        result = {}
        for signature, name in zip(self.signature, expr.leaves, strict=True):
            parameter = signature.name
            slot = slots.get(name)
            _, domain = _ARGUMENTS[parameter]
            if (
                slot is None
                or slot.type != signature.type
                or slot.role != signature.role
                or (domain and slot.domain.values != (domain,))
            ):
                raise ValueError(
                    "Expression argument has the wrong type role or domain"
                )
            result[parameter] = slot
        return result


def _definition(
    parameters: tuple[str, ...],
    qualifiers: tuple[str, ...] = (),
    operator: str | None = None,
) -> Definition:
    identifier = ".".join(("count.current_zone", *parameters, *qualifiers))
    if operator is not None:
        identifier += "." + operator
        parameters = (*parameters, "offset")
    return Definition(identifier + ".v1", parameters, qualifiers, operator)


# These are the thirteen sealed operand/qualifier shapes, plus the three B03 offsets.
_DEFINITIONS = (
    _definition(("owner", "zone")),
    _definition(("owner", "zone", "kind")),
    _definition(("owner", "zone", "name")),
    _definition(("owner", "zone", "card")),
    _definition(("owner", "zone", "kind", "trait")),
    _definition(("owner", "zone", "card", "trait")),
    _definition(("owner", "zone", "class", "kind")),
    _definition(("owner", "zone", "class", "card")),
    _definition(("owner", "zone", "keyword", "kind")),
    _definition(("owner", "zone", "token_status", "trait"), ("token",)),
    _definition(("owner", "zone", "kind"), ("exclude_this",)),
    _definition(("owner", "zone", "kind", "trait"), ("exclude_this",)),
    _definition(("owner", "zone", "keyword", "kind"), ("exclude_this",)),
    _definition(("owner", "zone", "name"), operator="add_offset"),
    _definition(("owner", "zone"), operator="subtract_offset"),
    _definition(("owner", "zone", "kind"), operator="add_offset"),
)
DEFINITIONS = MappingProxyType(
    {definition.id: definition for definition in _DEFINITIONS}
)
DOMAIN_CODE = "quantity.count_expression.current_zone.v1"
DOMAIN = QuantityDomain(
    type="QuantityExpr",
    modes=(),
    imports=(),
    expressions=tuple(DEFINITIONS),
)


def definition(expr: Expression) -> Definition:
    """Unknown expression IDs cannot acquire meaning from a translated target."""
    if expr.expression_id not in DEFINITIONS:
        raise ValueError("Unknown source expression ID")
    return DEFINITIONS[expr.expression_id]


def verify_import(projection: Projection, name: str, scope: str) -> None:
    """A source-proven use sees only its own scope and ancestors, never a sibling branch."""
    port = next((port for port in projection.imports if port.name == name), None)
    if port is None or port.type not in {"QuantityExpr", "CapturedValue"}:
        raise ValueError("Expression has a missing or mistyped import")
    visible(projection, port, scope)


def verify_quantities(
    frame: Frame,
    binding: SourceBinding,
    *,
    scopes: Mapping[str, str],
    registry: Registry = DEFAULT_REGISTRY,
) -> None:
    """Check dependencies before expansion; source reconstruction supplies use scopes."""
    visited: set[str] = set()
    active: set[str] = set()

    def visit(name: str) -> None:
        if name in active:
            raise ValueError("Cyclic expression dependencies")
        if name in visited:
            return
        if name not in binding.values:
            raise ValueError("Expression refers to a missing actual value")
        active.add(name)
        value = binding.values[name]
        slot = next((s for s in frame.leaf_schema.slots if s.name == name), None)
        if (
            slot is not None
            and slot.expression is not None
            and (value.expr if isinstance(value, QuantitySpec) else value)
            != slot.expression
        ):
            raise ValueError("Expression value differs from its source structure")
        _verify_expression(frame, binding, name, scopes, registry, visit)
        active.remove(name)
        visited.add(name)

    for name in binding.values:
        visit(name)
    for rule in frame.mode_rules:
        verify_mode_count(rule, frame, binding)


def _verify_expression(
    frame: Frame,
    binding: SourceBinding,
    name: str,
    scopes: Mapping[str, str],
    registry: Registry,
    visit: Callable[[str], None],
) -> None:
    value = binding.values[name]
    expr = value.expr if isinstance(value, QuantitySpec) else value
    if isinstance(value, QuantitySpec):
        from sve_carddb.domains.translations.four_layer_n3_context import (  # ruff: ignore[import-outside-top-level] -- avoids a descriptor/registry cycle
            verify_context_choice,
        )

        slot = next((s for s in frame.leaf_schema.slots if s.name == name), None)
        if slot is not None and slot.domain.values == (CHOICE_DOMAIN,):
            verify_context_choice(frame, binding, name, value)
    if isinstance(expr, Expression):
        for operand in expr.leaves:
            visit(operand)
        definition(expr).arguments(expr, frame.leaf_schema)
    elif isinstance(expr, (ContextExpression, PredicateExpr)):
        for operand in registry.verify(frame, binding, name, expr):
            visit(operand)
    elif isinstance(expr, Bound):
        if name not in scopes:
            raise ValueError("Bound expression lacks a source-proven use scope")
        verify_import(frame.projection, expr.import_, scopes[name])


def target_dependencies(
    frame: Frame, binding: SourceBinding, *, registry: Registry = DEFAULT_REGISTRY
) -> dict[str, tuple[str, ...]]:
    """Targets inherit usage through source-bound expression actuals."""
    if frame.source.normalizer_version != VERSION and any(
        isinstance(
            value.expr if isinstance(value, QuantitySpec) else value,
            (Expression, ContextExpression, PredicateExpr),
        )
        for value in binding.values.values()
    ):
        raise ValueError("Expression target dependencies require a v5 frame")
    dependencies = {}
    for name, value in binding.values.items():
        expr = value.expr if isinstance(value, QuantitySpec) else value
        if isinstance(expr, Expression):
            dependencies[name] = expr.leaves
        elif isinstance(expr, (ContextExpression, PredicateExpr)):
            dependencies[name] = registry.verify(frame, binding, name, expr)
    return dependencies


class ExpressionTargets:
    def __init__(
        self,
        targets: Mapping[tuple[str, str], SelectedTarget],
        *,
        registry: Registry = DEFAULT_REGISTRY,
    ) -> None:
        self.targets = dict(targets)
        self.registry = registry
        if any(
            key[0] not in DEFINITIONS and key[0] not in registry.descriptors
            for key in self.targets
        ):
            raise ValueError("Target selects an unknown source expression")

    def import_arguments(
        self,
        expr: ContextExpression | PredicateExpr,
        schema: LeafSchema,
        frame: Frame | None,
    ) -> dict[str, LeafSlot]:
        """Imported source leaves remain required, typed presentation formals."""
        if frame is None:
            raise ValueError("Context presentation requires source-bound imports")
        arguments = {}
        for signature in self.registry.definition(expr).imports:
            port = next(
                (
                    p
                    for p in frame.projection.imports
                    if p.name == expr.imports[signature.name]
                ),
                None,
            )
            if port is None or port.evidence is None:
                raise ValueError("Context presentation lacks assigned provider")
            for index, name in enumerate(port.evidence.source_leaves):
                arguments[f"{signature.name}.{index}"] = next(
                    s for s in schema.slots if s.name == name
                )
        return arguments

    def expand(
        self,
        expr: Expression | ContextExpression | PredicateExpr,
        schema: LeafSchema,
        lang: str,
        forms: Mapping[tuple[str, str], FormDefinition],
        *,
        frame: Frame | None = None,
        binding: SourceBinding | None = None,
    ) -> SelectedTarget:
        """Use normal Target validation on formals, then substitute actual slot names."""
        if binding is not None and frame is None:
            raise ValueError("Source-bound presentation requires its frame")
        arguments = (
            definition(expr).arguments(expr, schema)
            if isinstance(expr, Expression)
            else self.registry.definition(expr).arguments(expr, schema)
        )
        if isinstance(expr, (ContextExpression, PredicateExpr)):
            arguments.update(self.import_arguments(expr, schema, frame))
        selected = self.targets.get((expr.expression_id, lang))
        if selected is None:
            raise MissingValueError("missing_expression_translation")
        formal = LeafSchema(
            format=2,
            slots=tuple(
                slot.model_copy(
                    update={"name": name, "occurrences": (), "required": True}
                )
                for name, slot in arguments.items()
            ),
        )
        target = selected.target
        if any(
            not isinstance(node, (LiteralNode, LeafRef, FormNode))
            for node in target.nodes
        ):
            raise ValueError("Expression targets allow only Literal LeafRef and Form")
        target.verify(formal, lang, forms)
        nodes: list[Node] = []
        for node in target.nodes:
            if isinstance(node, LeafRef):
                nodes.append(
                    node.model_copy(update={"slot": arguments[node.slot].name})
                )
            elif isinstance(node, FormNode):
                nodes.append(
                    node.model_copy(
                        update={
                            "args": {
                                name: SlotRef(slot=arguments[ref.slot].name)
                                for name, ref in node.args.items()
                            }
                        }
                    )
                )
            else:
                nodes.append(node)
        return replace(selected, target=Target(format=1, nodes=tuple(nodes)))


@cache
def _source_registry() -> Registry:
    from sve_carddb.domains.translations.four_layer_integrated import FAMILY  # ruff: ignore[import-outside-top-level] -- source construction imports this module
    from sve_carddb.domains.translations.four_layer_n3_sources import FAMILY as N3  # ruff: ignore[import-outside-top-level] -- source adapters depend on the shared derivation boundary

    return DEFAULT_REGISTRY.extend((*FAMILY.descriptors, *N3.descriptors))


def verify_target_shape(
    frame: Frame,
    target: Target,
    translations: Mapping[str, Target],
    lang: str,
    forms: Mapping[tuple[str, str], FormDefinition],
) -> None:
    """Inactive wording must satisfy every constraint already fixed by its frame."""
    from sve_carddb.domains.translations.four_layer_render import SelectedTarget  # ruff: ignore[import-outside-top-level] -- the renderer imports expression validation

    registry = _source_registry()
    expander = ExpressionTargets(
        {
            (identifier, lang): SelectedTarget(value)
            for identifier, value in translations.items()
        },
        registry=registry,
    )
    used = target.referenced_slots(frame.leaf_schema, lang, forms)
    slots = {s.name: s for s in frame.leaf_schema.slots}
    dependencies: dict[str, tuple[str, ...]] = {}
    identifiers: set[str] = set()
    possible: set[str] = set()
    pending = list(used)
    incomplete = False
    while pending:
        name = pending.pop()
        slot = slots[name]
        expr = slot.expression
        actuals: tuple[str, ...] = ()
        if expr is not None:
            arguments = registry.definition(expr).arguments(expr, frame.leaf_schema)
            arguments.update(expander.import_arguments(expr, frame.leaf_schema, frame))
            actuals = tuple(s.name for s in arguments.values())
            identifiers.add(expr.expression_id)
            if expr.expression_id in translations:
                expander.expand(expr, frame.leaf_schema, lang, forms, frame=frame)
        elif slot.type in {"QuantityExpr", "PredicateExpr"} or slot.domain.values == (
            CHOICE_DOMAIN,
        ):
            # B11 operands are supplied by the binding rather than embedded in the frame.
            incomplete = True
            possible.update(
                DEFINITIONS
                if DOMAIN_CODE in slot.domain.values
                else registry.descriptors
            )
        dependencies[name] = actuals
        pending.extend(set(actuals) - used)
        used.update(actuals)
    if not incomplete:
        target.verify(frame.leaf_schema, lang, forms, dependencies=dependencies)
    if set(translations) - identifiers - possible:
        raise ValueError("Unused expression translation")
    for identifier, translated in translations.items():
        if any(
            not isinstance(n, (LiteralNode, LeafRef, FormNode))
            for n in translated.nodes
        ):
            raise ValueError("Expression targets allow only Literal LeafRef and Form")
        if identifier in DEFINITIONS:
            _verify_b11_target(identifier, translated, lang, forms)


def _verify_b11_target(
    identifier: str,
    target: Target,
    lang: str,
    forms: Mapping[tuple[str, str], FormDefinition],
) -> None:
    formal = LeafSchema(
        format=2,
        slots=tuple(
            LeafSlot(
                name=p.name,
                type=p.type,
                role=p.role,
                required=True,
                occurrences=(),
                domain=(
                    Domain(values=(), min=0, max=9007199254740991)
                    if p.type == "Nat"
                    else Domain(values=(_ARGUMENTS[p.name][1],), min=None, max=None)
                ),
            )
            for p in DEFINITIONS[identifier].signature
        ),
    )
    target.verify(formal, lang, forms)


def verify_expression_selection(  # ruff: ignore[too-many-arguments] -- binding-dependent closure and its sentence-owned wording share the render boundary
    frame: Frame,
    binding: SourceBinding,
    target: Target,
    translations: Mapping[str, Target],
    lang: str,
    forms: Mapping[tuple[str, str], FormDefinition],
    *,
    registry: Registry = DEFAULT_REGISTRY,
) -> None:
    """Only expressions reachable from this selected sentence may carry wording."""
    dependencies = target_dependencies(frame, binding, registry=registry)
    used = target.referenced_slots(frame.leaf_schema, lang, forms)
    pending = list(used)
    identifiers = set()
    while pending:
        name = pending.pop()
        value = binding.values[name]
        expr = value.expr if isinstance(value, QuantitySpec) else value
        if isinstance(expr, (Expression, ContextExpression, PredicateExpr)):
            identifiers.add(expr.expression_id)
        for actual in dependencies.get(name, ()):
            if actual not in used:
                used.add(actual)
                pending.append(actual)
    if set(translations) - identifiers:
        raise ValueError("Unused expression translation")
    if identifiers - set(translations):
        raise MissingValueError("missing_expression_translation")

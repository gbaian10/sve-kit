"""Closed preparation expressions and positional, typed presentation arguments."""

from dataclasses import dataclass, replace
from types import MappingProxyType
from typing import TYPE_CHECKING, override

from sve_carddb.contracts.four_layer import (
    Bound,
    ContextExpression,
    Expression,
    FormNode,
    LeafRef,
    LeafSchema,
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
from sve_carddb.domains.translations.four_layer_n2_forms import validate_parameter_form
from sve_carddb.domains.translations.four_layer_normalizer import PREPARATION_VERSION
from sve_carddb.domains.translations.four_layer_render import (
    MissingValueError,
    Renderer,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Mapping

    from sve_carddb.contracts.four_layer import (
        FormDefinition,
        Frame,
        LeafSlot,
        LeafType,
        Node,
        Projection,
        Reference,
    )
    from sve_carddb.contracts.source_binding import SourceBinding
    from sve_carddb.domains.translations.four_layer_render import (
        BoundTarget,
        Form,
        Result,
        SelectedTarget,
    )

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
        raise ValueError("Unknown preparation expression ID")
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
            and value != slot.expression
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
    """Only preparation targets inherit usage through source-bound expression actuals."""
    if frame.source.normalizer_version != PREPARATION_VERSION:
        raise ValueError("Expression target dependencies require a v3 frame")
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
            raise ValueError("Target selects an unknown preparation expression")

    def import_arguments(
        self,
        expr: ContextExpression | PredicateExpr,
        schema: LeafSchema,
        frame: Frame | None,
        binding: SourceBinding | None,
    ) -> dict[str, LeafSlot]:
        """Imported source leaves remain required, typed presentation formals."""
        if frame is None or binding is None:
            raise ValueError("Context presentation requires source-bound imports")
        arguments = {}
        for signature in self.registry.definition(expr).imports:
            port = next(
                p
                for p in frame.projection.imports
                if p.name == expr.imports[signature.name]
            )
            if port.evidence is None:
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
        arguments = (
            definition(expr).arguments(expr, schema)
            if isinstance(expr, Expression)
            else self.registry.definition(expr).arguments(expr, schema)
        )
        if isinstance(expr, (ContextExpression, PredicateExpr)):
            arguments.update(self.import_arguments(expr, schema, frame, binding))
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


class PreparationRenderer(Renderer):
    def __init__(
        self,
        renderer: Renderer,
        targets: ExpressionTargets,
        *,
        preparation_forms: Mapping[tuple[str, str], Form] | None = None,
    ) -> None:
        super().__init__(
            renderer.labels,
            renderer.code_references,
            renderer.forms,
            domains=renderer.domains,
        )
        for key, form in (preparation_forms or {}).items():
            if key != (form.definition.id, form.definition.lang) or key in self.forms:
                raise ValueError("Preparation form key conflicts with its identity")
            validate_parameter_form(form.definition)
            self.forms[key] = form
        self.expression_targets = targets

    @override
    def expression_target(
        self,
        item: BoundTarget,
        expr: Expression | ContextExpression | PredicateExpr,
        lang: str,
    ) -> SelectedTarget:
        """Keep preparation expression selections outside production renderer inputs."""
        return self.expression_targets.expand(
            expr,
            item.frame.leaf_schema,
            lang,
            {key: form.definition for key, form in self.forms.items()},
            frame=item.frame,
            binding=item.binding,
        )

    @override
    def verify_target(
        self,
        item: BoundTarget,
        lang: str,
        forms: Mapping[tuple[str, str], FormDefinition],
    ) -> None:
        """Expression translations must present all formals, so their actuals count as used."""
        assert item.selected is not None
        item.selected.target.verify(
            item.frame.leaf_schema,
            lang,
            forms,
            dependencies=target_dependencies(
                item.frame, item.binding, registry=self.expression_targets.registry
            ),
        )

    @override
    def references(self, item: BoundTarget, name: str) -> tuple[Reference, ...]:
        """Expression operands contribute their normal required reference closure."""
        value = item.binding.values.get(name)
        expr = value.expr if isinstance(value, QuantitySpec) else value
        if isinstance(expr, (Expression, ContextExpression, PredicateExpr)):
            dependencies = target_dependencies(
                item.frame, item.binding, registry=self.expression_targets.registry
            )
            return tuple(
                ref
                for actual in dependencies.get(name, ())
                for ref in self.references(item, actual)
            )
        return super().references(item, name)

    @override
    def render(
        self, context_id: str, lang: str, plan: tuple[BoundTarget, ...]
    ) -> Result:
        """Validate the preparation dependency graph once at the rendering boundary."""
        for item in plan:
            if item.frame.source.normalizer_version != PREPARATION_VERSION:
                raise ValueError("Preparation rendering requires a v3 frame")
            verify_quantities(
                item.frame,
                item.binding,
                scopes={},
                registry=self.expression_targets.registry,
            )
        return super().render(context_id, lang, plan)

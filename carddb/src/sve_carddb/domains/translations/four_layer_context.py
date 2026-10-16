"""Closed preparation descriptors shared by independently implemented source families."""

from dataclasses import dataclass
from types import MappingProxyType
from typing import TYPE_CHECKING, Literal, cast

from sve_carddb.contracts.four_layer import (
    CapturedValueRef,
    ContextExpression,
    EventRef,
    Expression,
    GlossaryReference,
    ObjectSetRef,
    Port,
    PredicateExpr,
    ReceiptRef,
    ResolvedCondition,
    Signature,
)
from sve_carddb.contracts.source_binding import PreparationDomain

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import (
        CaptureTime,
        Frame,
        LeafSchema,
        LeafSlot,
        LeafType,
        ModeCountRule,
        PortType,
        Projection,
    )
    from sve_carddb.contracts.source_binding import SourceBinding, TypedValue


@dataclass(frozen=True)
class Parameter:
    name: str
    type: LeafType
    role: str
    domain: str
    values: tuple[TypedValue, ...] = ()
    alternate_roles: tuple[str, ...] = ()


@dataclass(frozen=True)
class ImportSignature:
    name: str
    type: PortType
    capture: CaptureTime | tuple[CaptureTime, ...]
    cardinality: Literal["one", "many"] | None = None
    origin: str | None = None
    result_type: Literal["Nat", "Bool"] | None = None
    measure: str | None = None
    event_kind: Literal["EnterField", "Play", "Damage"] | None = None
    subject_import: str | None = None


@dataclass(frozen=True)
class HistorySignature:
    event_kind: str
    aggregate: Literal["count_facts", "count_subjects", "sum", "exists"]
    window: Literal["CurrentTurn"] = "CurrentTurn"
    include_execution: bool = True
    cutoff: Literal["at_query"] = "at_query"


@dataclass(frozen=True)
class Descriptor:
    id: str
    domain: str
    role: str
    result_type: Literal["Nat", "Bool"]
    measure: str
    parameters: tuple[Parameter, ...]
    imports: tuple[ImportSignature, ...]
    source_family: str
    ir_bridge: tuple[str, ...]
    scalable: bool = False
    history: HistorySignature | None = None

    @property
    def signature(self) -> tuple[Signature, ...]:
        """Expose formals to the same expression-target presentation boundary as B11."""
        return tuple(
            Signature(name=p.name, type=p.type, role=p.role) for p in self.parameters
        )

    def arguments(
        self, expr: ContextExpression | PredicateExpr, schema: LeafSchema
    ) -> dict[str, LeafSlot]:
        """Bind source positions, preserving independent equal-valued operands."""
        expected = ContextExpression if self.result_type == "Nat" else PredicateExpr
        if not isinstance(expr, expected) or len(expr.leaves) != len(self.parameters):
            raise ValueError(
                "Expression result type or parameter arity differs from descriptor"
            )
        if set(expr.imports) != {p.name for p in self.imports}:
            raise ValueError("Expression imports differ from descriptor signature")
        slots = {s.name: s for s in schema.slots}
        result = {}
        for param, name in zip(self.parameters, expr.leaves, strict=True):
            slot = slots.get(name)
            if (
                slot is None
                or slot.type != param.type
                or slot.role not in {param.role, *param.alternate_roles}
                or (param.domain and slot.domain.values != (param.domain,))
            ):
                raise ValueError(
                    "Expression argument has the wrong type role or domain"
                )
            result[param.name] = slot
        return result


def visible(projection: Projection, port: Port, scope: str) -> None:
    """A branch can read ancestors within one ability, never sibling or other abilities."""
    parents = {s.id: s.parent for s in projection.scopes}
    if scope not in parents:
        raise ValueError("Expression use has an unknown source scope")
    current: str | None = scope
    while current is not None:
        if current == port.scope:
            return
        node = next(s for s in projection.scopes if s.id == current)
        if node.kind == "ability":
            break
        current = parents[current]
    raise ValueError("Expression import crosses a source branch or ability")


def typed_import(
    frame: Frame,
    binding: SourceBinding,
    name: str,
    scope: str,
    signature: ImportSignature,
) -> Port:
    """Only source-anchored assigned ports supply context; absence is never false or zero."""
    port = next((p for p in frame.projection.imports if p.name == name), None)
    if port is None or port.type != signature.type or port.evidence is None:
        raise ValueError("Expression has a missing or mistyped context provider")
    visible(frame.projection, port, scope)
    evidence = port.evidence
    source = binding.occurrence_key()
    captures = (
        (signature.capture,)
        if isinstance(signature.capture, str)
        else signature.capture
    )
    if (
        evidence.value.kind != port.type
        or evidence.capture not in captures
        or evidence.source.model_dump(exclude={"line_ordinal", "role", "segments"})
        != source.model_dump(exclude={"line_ordinal", "role", "segments"})
    ):
        raise ValueError("Expression provider has incompatible source or capture")
    value = evidence.value
    if isinstance(value, ObjectSetRef):
        if len(set(value.objects)) != len(value.objects):
            raise ValueError("ObjectSet provider contains duplicate generations")
        if signature.cardinality == "one" and len(value.objects) != 1:
            raise ValueError("Expression requires a single object")
        if signature.origin is not None and value.origin != signature.origin:
            raise ValueError("Expression object has the wrong source role")
    if isinstance(value, EventRef) and value.event_kind != signature.event_kind:
        raise ValueError("Expression provider has the wrong event kind")
    if isinstance(value, CapturedValueRef) and (
        value.result_type != signature.result_type or value.measure != signature.measure
    ):
        raise ValueError("Expression capture has the wrong result type or measure")
    return port


def _validate_descriptor(d: Descriptor) -> None:
    if (
        not d.domain.startswith(("quantity.", "predicate."))
        or ".constant." in d.domain
        or d.domain == "quantity.count_expression.current_zone.v1"
    ):
        raise ValueError("Descriptor cannot widen a protected leaf domain")
    if not d.ir_bridge or not d.source_family or not d.measure:
        raise ValueError("Descriptor requires source, measure and IR bridge")
    for names in (
        tuple(p.name for p in d.parameters),
        tuple(p.name for p in d.imports),
    ):
        if len(set(names)) != len(names):
            raise ValueError("Descriptor signature has duplicate formals")
    _validate_imports(d.imports)
    if d.history is not None:
        expected_measure = {
            "count_subjects": "card_count",
            "count_facts": "event_count",
        }.get(d.history.aggregate)
        if (d.history.aggregate == "exists") != (d.result_type == "Bool") or (
            expected_measure is not None and d.measure != expected_measure
        ):
            raise ValueError("History aggregate disagrees with its result measure")
    if d.result_type == "Bool" and d.measure != "truth":
        raise ValueError("Predicate descriptor requires truth measure")


def _validate_imports(signatures: tuple[ImportSignature, ...]) -> None:
    imports = {p.name: p for p in signatures}
    for signature in signatures:
        if signature.type == "ObjectSet" and signature.cardinality is None:
            raise ValueError("ObjectSet signature requires explicit cardinality")
        if signature.type == "CapturedValue" and (
            signature.result_type is None or signature.measure is None
        ):
            raise ValueError("CapturedValue signature requires result type and measure")
        if signature.type == "EventRef" and signature.event_kind is None:
            raise ValueError("EventRef signature requires a closed event kind")
        if signature.subject_import is not None and (
            signature.type != "EventRef"
            or signature.subject_import not in imports
            or imports[signature.subject_import].type != "ObjectSet"
            or imports[signature.subject_import].cardinality != "one"
        ):
            raise ValueError(
                "Event subject signature requires a single ObjectSet import"
            )


def _event_subjects(rule: Descriptor, ports: Mapping[str, Port]) -> None:
    for signature in rule.imports:
        if signature.subject_import is None:
            continue
        fact = ports[signature.name].evidence
        subject = ports[signature.subject_import].evidence
        if (
            fact is None
            or subject is None
            or not isinstance(fact.value, EventRef)
            or not isinstance(subject.value, ObjectSetRef)
            or subject.value.objects != (fact.value.subject,)
        ):
            raise ValueError("Expression event subject generation mismatch")


class Registry:
    def __init__(self, descriptors: tuple[Descriptor, ...]) -> None:
        by_id = {d.id: d for d in descriptors}
        if len(by_id) != len(descriptors):
            raise ValueError("Duplicate preparation descriptor")
        for d in descriptors:
            _validate_descriptor(d)
        self.descriptors = MappingProxyType(by_id)
        domains: dict[str, PreparationDomain] = {}
        for d in descriptors:
            type_name: Literal["QuantityExpr", "PredicateExpr"] = (
                "QuantityExpr" if d.result_type == "Nat" else "PredicateExpr"
            )
            if d.domain in domains and domains[d.domain].type != type_name:
                raise ValueError("Preparation domain mixes result types")
            ids = tuple(sorted(x.id for x in descriptors if x.domain == d.domain))
            domains[d.domain] = PreparationDomain(type=type_name, expressions=ids)
        self.domains = MappingProxyType(domains)

    def extend(self, descriptors: tuple[Descriptor, ...]) -> Registry:
        """Family-owned modules supply explicit sealed additions without mutable globals."""
        return Registry((*self.descriptors.values(), *descriptors))

    def definition(self, expr: ContextExpression | PredicateExpr) -> Descriptor:
        """Unknown source descriptors cannot acquire semantics from target wording."""
        if expr.expression_id not in self.descriptors:
            raise ValueError("Unknown preparation context descriptor")
        return self.descriptors[expr.expression_id]

    def verify(
        self,
        frame: Frame,
        binding: SourceBinding,
        name: str,
        expr: ContextExpression | PredicateExpr,
    ) -> tuple[str, ...]:
        """Validate one node; the shared expression walker owns the dependency DAG."""
        rule = self.definition(expr)
        if expr.scope not in {s.id for s in frame.projection.scopes}:
            raise ValueError("Expression has an unknown source scope")
        arguments = rule.arguments(expr, frame.leaf_schema)
        slot = next((s for s in frame.leaf_schema.slots if s.name == name), None)
        if (
            slot is None
            or slot.expression != expr
            or slot.type
            != ("QuantityExpr" if rule.result_type == "Nat" else "PredicateExpr")
            or slot.domain.values != (rule.domain,)
            or slot.role != rule.role
        ):
            raise ValueError("Expression structure differs from its source leaf schema")
        for actual in expr.leaves:
            child = binding.values.get(actual)
            if isinstance(child, (ContextExpression, PredicateExpr)):
                self._visible_dependency(frame.projection, child.scope, expr.scope)
        for param in rule.parameters:
            if (
                param.values
                and binding.values.get(arguments[param.name].name) not in param.values
            ):
                raise ValueError("Expression operand is outside the sealed descriptor")
        ports = {
            sig.name: typed_import(
                frame, binding, expr.imports[sig.name], expr.scope, sig
            )
            for sig in rule.imports
        }
        _event_subjects(rule, ports)
        if rule.source_family == "A11":
            self._entry(ports)
        self._verify_operands(rule, expr, binding, ports)
        return (
            *expr.leaves,
            *(
                leaf
                for p in ports.values()
                if p.evidence
                for leaf in p.evidence.source_leaves
            ),
        )

    def _verify_operands(
        self,
        rule: Descriptor,
        expr: ContextExpression | PredicateExpr,
        binding: SourceBinding,
        ports: Mapping[str, Port],
    ) -> None:
        if rule.id == "quantity.scale.once.v1":
            base = self.unwrapped(binding.values.get(expr.leaves[0]), binding)
            if not self.definition(base).scalable:
                raise ValueError("Unregistered or nested constant multiplier")
            factor = binding.values.get(expr.leaves[1])
            if type(factor) is not int or factor < 1:
                raise ValueError("Multiplier requires a positive Nat")
        if rule.id.startswith("condition.quantity."):
            value = binding.values.get(expr.leaves[0])
            if not isinstance(value, (ContextExpression, Expression)):
                raise ValueError("Quantity condition requires a typed numeric read")
            if (
                isinstance(value, ContextExpression)
                and self.definition(value).result_type != "Nat"
            ):
                raise ValueError("Quantity condition cannot consume a predicate")
        if rule.id == "read.receipt.amount.v1":
            receipt = ports["receipt"].evidence
            if (
                receipt is None
                or not isinstance(receipt.value, ReceiptRef)
                or receipt.value.result != "amount"
            ):
                raise ValueError("Receipt read requires an amount result")

    def unwrapped(
        self, value: TypedValue | None, binding: SourceBinding
    ) -> ContextExpression:
        """Aliases do not hide nested arithmetic or change a read's result measure."""
        visited: set[str] = set()
        while (
            isinstance(value, ContextExpression)
            and value.expression_id == "read.quantity.v1"
        ):
            if not value.leaves or value.leaves[0] in visited:
                raise ValueError("Cyclic or missing quantity alias")
            visited.add(value.leaves[0])
            value = binding.values.get(value.leaves[0])
        if (
            not isinstance(value, ContextExpression)
            or value.expression_id not in self.descriptors
        ):
            raise ValueError("Unregistered constant multiplier operand")
        return value

    def result_measure(
        self, expr: ContextExpression | PredicateExpr, binding: SourceBinding
    ) -> str:
        """A consumer's damage/recovery position never changes the operand's dimension."""
        rule = self.definition(expr)
        if rule.measure != "operand":
            return rule.measure
        base = self.unwrapped(binding.values.get(expr.leaves[0]), binding)
        return self.definition(base).measure

    @staticmethod
    def _visible_dependency(projection: Projection, source: str, use: str) -> None:
        visible(
            projection,
            Port(
                name="dependency",
                type="QuantityExpr",
                source_role="read_value",
                scope=source,
            ),
            use,
        )

    @staticmethod
    def _entry(ports: Mapping[str, Port]) -> None:
        subject = ports["entry_subject"].evidence
        event = ports["entry_fact"].evidence
        if subject is None or event is None:
            raise ValueError("Entry predicate requires assigned context")
        obj, fact = subject.value, event.value
        if (
            not isinstance(obj, ObjectSetRef)
            or not isinstance(fact, EventRef)
            or fact.event_kind != "EnterField"
            or obj.objects != (fact.subject,)
        ):
            raise ValueError("Entry event kind or subject generation mismatch")
        if "ability_source" in ports:
            source = ports["ability_source"].evidence
            if (
                source is None
                or not isinstance(source.value, ObjectSetRef)
                or source.value.objects != (fact.direct_ability_source,)
            ):
                raise ValueError(
                    "Entry trait requires the direct ability source snapshot"
                )


def _read(
    id_: str,
    measure: str,
    imports: tuple[ImportSignature, ...],
    parameters: tuple[Parameter, ...] = (),
) -> Descriptor:
    return Descriptor(
        id_,
        "quantity.read.context.v1",
        "read_value",
        "Nat",
        measure,
        parameters,
        imports,
        "A1",
        {
            "read.object.attack.v1": ("V.stats",),
            "read.object.original_cost.v1": ("V.cost",),
            "read.object.effective_cost.v1": ("V.cost",),
            "read.player.pp_maximum.v1": ("V.player-resource",),
            "read.receipt.amount.v1": ("V.receipt-field",),
            "read.captured.dice.v1": ("V.binding",),
            "read.object.counter.v1": ("V.counter",),
            "read.faceup_evolve.count.v1": ("V.zone-count", "V.state", "X.aggregate"),
            "read.quantity.v1": ("V.binding",),
            "read.parameter.chosen_x.v1": ("D.play-param", "V.binding"),
        }[id_],
        scalable=True,
    )


_OBJECT = ImportSignature("subject", "ObjectSet", "live", "one")
_PLAYER = ImportSignature("actor", "PlayerRef", "current_turn")
_THRESHOLD = Parameter("threshold", "Nat", "comparison_threshold", "")
_VALUE = Parameter(
    "value",
    "QuantityExpr",
    "read_value",
    "",
    alternate_roles=("history_read_value", "distinct_name_count", "count_expression"),
)
_ZONE = Parameter(
    "zone", "ZoneSet", "entry_from_zone", "zone.single.v1", (("hand",), ("graveyard",))
)
_TRAIT = Parameter(
    "trait",
    "Concept",
    "entry_source_trait",
    "concept.trait.v1",
    (GlossaryReference(kind="glossary", key="term:trait.umamusume"),),
)
_READS = (
    _read("read.object.attack.v1", "attack", (_OBJECT,)),
    _read("read.object.original_cost.v1", "cost", (_OBJECT,)),
    _read("read.object.effective_cost.v1", "cost", (_OBJECT,)),
    _read(
        "read.player.pp_maximum.v1",
        "pp",
        (ImportSignature("player", "PlayerRef", "live"),),
    ),
    _read(
        "read.receipt.amount.v1",
        "amount",
        (ImportSignature("receipt", "ReceiptId", "receipt"),),
    ),
    _read(
        "read.captured.dice.v1",
        "die_face",
        (
            ImportSignature(
                "value",
                "CapturedValue",
                "receipt",
                result_type="Nat",
                measure="die_face",
            ),
        ),
    ),
    _read(
        "read.object.counter.v1",
        "counter_count",
        (_OBJECT,),
        (Parameter("counter", "Concept", "read_counter", "concept.rule_term.v1"),),
    ),
    _read(
        "read.faceup_evolve.count.v1",
        "card_count",
        (),
        (
            Parameter("owner", "Player", "read_owner", "player.relative.v1"),
            Parameter(
                "zone", "ZoneSet", "read_zone", "zone.single.v1", (("evolve_deck",),)
            ),
            Parameter("name", "CardName", "read_name", "card_name.any.v1"),
        ),
    ),
    _read("read.quantity.v1", "operand", (), (_VALUE,)),
    _read(
        "read.parameter.chosen_x.v1",
        "declared_x",
        (
            ImportSignature(
                "value",
                "CapturedValue",
                "play_parameter",
                result_type="Nat",
                measure="declared_x",
            ),
        ),
    ),
)
_HISTORY = tuple(
    Descriptor(
        f"history.{event}.{aggregate}.v1",
        "predicate.history.current_turn.v1"
        if aggregate == "exists"
        else "quantity.history.current_turn.v1",
        "history_condition" if aggregate == "exists" else "history_read_value",
        "Bool" if aggregate == "exists" else "Nat",
        "truth" if aggregate == "exists" else measure,
        (),
        (_PLAYER,),
        "A2",
        ("V.history", "H.query", "H.window"),
        scalable=aggregate != "exists",
        history=HistorySignature(
            event,
            cast(
                "Literal['count_facts', 'count_subjects', 'sum', 'exists']", aggregate
            ),
        ),
    )
    for event, aggregates in (
        ("attack", (("count_facts", "event_count"),)),
        ("evolve", (("count_facts", "event_count"), ("exists", "truth"))),
        (
            "play",
            (
                ("count_facts", "event_count"),
                ("count_subjects", "card_count"),
                ("exists", "truth"),
            ),
        ),
        ("discard", (("count_subjects", "card_count"), ("exists", "truth"))),
        ("destroy", (("count_subjects", "card_count"), ("exists", "truth"))),
        ("banish", (("count_subjects", "card_count"), ("exists", "truth"))),
        ("field_to_graveyard", (("count_subjects", "card_count"),)),
        ("return_to_hand", (("count_subjects", "card_count"),)),
        ("life_increase", (("count_facts", "event_count"), ("sum", "life"))),
        ("life_decrease", (("count_facts", "event_count"), ("sum", "life"))),
        ("self_buff", (("count_facts", "event_count"),)),
        ("dice", (("count_facts", "event_count"),)),
        ("ub", (("exists", "truth"),)),
        ("earth_rite", (("exists", "truth"),)),
    )
    for aggregate, measure in aggregates
)
_ENTRY = tuple(
    Descriptor(
        id_,
        "predicate.entry.provenance.v1",
        "entry_condition",
        "Bool",
        "truth",
        parameters,
        (
            ImportSignature("entry_subject", "ObjectSet", "at_event", "one", origin),
            ImportSignature(
                "entry_fact",
                "EventRef",
                "at_event",
                event_kind="EnterField",
                subject_import="entry_subject",
            ),
            *(
                (
                    ImportSignature(
                        "ability_source",
                        "ObjectSet",
                        ("at_event", "lki"),
                        "one",
                        "direct_ability_source",
                    ),
                )
                if trait
                else ()
            ),
        ),
        "A11",
        ("E.enter-field", "V.event-field", "X.predicate"),
    )
    for id_, origin, parameters, trait in (
        ("entry.self.from_zone.eq.v1", "holder", (_ZONE,), False),
        (
            "entry.self.from_zone.ne.v1",
            "holder",
            (
                Parameter(
                    "zone", "ZoneSet", "entry_from_zone", "zone.single.v1", (("hand",),)
                ),
            ),
            False,
        ),
        (
            "entry.event_subject.from_zone.eq.v1",
            "event_subject",
            (
                Parameter(
                    "zone",
                    "ZoneSet",
                    "entry_from_zone",
                    "zone.single.v1",
                    (("graveyard",),),
                ),
            ),
            False,
        ),
        ("entry.self.method.ability.v1", "holder", (), False),
        ("entry.self.method.ability_source_trait.v1", "holder", (_TRAIT,), True),
    )
)
_CONDITIONS = tuple(
    Descriptor(
        f"condition.quantity.{operator}.v1",
        "predicate.quantity.compare.v1",
        "quantity_condition",
        "Bool",
        "truth",
        (_VALUE, _THRESHOLD),
        (),
        "A7",
        ("X.predicate",),
    )
    for operator in ("at_least", "equal")
)
_DISTINCT = Descriptor(
    "name.distinct_count.objects.v1",
    "quantity.count.distinct_name.v1",
    "distinct_name_count",
    "Nat",
    "name_count",
    (),
    (ImportSignature("objects", "ObjectSet", "selection", "many"),),
    "A3",
    ("V.name", "X.aggregate"),
)
_SCALE = Descriptor(
    "quantity.scale.once.v1",
    "quantity.scale.once.v1",
    "read_value",
    "Nat",
    "operand",
    (_VALUE, Parameter("factor", "Nat", "constant_multiplier", "")),
    (),
    "A1/A2",
    ("X.math.Mul",),
)
DEFAULT_REGISTRY = Registry(
    (*_READS, *_HISTORY, *_ENTRY, *_CONDITIONS, _DISTINCT, _SCALE)
)


def verify_mode_count(
    rule: ModeCountRule,
    frame: Frame,
    binding: SourceBinding,
) -> None:
    """Mode selection consumes a typed predicate or keeps an explicit source pending."""
    typed_import(
        frame,
        binding,
        rule.choice_context,
        rule.scope,
        ImportSignature("choice", "ChoiceContext", "play_parameter"),
    )
    if isinstance(rule.condition, ResolvedCondition):
        predicate = binding.values.get(rule.condition.predicate)
        if (
            not isinstance(predicate, PredicateExpr)
            or rule.condition.predicate not in binding.values
        ):
            raise ValueError("Mode count condition requires a typed predicate")
        if predicate.scope != rule.scope:
            raise ValueError("Mode count condition crosses a source scope")

"""Render typed source bindings without parsing placeholders or rediscovering references."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.annotations import (
    Annotation,
    AnnotationSet,
    RenderLeafOccurrence,
)
from sve_carddb.contracts.four_layer import (
    Bound,
    CardNameReference,
    CardNP,
    Constant,
    CountExpr,
    Expression,
    FormDefinition,
    FormNode,
    GlossaryReference,
    LabelPart,
    LeafRef,
    LiteralNode,
    QuantitySpec,
    QuantityValuePart,
    SlotRef,
    Span,
    UnionNP,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.core.json import canonical, digest

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import Frame, LeafType, Node, Reference, Target
    from sve_carddb.contracts.source_binding import (
        ClosedDomain,
        SourceBinding,
        TypedValue,
    )

RENDERER_VERSION = "four-layer-renderer-v1"
ZONE_CASES = frozenset(
    {"battlefield", "deck", "evolve_deck", "ex", "graveyard", "hand", "mixed"}
)


@dataclass(frozen=True)
class Label:
    reference: Reference
    lang: str
    text: str
    origin: Literal["official", "project", "machine"]
    low_confidence: bool
    bold: bool | None
    variant_key: str = "default"

    def __post_init__(self) -> None:
        """A blank label would lose the semantic range of its source leaf."""
        if not self.text or self.origin not in {"official", "project", "machine"}:
            raise ValueError("Invalid selected four-layer label")
        if (
            isinstance(self.reference, CardNameReference)
            or (
                isinstance(self.reference, VocabularyReference)
                and self.reference.key[0] in {"class", "type"}
            )
        ) and self.bold is not True:
            raise ValueError("Card names and class/type labels require bold=true")
        self.text.encode("utf-8")


@dataclass(frozen=True)
class Form:
    definition: FormDefinition
    origin: Literal["official", "project", "machine"] = "project"
    low_confidence: bool = False


@dataclass(frozen=True)
class SelectedTarget:
    target: Target
    origin: Literal["official", "project", "machine"] = "project"
    low_confidence: bool = False
    variant_key: str = "default"


@dataclass(frozen=True)
class BoundTarget:
    frame: Frame
    binding: SourceBinding
    selected: SelectedTarget | None
    source_low_confidence: bool = False


@dataclass(frozen=True)
class LeafOutput:
    binding_id: str
    node_path: tuple[int, ...]
    slot: str
    source_ordinals: tuple[int, ...]
    ranges: tuple[Span, ...]


@dataclass(frozen=True)
class Rendered:
    context_id: str
    target_lang: str
    text: str
    annotation: AnnotationSet
    dependency_key: str
    origin: Literal["project", "machine"]
    low_confidence: bool
    leaves: tuple[LeafOutput, ...]

    def identity(self) -> tuple[str, int]:
        """Quality and actual presentation dependencies participate in render-v3."""
        checksum = hash_payload(
            {
                "recipe": "render-v3",
                "context_id": self.context_id,
                "target_lang": self.target_lang,
                "dependency_key": self.dependency_key,
                "text": self.text,
                "origin": self.origin,
                "authority": "unofficial",
                "low_confidence": self.low_confidence,
            }
        )
        return "tr:" + checksum, int(checksum[:13], 16)

    def occurrences(self) -> tuple[RenderLeafOccurrence, ...]:
        """Keep each repeated target path linked to its exact source leaf occurrences."""
        identifier, _ = self.identity()
        return tuple(
            RenderLeafOccurrence(
                translation_id=identifier,
                binding_id=leaf.binding_id,
                node_path=leaf.node_path,
                slot=leaf.slot,
                source_ordinals=leaf.source_ordinals,
                ranges=leaf.ranges,
            )
            for leaf in self.leaves
        )

    def verify_occurrences(self, rows: tuple[RenderLeafOccurrence, ...]) -> None:
        """A persisted subset cannot erase a repeated target path or its source link."""

        def key(row: RenderLeafOccurrence) -> tuple[str, tuple[int, ...], str]:
            return row.binding_id, row.node_path, row.slot

        if sorted(rows, key=key) != sorted(self.occurrences(), key=key):
            raise ValueError("Missing or mismatched render occurrence")


@dataclass(frozen=True)
class Result:
    rendered: Rendered | None
    issues: tuple[str, ...]


class MissingValueError(Exception):
    """Only absent usable values cause whole-field fallback; invalid structure still fails."""


def validate_form(form: FormDefinition) -> None:
    """Rules and branch sets are finite and pinned independently of editable Chinese parts."""
    expected = {
        "zone.case.v1": ZONE_CASES,
        "keyword.display.v1": frozenset({"default"}),
        "quantity.classifier.v1": frozenset({"card", "object"}),
        "quantity.constant_value.v1": frozenset({"default"}),
    }.get(form.rule)
    if expected is None:
        raise ValueError("Unknown four-layer form rule")
    if set(form.cases) != expected:
        raise ValueError("Form must contain exactly its registered rule cases")
    types = {arg.name: arg.type for arg in form.signature}
    if form.rule == "zone.case.v1" and types != {"zone": "ZoneSet"}:
        raise ValueError("Zone form requires its exact typed signature")
    if form.rule == "keyword.display.v1" and types != {"term": "Concept"}:
        raise ValueError("Keyword form requires its exact typed signature")
    if form.rule == "quantity.classifier.v1" and types != {
        "kind": "CardKind",
        "quantity": "QuantitySpec",
        "zone": "ZoneSet",
    }:
        raise ValueError("Classifier form requires its exact typed signature")
    if form.rule == "quantity.classifier.v1" and any(
        isinstance(part, LabelPart) for parts in form.cases.values() for part in parts
    ):
        raise ValueError(
            "Classifier rule supplies a unit, not another copy of its arguments"
        )
    if form.rule == "quantity.constant_value.v1":
        roles = {
            "selection_count",
            "choice_mode_count",
            "existence_count",
            "counter_amount",
            "repeat_count",
        }
        if (
            len(form.signature) != 1
            or form.signature[0].type != "QuantitySpec"
            or form.signature[0].role not in roles
            or form.id != f"quantity.{form.signature[0].role}.value"
            or form.cases["default"]
            != (QuantityValuePart(kind="QuantityValue", arg=form.signature[0].name),)
        ):
            raise ValueError(
                "Constant quantity form requires its exact role and value part"
            )


class Renderer:
    def __init__(
        self,
        labels: Mapping[tuple[bytes, str], Label],
        code_references: Mapping[tuple[LeafType, str], Reference],
        forms: Mapping[tuple[str, str], Form],
        *,
        domains: Mapping[str, ClosedDomain] | None = None,
    ) -> None:
        self.labels = dict(labels)
        self.code_references = dict(code_references)
        self.forms = dict(forms)
        self.domains = dict(domains or {})
        for key, value in self.labels.items():
            if key != (canonical(value.reference.model_dump(mode="json")), value.lang):
                raise ValueError(
                    "Selected label key differs from its concept or language"
                )
        for form_key, selected_form in self.forms.items():
            if form_key != (selected_form.definition.id, selected_form.definition.lang):
                raise ValueError("Selected form key differs from its identity")
            validate_form(selected_form.definition)

    def render(
        self, context_id: str, lang: str, plan: tuple[BoundTarget, ...]
    ) -> Result:
        """Only a complete field renders; unavailable labels preserve whole-field fallback."""
        if tuple(p.binding.ordinal for p in plan) != tuple(range(len(plan))):
            raise ValueError("Rendered bindings must preserve complete source order")
        definitions = {key: value.definition for key, value in self.forms.items()}
        for item in plan:
            item.binding.verify(item.frame, self.domains)
            if item.selected is None:
                return Result(None, ("missing_template_translation",))
            item.selected.target.verify(item.frame.leaf_schema, lang, definitions)
        output = _Output(self, lang)
        try:
            for item in _assembly(plan):
                assert item.selected is not None
                output.dependencies.append(
                    {
                        "binding": item.binding.model_dump(
                            mode="json",
                            include={
                                "frame_id",
                                "values",
                                "source_span",
                                "occurrences",
                                "trace",
                            },
                        )
                        | {"source_hash": item.binding.source.source_hash},
                        "target": item.selected.target.model_dump(mode="json"),
                        "variant_key": item.selected.variant_key,
                        "origin": item.selected.origin,
                        "low_confidence": item.selected.low_confidence,
                        "source_low_confidence": item.source_low_confidence,
                    }
                )
                output.quality.append(
                    (
                        item.selected.origin,
                        item.selected.low_confidence or item.source_low_confidence,
                    )
                )
                for index, node in enumerate(item.selected.target.nodes):
                    output.node(item, node, (index,))
        except MissingValueError as error:
            return Result(None, (str(error),))
        return Result(output.finish(context_id), tuple(sorted(output.issues)))


def _assembly(plan: tuple[BoundTarget, ...]) -> tuple[BoundTarget, ...]:
    ordered: list[BoundTarget] = []
    for item in plan:
        anchor = item.binding.source_span.anchor
        if anchor is not None:
            if anchor >= len(plan) or plan[anchor].binding.source_span.role != "body":
                raise ValueError("Rendered reminder has an invalid body anchor")
            continue
        ordered.append(item)
        ordered.extend(
            p for p in plan if p.binding.source_span.anchor == item.binding.ordinal
        )
    if len(ordered) != len(plan):
        raise ValueError("Rendered reminder assembly loses a source binding")
    return tuple(ordered)


class _Output:
    def __init__(self, renderer: Renderer, lang: str) -> None:
        self.renderer = renderer
        self.lang = lang
        self.text = ""
        self.annotations: list[Annotation] = []
        self.leaves: list[LeafOutput] = []
        self.dependencies: list[object] = []
        self.quality: list[tuple[str, bool]] = []
        self.issues: set[str] = set()

    def node(self, item: BoundTarget, node: Node, path: tuple[int, ...]) -> None:
        if isinstance(node, LiteralNode):
            self.text += node.text
        elif isinstance(node, LeafRef):
            self.leaf(item, node.slot, path)
        elif isinstance(node, FormNode):
            self.form(item, node, path)
        elif isinstance(node, CardNP):
            self.card(item, node, path)
        elif isinstance(node, UnionNP):
            for index, branch in enumerate(node.args.branches):
                if index:
                    self.text += "或"
                self.card(item, branch, (*path, index))
            if node.args.quantity is not None:
                self.text += "共"
                self.leaf(
                    item, node.args.quantity.slot, (*path, len(node.args.branches))
                )
        elif isinstance(node, CountExpr):
            self.leaf(item, node.args.expr.slot, (*path, 0))

    def label(self, reference: Reference) -> None:
        key = (canonical(reference.model_dump(mode="json")), self.lang)
        value = self.renderer.labels.get(key)
        if value is None:
            raise MissingValueError("missing_term_translation")
        start = len(self.text)
        self.text += value.text
        self.annotations.append(
            Annotation(
                ordinal=len(self.annotations),
                reference=reference,
                ranges=(Span(start=start, end=len(self.text)),),
                bold=value.bold,
            )
        )
        self.dependencies.append(
            {
                "label": reference.model_dump(mode="json"),
                "lang": self.lang,
                "text": value.text,
                "origin": value.origin,
                "low_confidence": value.low_confidence,
                "bold": value.bold,
                "variant_key": value.variant_key,
            }
        )
        self.quality.append((value.origin, value.low_confidence))
        if value.bold is None:
            self.issues.add("missing_emphasis")

    @staticmethod
    def value(item: BoundTarget, name: str) -> TypedValue:
        if name not in item.binding.values:
            raise MissingValueError("missing_leaf_value")
        return item.binding.values[name]

    def leaf(self, item: BoundTarget, name: str, path: tuple[int, ...]) -> None:
        slot = next(s for s in item.frame.leaf_schema.slots if s.name == name)
        value = self.value(item, name)
        start = len(self.text)
        if type(value) is int:
            self.text += str(value)
        elif isinstance(
            value, (GlossaryReference, VocabularyReference, CardNameReference)
        ):
            self.label(value)
        elif isinstance(value, tuple):
            for index, zone in enumerate(value):
                if index:
                    self.text += "與"
                self.code(slot.type, zone)
        elif isinstance(value, QuantitySpec):
            self.quantity(value)
        elif isinstance(value, (Bound, Constant, Expression)):
            self.expression(value)
        elif isinstance(value, str):
            if slot.type == "LiteralLayout":
                self.text += value
            else:
                self.code(slot.type, value)
        else:
            raise ValueError("Typed leaf cannot be presented as a generic string")
        self.record_leaf(item, name, path, start)

    def record_leaf(
        self, item: BoundTarget, name: str, path: tuple[int, ...], start: int
    ) -> None:
        if len(self.text) > start:
            self.leaves.append(
                LeafOutput(
                    item.binding.id,
                    path,
                    name,
                    tuple(
                        o.ordinal for o in item.binding.occurrences if o.slot == name
                    ),
                    (Span(start=start, end=len(self.text)),),
                )
            )

    def code(self, type_name: LeafType, code: str) -> None:
        reference = self.renderer.code_references.get((type_name, code))
        if reference is None:
            raise ValueError("Typed code lacks its registered concept reference")
        self.label(reference)

    def expression(self, value: Bound | Constant | Expression) -> None:
        if isinstance(value, Constant):
            self.text += str(value.value)
        else:
            raise MissingValueError("unsupported_quantity_presentation")

    def quantity(self, value: QuantitySpec) -> None:
        if value.mode in {"all", "any"}:
            self.text += {"all": "所有", "any": "任意數量的"}[value.mode]
        else:
            self.text += {"exact": "", "up_to": "至多", "at_least": "至少"}[value.mode]
            assert value.expr is not None
            self.expression(value.expr)

    def form(self, item: BoundTarget, node: FormNode, path: tuple[int, ...]) -> None:
        selected = self.renderer.forms[node.form_id, self.lang]
        form = selected.definition
        form.verify_arguments(
            node.args, {s.name: s for s in item.frame.leaf_schema.slots}
        )
        if form.rule == "zone.case.v1":
            zone = self.value(item, node.args["zone"].slot)
            if not isinstance(zone, tuple):
                raise ValueError("Zone form has a non-zone value")
            case = zone[0] if len(zone) == 1 else "mixed"
        elif form.rule == "quantity.classifier.v1":
            case = _classifier(
                self.value(item, node.args["kind"].slot),
                self.value(item, node.args["zone"].slot),
            )
        else:
            case = "default"
        if case not in form.cases:
            raise ValueError("Typed form value is outside its registered cases")
        self.dependencies.append(
            {
                "form": form.model_dump(mode="json"),
                "origin": selected.origin,
                "low_confidence": selected.low_confidence,
            }
        )
        self.quality.append((selected.origin, selected.low_confidence))
        for index, part in enumerate(form.cases[case]):
            if isinstance(part, LabelPart):
                self.leaf(item, node.args[part.arg].slot, (*path, index))
            elif isinstance(part, QuantityValuePart):
                name = node.args[part.arg].slot
                value = self.value(item, name)
                start = len(self.text)
                if isinstance(value, QuantitySpec) and isinstance(value.expr, Constant):
                    self.text += str(value.expr.value)
                else:
                    raise TypeError("QuantityValue requires a constant quantity")
                self.record_leaf(item, name, (*path, index), start)
            else:
                self.text += part.text

    def card(self, item: BoundTarget, node: CardNP, path: tuple[int, ...]) -> None:
        args = node.args
        for index, ref in enumerate((args.owner, args.zone)):
            if ref is not None:
                self.leaf(item, ref.slot, (*path, index))
                self.text += "的"
        quantity = self.value(item, args.quantity.slot)
        self.leaf(item, args.quantity.slot, (*path, 2, 0))
        if isinstance(quantity, QuantitySpec) and quantity.mode not in {"all", "any"}:
            zones = [
                slot.name
                for slot in item.frame.leaf_schema.slots
                if slot.type == "ZoneSet" and slot.role == "counted_zone"
            ]
            if len(zones) != 1:
                raise MissingValueError("unresolved_classifier_context")
            if ("quantity.classifier", self.lang) not in self.renderer.forms:
                raise MissingValueError("missing_classifier_form")
            self.form(
                item,
                FormNode(
                    kind="Form",
                    form_id="quantity.classifier",
                    args={
                        "kind": args.kind,
                        "quantity": args.quantity,
                        "zone": SlotRef(slot=zones[0]),
                    },
                ),
                (*path, 2, 1),
            )
        modifiers = [*args.traits]
        if args.class_ is not None:
            modifiers.append(args.class_)
        if args.token is not None:
            modifiers.append(args.token)
        for index, ref in enumerate(modifiers):
            self.leaf(item, ref.slot, (*path, 3, index))
        self.leaf(item, args.kind.slot, (*path, 4))

    def finish(self, context_id: str) -> Rendered:
        text_id = "t:" + self.lang + ":" + digest(self.text.encode())[7:23]
        occurrences = [a.model_dump(mode="json") for a in self.annotations]
        annotation = AnnotationSet(
            id="ann:"
            + hash_payload(
                {
                    "recipe": "annotation-v1",
                    "text_unit_id": text_id,
                    "occurrences": occurrences,
                }
            ),
            text_unit_id=text_id,
            occurrences=tuple(self.annotations),
        )
        annotation.verify(text_id, self.lang, self.text)
        return Rendered(
            context_id,
            self.lang,
            self.text,
            annotation,
            hash_payload(
                {
                    "renderer_version": RENDERER_VERSION,
                    "dependencies": self.dependencies,
                }
            ),
            "machine"
            if any(origin == "machine" for origin, _ in self.quality)
            else "project",
            any(low for _, low in self.quality),
            tuple(self.leaves),
        )


def _classifier(kind: TypedValue, zone: TypedValue) -> str:
    if not isinstance(kind, VocabularyReference) or kind.key[0] != "type":
        raise ValueError("Classifier requires a typed card kind")
    if not isinstance(zone, tuple) or not zone:
        raise ValueError("Classifier requires the counted source zones")
    if kind.key[1] not in {"follower", "spell", "amulet"}:
        raise MissingValueError("unsupported_classifier_kind")
    return "object" if zone in {("battlefield",), ("ex",)} else "card"

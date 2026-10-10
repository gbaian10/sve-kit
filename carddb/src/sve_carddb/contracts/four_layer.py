"""Closed four-layer authored values shared by source, DB and rendering boundaries."""

from itertools import pairwise
from typing import TYPE_CHECKING, Annotated, Literal, Self

from pydantic import ConfigDict, Field, field_validator, model_validator

from sve_carddb.contracts.n0 import verify_frame
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData, UInt

if TYPE_CHECKING:
    from collections.abc import Mapping

Code = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")]
TypeName = Annotated[str, Field(pattern=r"^[A-Z][A-Za-z0-9]*\Z")]
Hash = Annotated[str, Field(pattern=r"^[0-9a-f]{64}\Z")]
FrameId = Annotated[str, Field(pattern=r"^frame:[0-9a-f]{64}\Z")]
Id = Annotated[str, Field(min_length=1)]
Lang = Annotated[str, Field(pattern=r"^[a-z]{2,3}(?:-[A-Za-z0-9]{2,8})*\Z")]
Role = Literal["body", "reminder", "token_header", "layout", "name", "label"]
LeafType = Literal[
    "Nat",
    "Ordinal",
    "Player",
    "ZoneSet",
    "CardKind",
    "Concept",
    "CardName",
    "Phase",
    "Stat",
    "TokenStatus",
    "QuantitySpec",
    "QuantityExpr",
    "LiteralLayout",
]
PortType = Literal[
    "ObjectSet", "PlayerRef", "ReceiptId", "CapturedValue", "QuantityExpr"
]


def hash_payload(value: object) -> str:
    """The four-layer recipes use bare full SHA-256 rather than a transport prefix."""
    from pydantic import JsonValue, TypeAdapter  # ruff: ignore[import-outside-top-level] -- accept only JSON before hashing

    return digest(
        canonical(TypeAdapter(JsonValue).validate_python(value, strict=True))
    )[7:]


class Span(RecordData):
    start: UInt
    end: UInt

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        if self.start >= self.end:
            raise ValueError("Four-layer span must be nonempty")
        return self


def disjoint(spans: tuple[Span, ...]) -> None:
    """Validation preserves order; sorting here would repair invalid source evidence."""
    if any(left.end > right.start for left, right in pairwise(spans)):
        raise ValueError("Four-layer spans must be ordered and disjoint")


class GlossaryReference(RecordData):
    kind: Literal["glossary"]
    key: Id


class VocabularyReference(RecordData):
    kind: Literal["vocabulary"]
    key: tuple[Code, Code]


class CardNameReference(RecordData):
    kind: Literal["card_name"]
    term_id: Id


TermReference = Annotated[
    GlossaryReference | VocabularyReference, Field(discriminator="kind")
]
Reference = Annotated[
    GlossaryReference | VocabularyReference | CardNameReference,
    Field(discriminator="kind"),
]
Scalar = UInt | bool | Code | Reference


class Domain(RecordData):
    values: tuple[Scalar, ...]
    min: UInt | None
    max: UInt | None

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        encoded = tuple(
            canonical(v.model_dump(mode="json"))
            if isinstance(v, RecordData)
            else canonical(v)
            for v in self.values
        )
        if encoded != tuple(sorted(set(encoded))):
            raise ValueError("Leaf domain values must be sorted and unique")
        return self


class LeafSlot(RecordData):
    name: Code
    type: LeafType
    role: Code
    domain: Domain
    required: bool
    occurrences: tuple[Span, ...]

    @model_validator(mode="after")
    def _typed(self) -> Self:
        disjoint(self.occurrences)
        if self.type in {"Nat", "Ordinal"}:
            if self.domain.values or self.domain.min is None or self.domain.max is None:
                raise ValueError("Numeric leaf needs bounded domain")
            if self.domain.min > self.domain.max:
                raise ValueError("Numeric leaf domain is inverted")
            if self.type == "Ordinal" and self.domain.min < 1:
                raise ValueError("Ordinal leaf domain must be positive")
        elif (
            not self.domain.values
            or self.domain.min is not None
            or self.domain.max is not None
        ):
            raise ValueError("Non-numeric leaf needs a named domain")
        _domain_shape(self)
        return self


def _domain_shape(slot: LeafSlot) -> None:
    if (
        slot.type in {"Concept", "CardName", "CardKind"}
        and len(slot.domain.values) == 1
        and isinstance(slot.domain.values[0], str)
    ):
        return
    if slot.type == "CardKind":
        if any(
            not isinstance(v, VocabularyReference) or v.key[0] != "type"
            for v in slot.domain.values
        ):
            raise ValueError("CardKind domain must reference type vocabulary")
    elif slot.type == "CardName":
        if any(not isinstance(v, CardNameReference) for v in slot.domain.values):
            raise ValueError("CardName domain must reference a name concept")
    elif slot.type == "Concept":
        if any(
            not isinstance(v, (GlossaryReference, VocabularyReference))
            for v in slot.domain.values
        ):
            raise ValueError("Concept domain must reference a term or vocabulary")
    elif slot.type not in {"Nat", "Ordinal"} and any(
        not isinstance(v, str) for v in slot.domain.values
    ):
        raise ValueError("This leaf type requires registered domain codes")


class LeafSchema(RecordData):
    format: Literal[2]
    slots: tuple[LeafSlot, ...]

    @field_validator("format", mode="before")
    @classmethod
    def _integer(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Leaf schema format must be integer")
        return value

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        if len({slot.name for slot in self.slots}) != len(self.slots):
            raise ValueError("Leaf names must be unique")
        explicit = tuple(s for s in self.slots if s.occurrences)
        if explicit != tuple(sorted(explicit, key=lambda s: s.occurrences[0].start)):
            raise ValueError("Leaf slots must follow canonical source order")
        disjoint(
            tuple(
                sorted(
                    (r for s in self.slots for r in s.occurrences),
                    key=lambda r: r.start,
                )
            )
        )
        return self


class Constant(RecordData):
    kind: Literal["constant"]
    value: UInt


class Bound(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    kind: Literal["bound"]
    import_: Code = Field(alias="import")


class Expression(RecordData):
    kind: Literal["expression"]
    expression_id: Code
    leaves: Annotated[tuple[Code, ...], Field(min_length=1)]


QuantityExpr = Annotated[Constant | Bound | Expression, Field(discriminator="kind")]


class QuantitySpec(RecordData):
    mode: Literal["exact", "up_to", "at_least", "all", "any"]
    expr: QuantityExpr | None

    @model_validator(mode="after")
    def _typed(self) -> Self:
        if (self.mode in {"all", "any"}) != (self.expr is None):
            raise ValueError("Quantity mode and expression disagree")
        return self


class Scope(RecordData):
    id: Code
    parent: Code | None
    kind: Literal["ability", "branch", "sequence"]


class Port(RecordData):
    name: Code
    type: PortType
    source_role: Code
    scope: Code


class Projection(RecordData):
    projection_kind: Literal[
        "ability_body", "ability_flag", "card_field", "none", "pending"
    ]
    discriminator: Code | None
    scopes: tuple[Scope, ...]
    imports: tuple[Port, ...]
    exports: tuple[Port, ...]

    @model_validator(mode="after")
    def _linked(self) -> Self:
        if (self.projection_kind == "pending") != (self.discriminator is None):
            raise ValueError("Projection discriminator disagrees with pending state")
        if self.projection_kind == "none" and (self.imports or self.exports):
            raise ValueError("None projection cannot have ports")
        ids = tuple(s.id for s in self.scopes)
        if ids != tuple(sorted(set(ids))):
            raise ValueError("Projection scopes must be sorted and unique")
        parents = {s.id: s.parent for s in self.scopes}
        for scope in self.scopes:
            visited: set[str] = set()
            current: str | None = scope.id
            while current is not None:
                if current in visited or current not in parents:
                    raise ValueError("Projection scope is dangling or cyclic")
                visited.add(current)
                current = parents[current]
        for ports in (self.imports, self.exports):
            names = tuple(p.name for p in ports)
            if names != tuple(sorted(set(names))) or any(
                p.scope not in parents for p in ports
            ):
                raise ValueError("Projection port is duplicate unordered or dangling")
        return self

    def interface_key(self, frame_id: str) -> str:
        """Keep port/scope freshness separate from source semantic identity."""
        return hash_payload(
            {
                "recipe": "frame-interface-v1",
                "frame_id": frame_id,
                **self.model_dump(
                    mode="json", exclude={"projection_kind", "discriminator"}
                ),
            }
        )


class FaceRevisionOwner(RecordData):
    kind: Literal["face_revision"]
    revision_id: Id


class PrintingFaceOwner(RecordData):
    kind: Literal["printing_face"]
    printing_id: Id
    face_id: Id


class QaOwner(RecordData):
    kind: Literal["qa_version"]
    qa_version_id: Id


class CrOwner(RecordData):
    kind: Literal["cr_clause"]
    cr_clause_id: Id


class VocabularyOwner(RecordData):
    kind: Literal["vocabulary"]
    vocabulary_kind: Code
    vocabulary_code: Code


class ProductOwner(RecordData):
    kind: Literal["product"]
    product_id: Id


class FamilyOwner(RecordData):
    kind: Literal["product_family"]
    product_family_id: Id


class KeywordOwner(RecordData):
    kind: Literal["keyword"]
    keyword_id: Id


Owner = Annotated[
    FaceRevisionOwner
    | PrintingFaceOwner
    | QaOwner
    | CrOwner
    | VocabularyOwner
    | ProductOwner
    | FamilyOwner
    | KeywordOwner,
    Field(discriminator="kind"),
]


class OwnerField(RecordData):
    owner: Owner
    field: Code
    ordinal: UInt | None

    @model_validator(mode="after")
    def _field(self) -> Self:
        fields = {
            "face_revision": {"name", "effect", "section"},
            "printing_face": {"name", "effect", "flavor", "section"},
            "qa_version": {"question", "answer"},
            "cr_clause": {"effect"},
            "vocabulary": {"label"},
            "product": {"label"},
            "product_family": {"label"},
            "keyword": {"label", "effect", "action_label"},
        }
        if self.field not in fields[self.owner.kind]:
            raise ValueError("Source field is not legal for this owner kind")
        if (self.field in {"section", "action_label"}) != (self.ordinal is not None):
            raise ValueError("Source field and ordinal disagree")
        return self


class OccurrenceKey(OwnerField):
    source_hash: Hash
    line_ordinal: UInt
    role: Role
    segments: Annotated[tuple[Span, ...], Field(min_length=1)]


class SemanticVariant(RecordData):
    state: Literal["resolved", "pending"]
    key: Code | None
    scope: OccurrenceKey | None

    @model_validator(mode="after")
    def _typed(self) -> Self:
        if self.state == "resolved":
            if self.key is None or self.scope is not None:
                raise ValueError("Resolved variant must have key and no scope")
        elif self.key is not None or self.scope is None:
            raise ValueError("Pending variant must have exact occurrence scope")
        return self


class Source(RecordData):
    source_lang: Literal["ja"]
    canonical_hash: Hash
    normalizer_version: Code


class Frame(RecordData):
    id: FrameId
    source: Source
    role: Role
    semantic_variant: SemanticVariant
    leaf_schema: LeafSchema
    projection: Projection
    content_hash: Hash

    @model_validator(mode="after")
    def _identity(self) -> Self:
        verify_frame(self)
        if self.id != "frame:" + self.content_hash:
            raise ValueError("Frame ID differs from full content hash")
        if (
            self.projection.projection_kind == "pending"
            and self.semantic_variant.state != "pending"
        ):
            raise ValueError("Pending projection requires pending semantics")
        return self

    def payload(self, canonical_source: str) -> dict[str, object]:
        """Exclude optional rendering choices and interface description from frame identity."""
        return {
            "recipe": "frame-v1",
            "source_lang": self.source.source_lang,
            "canonical_source": canonical_source,
            "normalizer_version": self.source.normalizer_version,
            "role": self.role,
            "leaf_schema": self.leaf_schema.model_dump(mode="json"),
            "semantic_variant": self.semantic_variant.model_dump(mode="json"),
            "projection": self.projection.model_dump(
                mode="json", include={"projection_kind", "discriminator"}
            ),
        }

    def verify(self, canonical_source: str) -> None:
        """Validate reconstructed source before trusting authored content-addressed identity."""
        verify_frame(self)
        if digest(canonical_source.encode())[7:] != self.source.canonical_hash:
            raise ValueError("Frame canonical source hash mismatch")
        if hash_payload(self.payload(canonical_source)) != self.content_hash:
            raise ValueError("Frame semantic content hash mismatch")
        if any(
            span.end > len(canonical_source)
            for slot in self.leaf_schema.slots
            for span in slot.occurrences
        ):
            raise ValueError("Frame canonical leaf span is out of bounds")


class SlotRef(RecordData):
    slot: Code


class LiteralNode(RecordData):
    kind: Literal["Literal"]
    text: str


class LeafRef(RecordData):
    kind: Literal["LeafRef"]
    slot: Code


class FormNode(RecordData):
    kind: Literal["Form"]
    form_id: Code
    args: dict[Code, SlotRef]


class CardArgs(RecordData):
    model_config = ConfigDict(serialize_by_alias=True)

    kind: SlotRef
    quantity: SlotRef
    owner: SlotRef | None = Field(default=None, exclude_if=lambda v: v is None)
    zone: SlotRef | None = Field(default=None, exclude_if=lambda v: v is None)
    traits: tuple[SlotRef, ...] = ()
    class_: SlotRef | None = Field(
        default=None, alias="class", exclude_if=lambda v: v is None
    )
    token: SlotRef | None = Field(default=None, exclude_if=lambda v: v is None)

    @model_validator(mode="after")
    def _nullable(self) -> Self:
        if any(
            k in self.model_fields_set and getattr(self, k) is None
            for k in ("owner", "zone", "class_", "token")
        ):
            raise ValueError("Optional CardNP arguments cannot be null")
        return self


class CardNP(RecordData):
    kind: Literal["NP"]
    constructor: Literal["CardNP"]
    args: CardArgs


class UnionArgs(RecordData):
    branches: Annotated[tuple[CardNP, ...], Field(min_length=2)]
    quantity: SlotRef | None = Field(default=None, exclude_if=lambda v: v is None)

    @model_validator(mode="after")
    def _nullable(self) -> Self:
        if "quantity" in self.model_fields_set and self.quantity is None:
            raise ValueError("Optional UnionNP quantity cannot be null")
        return self


class UnionNP(RecordData):
    kind: Literal["NP"]
    constructor: Literal["UnionNP"]
    args: UnionArgs


class CountArgs(RecordData):
    expr: SlotRef


class CountExpr(RecordData):
    kind: Literal["NP"]
    constructor: Literal["CountExpr"]
    args: CountArgs


NP = Annotated[CardNP | UnionNP | CountExpr, Field(discriminator="constructor")]
Node = Annotated[LiteralNode | LeafRef | FormNode | NP, Field(discriminator="kind")]


class Target(RecordData):
    format: Literal[1]
    nodes: Annotated[tuple[Node, ...], Field(min_length=1)]

    @field_validator("format", mode="before")
    @classmethod
    def _integer(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Target format must be integer")
        return value

    def verify(
        self,
        schema: LeafSchema,
        lang: str,
        forms: Mapping[tuple[str, str], FormDefinition],
    ) -> None:
        """Required leaves cannot disappear inside literal wording or an opaque NP."""
        slots = {slot.name: slot for slot in schema.slots}
        used = set().union(
            *(_node_slots(node, slots, lang, forms) for node in self.nodes)
        )
        if any(slot.required and slot.name not in used for slot in schema.slots):
            raise ValueError("Target must use every required leaf")


def _leaf(
    slots: Mapping[str, LeafSlot], name: str, expected: LeafType | None = None
) -> LeafSlot:
    if name not in slots:
        raise ValueError("Target references an unknown leaf")
    slot = slots[name]
    if expected is not None and slot.type != expected:
        raise ValueError("Target leaf type disagrees with its argument")
    return slot


def _card_slots(node: CardNP, slots: Mapping[str, LeafSlot]) -> set[str]:
    args = node.args
    used = {
        _leaf(slots, args.kind.slot, "CardKind").name,
        _leaf(slots, args.quantity.slot, "QuantitySpec").name,
    }
    references: tuple[tuple[SlotRef | None, LeafType], ...] = (
        (args.owner, "Player"),
        (args.zone, "ZoneSet"),
        (args.class_, "Concept"),
        (args.token, "TokenStatus"),
    )
    used.update(
        _leaf(slots, ref.slot, expected).name
        for ref, expected in references
        if ref is not None
    )
    traits = tuple(ref.slot for ref in args.traits)
    if len(traits) != len(set(traits)):
        raise ValueError("CardNP trait leaves must be unique")
    for name in traits:
        if any(
            not isinstance(value, GlossaryReference)
            for value in _leaf(slots, name, "Concept").domain.values
        ):
            raise ValueError("CardNP traits require glossary concepts")
        used.add(name)
    if args.class_ is not None:
        _class_domain(slots[args.class_.slot])
    return used


def _class_domain(slot: LeafSlot) -> None:
    if any(
        not isinstance(value, VocabularyReference) or value.key[0] != "class"
        for value in slot.domain.values
    ):
        raise ValueError("CardNP class requires class vocabulary")


def _form_slots(
    node: FormNode,
    slots: Mapping[str, LeafSlot],
    lang: str,
    forms: Mapping[tuple[str, str], FormDefinition],
) -> set[str]:
    form = forms.get((node.form_id, lang))
    if form is None:
        raise ValueError("Target requires a form in its own language")
    return form.verify_arguments(node.args, slots)


def _node_slots(
    node: Node,
    slots: Mapping[str, LeafSlot],
    lang: str,
    forms: Mapping[tuple[str, str], FormDefinition],
) -> set[str]:
    if isinstance(node, LeafRef):
        return {_leaf(slots, node.slot).name}
    if isinstance(node, FormNode):
        return _form_slots(node, slots, lang, forms)
    if isinstance(node, CardNP):
        return _card_slots(node, slots)
    if isinstance(node, UnionNP):
        used = set().union(
            *(_card_slots(branch, slots) for branch in node.args.branches)
        )
        if node.args.quantity is not None:
            used.add(_leaf(slots, node.args.quantity.slot, "QuantitySpec").name)
        return used
    if isinstance(node, CountExpr):
        return {_leaf(slots, node.args.expr.slot, "QuantityExpr").name}
    return set()


class Signature(RecordData):
    name: Code
    type: LeafType
    role: Code


class LabelPart(RecordData):
    kind: Literal["Label"]
    arg: Code


class QuantityValuePart(RecordData):
    kind: Literal["QuantityValue"]
    arg: Code


FormPart = Annotated[
    LiteralNode | LabelPart | QuantityValuePart, Field(discriminator="kind")
]


class FormDefinition(RecordData):
    id: Code
    lang: Lang
    signature: Annotated[tuple[Signature, ...], Field(min_length=1)]
    rule: Code
    cases: Annotated[
        dict[Code, Annotated[tuple[FormPart, ...], Field(min_length=1)]],
        Field(min_length=1),
    ]

    @model_validator(mode="after")
    def _linked(self) -> Self:
        names = tuple(arg.name for arg in self.signature)
        if len(names) != len(set(names)):
            raise ValueError("Form signature names must be unique")
        if any(
            part.arg not in names
            for parts in self.cases.values()
            for part in parts
            if isinstance(part, (LabelPart, QuantityValuePart))
        ):
            raise ValueError("Form part requires a signature argument")
        signature = {arg.name: arg.type for arg in self.signature}
        if any(
            self.rule != "quantity.constant_value.v1"
            or signature[part.arg] != "QuantitySpec"
            for parts in self.cases.values()
            for part in parts
            if isinstance(part, QuantityValuePart)
        ):
            raise ValueError("QuantityValue requires its constant quantity rule")
        return self

    def verify_arguments(
        self, args: Mapping[str, SlotRef], slots: Mapping[str, LeafSlot]
    ) -> set[str]:
        """Implicit NP forms obey the same typed role signature as explicit Form nodes."""
        signature = {arg.name: arg for arg in self.signature}
        if set(args) != set(signature):
            raise ValueError("Form argument names disagree with signature")
        used = set()
        for name, ref in args.items():
            slot = _leaf(slots, ref.slot, signature[name].type)
            if slot.role != signature[name].role:
                raise ValueError("Form leaf role disagrees with signature")
            used.add(slot.name)
        return used

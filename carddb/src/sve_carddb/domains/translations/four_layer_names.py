"""Source-bound name relations, name fragments and two independent trait operands."""

import re
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    ContextExpression,
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSchema,
    LeafSlot,
    ObjectGeneration,
    ObjectSetRef,
    OccurrenceKey,
    Port,
    PortEvidence,
    PredicateExpr,
    PrintingFaceOwner,
    Projection,
    ReceiptRef,
    Scope,
    Span,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.contracts.source_binding import LeafOccurrence, ReferenceDomain
from sve_carddb.domains.translations.four_layer_ability import (
    ABILITY_HEAD,
    OPTION_LABEL,
)
from sve_carddb.domains.translations.four_layer_context import (
    DEFAULT_REGISTRY,
    Descriptor,
    ImportSignature,
    Parameter,
)
from sve_carddb.domains.translations.four_layer_derivation import SourceFamily
from sve_carddb.domains.translations.four_layer_history import name_filter_spans
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import LeafType
    from sve_carddb.contracts.source_binding import TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


def _relation(
    name: str,
    *,
    receipt: bool = False,
    distinct: bool = False,
    holder: bool = False,
    event: bool = False,
) -> Descriptor:
    return Descriptor(
        "name." + name + ".v1",
        "predicate.selection.distinct_name.v1"
        if distinct
        else "predicate.name.relative.v1",
        "selection_name_constraint" if distinct else "name_relation",
        "Bool",
        "truth",
        (),
        (
            ImportSignature(
                "receipt" if receipt else "objects",
                "ReceiptId" if receipt else "ObjectSet",
                "receipt"
                if receipt
                else "lki"
                if event
                else "live"
                if holder
                else "selection",
                cardinality=None if receipt else "many" if distinct else "one",
                receipt_result="objects" if receipt else None,
                origin=None
                if receipt
                else "event_subject"
                if event
                else "holder"
                if holder
                else "selection",
            ),
        ),
        "A3",
        ("V.receipt-field", "V.name", "X.predicate")
        if receipt
        else ("V.name", "S.constraint" if distinct else "S.filter", "X.predicate"),
    )


_FRAGMENT = Parameter("fragment", "CardName", "name_fragment", "card_name.any.v1")
DESCRIPTORS = (
    _relation("relative.holder.not_equal", holder=True),
    _relation("relative.objects.equal"),
    _relation("relative.event_subject.equal", event=True),
    _relation("relative.receipt.equal", receipt=True),
    _relation("selection.distinct", distinct=True),
    Descriptor(
        "name.contains.fragment.v1",
        "predicate.name.contains.v1",
        "name_contains_filter",
        "Bool",
        "truth",
        (_FRAGMENT,),
        (),
        "A5",
        ("V.name", "X.predicate", "S.filter"),
    ),
    *(
        Descriptor(
            f"name.contains.any{count}.v1",
            "predicate.name.contains.v1",
            "name_contains_filter",
            "Bool",
            "truth",
            tuple(replace(_FRAGMENT, name=f"fragment_{i}") for i in range(count)),
            (),
            "A5",
            ("V.name", "X.predicate", "S.filter"),
        )
        for count in (2, 4, 10)
    ),
    Descriptor(
        "traits.all2.v1",
        "predicate.traits.all2.v1",
        "trait_conjunction",
        "Bool",
        "truth",
        (
            Parameter("left", "Concept", "filter_trait_left", "concept.trait.v1"),
            Parameter("right", "Concept", "filter_trait_right", "concept.trait.v1"),
        ),
        (),
        "A9",
        ("V.traits", "X.predicate", "S.filter"),
    ),
)
_REGISTRY = DEFAULT_REGISTRY.extend(DESCRIPTORS)
_CONTAINS = re.compile(r"カード名に(?P<names>『[^『』]+』(?:か『[^『』]+』)*)を含む")
_RELATIVE = re.compile(
    r"これと同名(?:を除く|でない)|それと同名|(?:公開した|消滅させた)フォロワーと同名"
)
_DISTINCT = re.compile(r"カード名が異なる(?:ように)?")
_COUNT = re.compile(r"カード名の種類数")


def _origins(part: SourcePart, span: Span) -> tuple[Span, ...]:
    return merged(
        tuple(s for u in part.units[span.start : span.end] for s in u.origins)
    )


def _overlap(left: tuple[Span, ...], right: tuple[Span, ...]) -> bool:
    return any(a.start < b.end and b.start < a.end for a in left for b in right)


@dataclass
class _Builder:
    raw: str
    part: SourcePart
    original: SourcePart
    found: Recognized
    rows: list[str]
    slots: list[LeafSlot]
    values: dict[str, TypedValue]
    occurrences: list[LeafOccurrence]
    scopes: dict[str, Scope]
    ports: dict[str, Port]
    source: OccurrenceKey
    issues: set[str]

    def positions(self, span: Span) -> tuple[Span, ...]:
        raw = _origins(self.original, span)
        return merged(
            tuple(
                Span(start=i, end=i + 1)
                for i, unit in enumerate(self.part.units)
                if _overlap(unit.origins, raw)
            )
        )

    def add(
        self,
        type_: LeafType,
        role: str,
        domain: str,
        value: TypedValue,
        span: Span,
        *,
        operand: bool = True,
    ) -> str | None:
        expression = (
            value if isinstance(value, (ContextExpression, PredicateExpr)) else None
        )
        positions = self.positions(span)
        for i, slot in enumerate(self.slots):
            if slot.occurrences == positions:
                if self.values[slot.name] != value:
                    return None
                # Classifier forms require a recognized kind to keep its counted role.
                if operand:
                    self.slots[i] = slot.model_copy(update={"role": role})
                return slot.name
        occupied = tuple(s for slot in self.slots for s in slot.occurrences)
        positions = merged(
            tuple(
                Span(start=i, end=i + 1)
                for pos in positions
                for i in range(pos.start, pos.end)
                if not _overlap((Span(start=i, end=i + 1),), occupied)
            )
        )
        if not positions:
            return None
        name = f"name_{len(self.slots)}"
        self.slots.append(
            LeafSlot(
                name=name,
                type=type_,
                role=role,
                domain=Domain(values=(domain,), min=None, max=None),
                required=True,
                occurrences=positions,
                expression=expression,
            )
        )
        self.values[name] = value
        self.occurrences.append(
            LeafOccurrence(
                slot=name,
                ordinal=0,
                raw_spans=merged(
                    tuple(s for p in positions for s in _origins(self.part, p))
                ),
                canonical_spans=positions,
                source_unit=None,
                source_presence="explicit",
                resolution_rule=None,
            )
        )
        self.rows.append(
            "N2-" + _REGISTRY.definition(expression).source_family
            if expression
            else "N2-name.operand"
        )
        return name

    def scope(self, position: int) -> str:
        text = self.original.canonical_source[:position]
        heads = tuple(
            re.finditer(
                ABILITY_HEAD,
                text,
            )
        )
        start = heads[-1].start() if heads else 0
        ability = f"names_{self.part.line_ordinal}_{start}"
        self.scopes[ability] = Scope(id=ability, parent=None, kind="ability")
        modes = tuple(re.finditer(r"【[0-9N]+】", text[start:]))
        if not modes:
            return ability
        branch = ability + f"_mode_{start + modes[-1].start()}"
        self.scopes[branch] = Scope(id=branch, parent=ability, kind="branch")
        return branch

    def expression(
        self,
        identifier: str,
        span: Span,
        leaves: tuple[str, ...] = (),
        imports: dict[str, str] | None = None,
    ) -> None:
        rule = _REGISTRY.descriptors[identifier]
        expr = (
            ContextExpression(
                kind="context_expression",
                expression_id=identifier,
                leaves=leaves,
                imports=imports or {},
                scope=self.scope(span.start),
            )
            if rule.result_type == "Nat"
            else PredicateExpr(
                kind="predicate",
                expression_id=identifier,
                leaves=leaves,
                imports=imports or {},
                scope=self.scope(span.start),
            )
        )
        self.add(
            "QuantityExpr" if rule.result_type == "Nat" else "PredicateExpr",
            rule.role,
            rule.domain,
            expr,
            span,
        )

    def port(
        self,
        span: Span,
        origin: Literal["holder", "selection", "event_subject"],
        *,
        receipt: bool = False,
    ) -> str:
        scope = self.scope(span.start)
        key = hash_payload(
            {
                "source": self.source.model_dump(mode="json"),
                "span": [
                    s.model_dump(mode="json") for s in _origins(self.original, span)
                ],
                "origin": origin,
                "receipt": receipt,
            }
        )
        name = "names_port_" + key[:16]
        object_key = (
            hash_payload(
                {
                    "owner": self.source.owner.model_dump(mode="json"),
                    "field": self.source.field,
                    "scope": scope,
                }
            )
            if origin == "holder"
            else key
        )
        value = (
            ReceiptRef(kind="ReceiptId", receipt_id="source:" + key, result="objects")
            if receipt
            else ObjectSetRef(
                kind="ObjectSet",
                objects=(
                    ObjectGeneration(object_id="source:" + object_key, generation=0),
                ),
                origin=origin,
            )
        )
        self.ports[name] = Port(
            name=name,
            type="ReceiptId" if receipt else "ObjectSet",
            source_role="name_source",
            scope=scope,
            evidence=PortEvidence(
                source=self.source,
                capture="receipt"
                if receipt
                else "lki"
                if origin == "event_subject"
                else "live"
                if origin == "holder"
                else "selection",
                value=value,
            ),
        )
        return name


def _reference(
    builder: _Builder,
    classifier: Classifier,
    category: str,
    role: str,
    span: Span,
) -> str | None:
    spelling = "".join(
        builder.raw[s.start : s.end] for s in _origins(builder.original, span)
    )
    terms = tuple(
        t
        for t in classifier.terms.values()
        if t.category == category and t.source_ja == spelling
    )
    if len(terms) != 1:
        builder.issues.add("missing_or_ambiguous_name_operand")
        return None
    value = (
        CardNameReference(kind="card_name", term_id=terms[0].id)
        if category == "card_name"
        else GlossaryReference(kind="glossary", key=terms[0].id)
    )
    return builder.add(
        "CardName" if category == "card_name" else "Concept",
        role,
        "card_name.any.v1" if category == "card_name" else "concept.trait.v1",
        value,
        span,
    )


def _fragments(builder: _Builder, classifier: Classifier) -> None:
    text = builder.original.canonical_source
    for match in _CONTAINS.finditer(text):
        names = tuple(re.finditer(r"『([^『』]+)』", match["names"]))
        if len(names) not in {1, 2, 4, 10}:
            continue
        leaves = []
        for name in names:
            start = match.start("names") + name.start(1)
            leaf = _reference(
                builder,
                classifier,
                "card_name",
                "name_fragment",
                Span(start=start, end=start + len(name[1])),
            )
            if leaf is not None:
                leaves.append(leaf)
        if len(leaves) == len(names):
            builder.expression(
                "name.contains.fragment.v1"
                if len(names) == 1
                else f"name.contains.any{len(names)}.v1",
                Span(start=match.start(), end=match.start("names")),
                tuple(leaves),
            )


def _traits(builder: _Builder, classifier: Classifier) -> None:
    text = builder.original.canonical_source
    terms = {t.source_ja for t in classifier.terms.values() if t.category == "trait"}
    if not terms:
        return
    alternatives = "|".join(
        re.escape(t) for t in sorted(terms, key=lambda t: (-len(t), t))
    )
    pattern = re.compile(
        r"(?<!・)(?P<left>"
        + alternatives
        + r")・(?P<right>"
        + alternatives
        + r")・(?P<kind>フォロワー|スペル|アミュレット|カード)"
    )
    for match in pattern.finditer(text):
        # Token identity and either-selector syntax have different source semantics.
        if text[match.end() :].startswith("・"):
            continue
        leaves = tuple(
            _reference(
                builder,
                classifier,
                "trait",
                role,
                Span(start=match.start(group), end=match.end(group)),
            )
            for group, role in (
                ("left", "filter_trait_left"),
                ("right", "filter_trait_right"),
            )
        )
        if all(leaf is not None for leaf in leaves):
            kind_span = Span(start=match.start("kind"), end=match.end("kind"))
            if match["kind"] == "カード":
                term = classifier.terms.get("term:object.card")
                if term is None or (term.category, term.source_ja) != (
                    "rule_term",
                    "カード",
                ):
                    builder.issues.add("missing_name_filter_kind")
                    continue
                builder.add(
                    "Concept",
                    "rule_term",
                    "concept.rule_term.v1",
                    GlossaryReference(kind="glossary", key="term:object.card"),
                    kind_span,
                    operand=False,
                )
            else:
                code = {
                    "フォロワー": "follower",
                    "スペル": "spell",
                    "アミュレット": "amulet",
                }[match["kind"]]
                reference = VocabularyReference(kind="vocabulary", key=("type", code))
                domain = classifier.domains.get("card_kind.any.v1")
                if not isinstance(domain, ReferenceDomain) or not domain.contains(
                    reference
                ):
                    builder.issues.add("missing_name_filter_kind")
                    continue
                builder.add(
                    "CardKind",
                    "filter_kind",
                    "card_kind.any.v1",
                    reference,
                    kind_span,
                    operand=False,
                )
            builder.expression(
                "traits.all2.v1",
                Span(start=match.end("left"), end=match.start("right")),
                tuple(leaf for leaf in leaves if leaf is not None),
            )


def _names(builder: _Builder) -> None:
    text = builder.original.canonical_source
    for match in _RELATIVE.finditer(text):
        if any(
            a <= match.start() < match.end() <= b for a, b in name_filter_spans(text)
        ):
            continue
        span = Span(start=match.start(), end=match.end())
        holder = match[0].startswith("これ")
        receipt = match[0].startswith(("公開した", "消滅させた"))
        if not holder:
            # A receipt must have exactly one matching producer in the same ability/branch.
            prefix = text[: match.start()]
            producer = (
                r"フォロワーN枚を公開[:：]"
                if match[0].startswith("公開した")
                else r"フォロワーN枚を消滅[:：]"
                if receipt
                else r"(?:N(?:体|枚)を選ぶ。|カードが場を離れたとき、)"
            )
            candidates = tuple(re.finditer(producer, prefix))
            if len(candidates) != 1 or re.search(
                OPTION_LABEL + "|" + ABILITY_HEAD,
                prefix[candidates[0].end() :],
            ):
                builder.issues.add("name_source_missing_or_ambiguous")
                continue
            start = candidates[0].start()
            producer_span = Span(start=start, end=candidates[0].end())
            event = "場を離れたとき" in candidates[0][0]
            port = builder.port(
                producer_span,
                "event_subject" if event else "selection",
                receipt=receipt,
            )
        else:
            event = False
            port = builder.port(span, "holder")
        builder.expression(
            "name.relative.holder.not_equal.v1"
            if holder
            else "name.relative.receipt.equal.v1"
            if receipt
            else "name.relative.event_subject.equal.v1"
            if event
            else "name.relative.objects.equal.v1",
            span,
            imports={"receipt" if receipt else "objects": port},
        )
    for match in _DISTINCT.finditer(text):
        span = Span(start=match.start(), end=match.end())
        port = builder.port(span, "selection")
        builder.expression(
            "name.selection.distinct.v1", span, imports={"objects": port}
        )
    for match in _COUNT.finditer(text):
        span = Span(start=match.start(), end=match.end())
        port = builder.port(span, "selection")
        builder.expression(
            "name.distinct_count.objects.v1", span, imports={"objects": port}
        )


def prepare_names(
    raw: str,
    prepared: DerivedField,
    classifier: Classifier,
    context: CardContext | None,
) -> DerivedField:
    """Keep all new descriptors behind the explicit family preparation callback."""
    del context
    derived = prepared.derived
    source = derived.field.source
    if source.field not in {"effect", "section"} or not isinstance(
        source.owner, (FaceRevisionOwner, PrintingFaceOwner)
    ):
        return prepared
    recognized = list(derived.recognized)
    rows = list(derived.registry_rows)
    interfaces = list(prepared.interfaces)
    for i, (part, original, found) in enumerate(
        zip(
            derived.field.parts, derived.original.parts, derived.recognized, strict=True
        )
    ):
        if part.source_span.role != "body":
            continue
        occurrence = OccurrenceKey(
            **source.model_dump(include={"owner", "field", "ordinal", "source_hash"}),
            line_ordinal=part.line_ordinal,
            role=part.source_span.role,
            segments=part.source_span.segments,
        )
        projection = interfaces[i] or (
            found.semantics.projection
            if found.semantics
            else Projection(
                projection_kind="pending",
                discriminator=None,
                scopes=(),
                imports=(),
                exports=(),
            )
        )
        builder = _Builder(
            raw,
            part,
            original,
            found,
            list(rows[i]),
            list(found.schema.slots),
            dict(found.values),
            list(found.occurrences),
            {s.id: s for s in projection.scopes},
            {p.name: p for p in projection.imports},
            occurrence,
            set(found.issues),
        )
        _fragments(builder, classifier)
        _traits(builder, classifier)
        _names(builder)
        if builder.slots == list(found.schema.slots) and builder.issues == set(
            found.issues
        ):
            continue
        ordered = sorted(
            zip(builder.slots, builder.rows, strict=True),
            key=lambda pair: (
                pair[0].occurrences[0].start
                if pair[0].occurrences
                else len(part.canonical_source)
            ),
        )
        builder.slots = [s for s, _ in ordered]
        builder.rows = [r for _, r in ordered]
        builder.occurrences.sort(
            key=lambda o: (o.raw_spans[0].start if o.raw_spans else len(raw), o.slot)
        )
        recognized[i] = replace(
            found,
            schema=LeafSchema(format=2, slots=tuple(builder.slots)),
            values=builder.values,
            occurrences=tuple(builder.occurrences),
            issues=tuple(sorted(builder.issues)),
        )
        rows[i] = tuple(builder.rows)
        interfaces[i] = projection.model_copy(
            update={
                "scopes": tuple(sorted(builder.scopes.values(), key=lambda s: s.id)),
                "imports": tuple(sorted(builder.ports.values(), key=lambda p: p.name)),
            }
        )
    return replace(
        prepared,
        derived=replace(
            derived, recognized=tuple(recognized), registry_rows=tuple(rows)
        ),
        interfaces=tuple(interfaces),
    )


FAMILY = SourceFamily("names", DESCRIPTORS, prepare_names)

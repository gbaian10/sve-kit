"""Source adapters for the finite holder, pool and event constructions in #749."""

from dataclasses import dataclass, field, replace
from typing import TYPE_CHECKING
from unicodedata import normalize

from sve_carddb.contracts.four_layer import (
    ContextExpression,
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSlot,
    PoolContext,
    Port,
    PortEvidence,
    PredicateExpr,
    PrintingFaceOwner,
    Projection,
    QuantitySpec,
    Scope,
    Span,
    hash_payload,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.domains.translations.four_layer_749_references import required_words
from sve_carddb.domains.translations.four_layer_context import DEFAULT_REGISTRY
from sve_carddb.domains.translations.four_layer_derivation import SourceFamily
from sve_carddb.domains.translations.four_layer_event_preparation import (
    occurrence,
    origins,
    update_source,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_n2_read_sources import regions
from sve_carddb.domains.translations.four_layer_n3_context import DESCRIPTORS
from sve_carddb.domains.translations.four_layer_n3_events import prepare_events
from sve_carddb.domains.translations.four_layer_n3_forms import plain_numbers
from sve_carddb.domains.translations.four_layer_n3_parameters import prepare_parameters
from sve_carddb.domains.translations.four_layer_n3_ports import (
    ability_interface,
    pool_interface,
    stack_interface,
)
from sve_carddb.domains.translations.four_layer_normalizer import SourceField

if TYPE_CHECKING:
    import re
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import CaptureTime, LeafType, PortValue
    from sve_carddb.contracts.source_binding import SourceDescriptor, TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


@dataclass
class Builder:
    raw: str
    source: SourceDescriptor
    original: SourcePart
    current: SourcePart
    found: Recognized
    classifier: Classifier
    additions: list[Leaf] = field(default_factory=list)
    ports: list[Port] = field(default_factory=list)
    exports: list[Port] = field(default_factory=list)

    @property
    def text(self) -> str:
        """Expose the normalized source before any typed substitutions."""
        return self.original.canonical_source

    def number(self, span: Span) -> int:
        """Recover the printed value after normalization replaced its digits."""
        spelling = "".join(
            self.raw[s.start : s.end] for s in origins(self.original, span)
        )
        return int(normalize("NFKC", spelling))

    def scope(self, position: int) -> str:
        """Keep sibling branches and other abilities from sharing providers."""
        return (
            "n3_"
            + min(
                (r for r in regions(self.text) if r.start <= position < r.end),
                key=lambda r: r.end - r.start,
            ).id
        )

    def identity(self, kind: str, span: Span) -> str:
        """Derive a symbol from exact provenance rather than a card number."""
        return (
            "source-"
            + kind
            + ":"
            + hash_payload(
                {
                    "source": self.source.model_dump(mode="json"),
                    "scope": self.scope(span.start),
                    "segments": [
                        s.model_dump(mode="json") for s in origins(self.original, span)
                    ],
                }
            )
        )

    def port(
        self,
        role: str,
        value: PortValue,
        capture: CaptureTime,
        span: Span,
        consumer: int,
        leaves: tuple[str, ...] = (),
    ) -> str:
        """Source coordinates, rather than matching values, identify a provider."""
        name = f"n3_port_{len(self.ports) + len(self.exports)}"
        self.ports.append(
            Port(
                name=name,
                type=value.kind,
                source_role=role,
                scope=self.scope(consumer),
                evidence=PortEvidence(
                    source=occurrence(self.source, self.original).model_copy(
                        update={"segments": origins(self.original, span)}
                    ),
                    capture=capture,
                    value=value,
                    source_leaves=leaves,
                ),
            )
        )
        return name

    def term(self, span: Span, category: str, key: str | None, role: str) -> str | None:
        """A required concept must have one adopted source spelling."""
        spelling = "".join(
            self.raw[s.start : s.end] for s in origins(self.original, span)
        )
        choices = [
            t
            for t in self.classifier.terms.values()
            if (key is None or t.id == key)
            and t.category == category
            and t.source_ja == spelling
        ]
        if len(choices) != 1:
            return None
        return self.scalar(
            "Concept",
            role,
            f"concept.{category}.v1",
            GlossaryReference(kind="glossary", key=choices[0].id),
            (span,),
        )

    def scalar(
        self,
        type_: LeafType,
        role: str,
        domain: str,
        value: TypedValue,
        positions: tuple[Span, ...],
        *,
        maximum: int = SAFE_INTEGER,
    ) -> str:
        """Keep separate printed operands independent even when their values agree."""
        name = f"n3_arg_{len(self.additions)}"
        item = (
            leaf(type_, role, domain, value, tuple((s,) for s in positions), "N3-749")
            if domain
            else Leaf(
                LeafSlot(
                    name=name,
                    type=type_,
                    role=role,
                    required=True,
                    domain=Domain(
                        values=(),
                        min=1
                        if type_ == "Ordinal"
                        or role
                        in {
                            "group_divisor",
                            "declaration_lower_bound",
                            "declaration_upper_bound",
                        }
                        else 0,
                        max=maximum,
                    ),
                    occurrences=positions,
                ),
                value,
                tuple((s,) for s in positions),
                "N3-749",
            )
        )
        self.additions.append(
            replace(
                item,
                slot=item.slot.model_copy(update={"name": name}),
                abstract=type_ not in {"Nat", "Ordinal", "QuantitySpec"},
            )
        )
        return name

    def numeric(
        self, match: re.Match[str], group: str, role: str, mode: str | None = None
    ) -> str:
        """Numeric roles are assigned only after a complete source guard matches."""
        span = Span(start=match.start(group), end=match.end(group))
        value = self.number(span)
        if mode is None:
            return self.scalar(
                "Nat",
                role,
                "",
                value,
                (span,),
                maximum=6
                if role in {"declaration_lower_bound", "declaration_upper_bound"}
                else SAFE_INTEGER,
            )
        return self.scalar(
            "QuantitySpec",
            role,
            f"quantity.{role}.constant.v1",
            QuantitySpec.model_validate(
                {"mode": mode, "expr": {"kind": "constant", "value": value}}
            ),
            (span,),
        )

    def expression(
        self,
        identifier: str,
        arguments: tuple[str, ...],
        positions: tuple[Span, ...],
        imports: dict[str, str],
        scope: str,
    ) -> str:
        """Store the sealed descriptor and its typed dependencies in source identity."""
        rule = next((d for d in DESCRIPTORS if d.id == identifier), None)
        if rule is None:
            rule = DEFAULT_REGISTRY.descriptors[identifier]
        expr = (
            ContextExpression(
                kind="context_expression",
                expression_id=identifier,
                leaves=arguments,
                imports=imports,
                scope=scope,
            )
            if rule.result_type == "Nat"
            else PredicateExpr(
                kind="predicate",
                expression_id=identifier,
                leaves=arguments,
                imports=imports,
                scope=scope,
            )
        )
        name = self.scalar(
            "QuantityExpr" if rule.result_type == "Nat" else "PredicateExpr",
            rule.role,
            rule.domain,
            expr,
            positions,
        )
        self.additions[-1] = replace(
            self.additions[-1],
            slot=self.additions[-1].slot.model_copy(update={"expression": expr}),
        )
        return name

    def finish(self) -> tuple[SourcePart, Recognized, tuple[str, ...], dict[str, str]]:
        """Rename the complete dependency graph while retaining raw evidence."""
        return update_source(
            self.raw,
            self.original,
            self.current,
            self.found,
            self.additions,
            self.classifier,
        )


def _remap_port(port: Port, names: Mapping[str, str]) -> Port:
    if port.evidence is None:
        return port
    value = port.evidence.value
    if isinstance(value, PoolContext):
        value = value.model_copy(
            update={"members": tuple((names[a], names[b]) for a, b in value.members)}
        )
    return port.model_copy(
        update={
            "evidence": port.evidence.model_copy(
                update={
                    "source_leaves": tuple(
                        names[n] for n in port.evidence.source_leaves
                    ),
                    "value": value,
                }
            )
        }
    )


def _projection(
    builder: Builder, previous: Projection | None, names: Mapping[str, str]
) -> Projection | None:
    if not builder.ports and not builder.exports:
        return previous
    scopes = {s.id: s for s in previous.scopes} if previous else {}
    for region in regions(builder.text):
        name = "n3_" + region.id
        scopes[name] = Scope(
            id=name,
            parent="n3_" + region.parent if region.parent else None,
            kind="ability" if region.kind == "ability" else "branch",
        )
    imports = tuple(
        _remap_port(p, names)
        for p in (*(previous.imports if previous else ()), *builder.ports)
    )
    exports = tuple(
        _remap_port(p, names)
        for p in (*(previous.exports if previous else ()), *builder.exports)
    )
    return Projection(
        projection_kind=previous.projection_kind if previous else "pending",
        discriminator=previous.discriminator if previous else None,
        scopes=tuple(sorted(scopes.values(), key=lambda scope: scope.id)),
        imports=tuple(sorted(imports, key=lambda port: port.name)),
        exports=tuple(sorted(exports, key=lambda port: port.name)),
    )


def _wire(builder: Builder, claim: str) -> str | None:
    if claim in {"P01", "P02", "P03"} and not pool_interface(builder, claim):
        return "n3_missing_pool_provider"
    if claim == "A01" and not ability_interface(builder):
        return "n3_missing_ability_provider"
    if claim == "H03":
        stack_interface(builder)
    return None


def prepare(
    raw: str,
    prepared: DerivedField,
    classifier: Classifier,
    context: CardContext | None,
) -> DerivedField:
    """Apply finite source adapters without promoting whole-line semantics or review."""
    del context
    derived = prepared.derived
    if derived.field.source.field not in {"effect", "section"} or not isinstance(
        derived.field.source.owner, (FaceRevisionOwner, PrintingFaceOwner)
    ):
        return prepared
    parts = list(derived.field.parts)
    founds = list(derived.recognized)
    rows = list(derived.registry_rows)
    interfaces = list(prepared.interfaces)
    for i, original in enumerate(derived.original.parts):
        builder = Builder(
            raw, derived.field.source, original, parts[i], founds[i], classifier
        )
        claim = (
            plain_numbers(builder, derived.original)
            or prepare_events(builder)
            or prepare_parameters(builder)
        )
        if claim is None or not builder.additions:
            continue
        required_words(builder, claim)
        issue = _wire(builder, claim)
        if issue is not None:
            founds[i] = replace(
                founds[i], issues=tuple(sorted({*founds[i].issues, issue}))
            )
            continue
        parts[i], founds[i], rows[i], names = builder.finish()
        interfaces[i] = _projection(builder, interfaces[i], names)
    return replace(
        prepared,
        derived=replace(
            derived,
            field=SourceField(derived.field.source, tuple(parts)),
            recognized=tuple(founds),
            registry_rows=tuple(rows),
        ),
        interfaces=tuple(interfaces),
    )


FAMILY = SourceFamily("N3-749", DESCRIPTORS, prepare)

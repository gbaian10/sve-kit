"""Derive typed leaves from pinned source grammar before matching authored frames."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING, override

from sve_carddb.contracts.four_layer import (
    CardNameReference,
    Domain,
    Frame,
    GlossaryReference,
    LeafSchema,
    LeafSlot,
    OccurrenceKey,
    Projection,
    SemanticVariant,
    Source,
    Span,
    VocabularyReference,
    hash_payload,
)
from sve_carddb.contracts.n0 import QUANTITY_ROLES, SAFE_INTEGER
from sve_carddb.contracts.source_binding import (
    LayoutDomain,
    LeafOccurrence,
    QuantityDomain,
    ReferenceDomain,
    SourceBinding,
)
from sve_carddb.contracts.template_parameters import Range
from sve_carddb.contracts.template_parameters import SourceSpan as ParameterSpan
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.translations.four_layer_normalizer import VERSION, SourcePart
from sve_carddb.domains.translations.four_layer_numbers import (
    number_issue,
    recognize_number,
)
from sve_carddb.domains.translations.four_layer_semantics import (
    CardContext,
    Classified,
    classify_semantics,
)
from sve_carddb.domains.translations.parameters.candidate_matching import classify
from sve_carddb.domains.translations.parameters.references import References, Resolution
from sve_carddb.domains.translations.parameters.spans import Located
from sve_carddb.domains.translations.source_inventory.inventory import entry
from sve_carddb.domains.translations.source_inventory.normalizer import Part, Segment

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.contracts.four_layer import LeafType, Reference
    from sve_carddb.contracts.source_binding import (
        ClosedDomain,
        SourceDescriptor,
        TypedValue,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import SourceField
    from sve_carddb.domains.translations.parameters.models import Candidate, Hint
    from sve_carddb.domains.translations.parameters.rules import Rules

type Leaf = tuple[LeafSlot, TypedValue, LeafOccurrence]

DOMAINS: Mapping[str, ClosedDomain] = {
    "layout.whitespace.v1": LayoutDomain(type="LiteralLayout"),
    **{
        f"quantity.{role}.constant.v1": QuantityDomain(
            type="QuantitySpec",
            modes=("exact", "up_to", "at_least")
            if role == "existence_count"
            else ("exact", "up_to"),
            imports=(),
            expressions=(),
            constant_only=True,
        )
        for role in QUANTITY_ROLES
    },
}


@dataclass(frozen=True)
class Term:
    id: str
    category: str
    source_ja: str


@dataclass
class SourceReferences(References):
    @override
    def proposed_vocabulary(self, kind: str, raw: str) -> Resolution:
        """Only this build's validated catalog supplies permanent vocabulary references."""
        if self.vocabulary is None:
            return Resolution(None, ("missing_current_vocabulary",))
        self.vocabulary.verify()
        if self.vocabulary.bindings and not self.vocabulary.terms:
            raise ValueError("Source leaves require an active catalog")
        try:
            found = self.vocabulary.lookup("jp", kind, raw)
        except ValueError:
            return Resolution(None, ("unknown_or_ambiguous_vocabulary",))
        return Resolution(
            {
                "kind": "vocabulary",
                "vocabulary_kind": kind,
                "vocabulary_code": found.code,
            },
            ("composite_vocabulary_requires_separate_slots",)
            if found.special_kinds
            else (),
        )


@dataclass(frozen=True)
class Recognized:
    schema: LeafSchema
    values: Mapping[str, TypedValue]
    occurrences: tuple[LeafOccurrence, ...]
    issues: tuple[str, ...]
    low_confidence: bool
    semantics: Classified | None = None

    def bind(
        self, source: SourceDescriptor, part: SourcePart
    ) -> tuple[Frame, SourceBinding]:
        """Unknown body semantics stay scoped to the exact occurrence, without macro eligibility."""
        if self.issues:
            raise ValueError("Unresolved source leaves cannot create an active binding")
        occurrence = OccurrenceKey(
            owner=source.owner,
            field=source.field,
            ordinal=source.ordinal,
            source_hash=source.source_hash,
            line_ordinal=part.line_ordinal,
            role=part.source_span.role,
            segments=part.source_span.segments,
        )
        resolved = self.semantics
        # Lexical leaves alone cannot settle timing, actor or cross-sentence scope.
        semantic = (
            SemanticVariant(state="resolved", key=resolved.key, scope=None)
            if resolved is not None
            else SemanticVariant(state="pending", key=None, scope=occurrence)
        )
        frame = Frame(
            id="frame:" + "0" * 64,
            source=Source(
                source_lang="ja",
                canonical_hash=digest(part.canonical_source.encode())[7:],
                normalizer_version=VERSION,
            ),
            role=part.source_span.role,
            semantic_variant=semantic,
            leaf_schema=self.schema,
            projection=resolved.projection
            if resolved is not None
            else Projection(
                projection_kind="pending",
                discriminator=None,
                scopes=(),
                imports=(),
                exports=(),
            ),
            content_hash="0" * 64,
        )
        checksum = hash_payload(frame.payload(part.canonical_source))
        frame = frame.model_copy(
            update={"id": "frame:" + checksum, "content_hash": checksum}
        )
        binding = SourceBinding(
            id="bind:" + "0" * 64,
            source=source,
            ordinal=part.ordinal,
            line_ordinal=part.line_ordinal,
            frame_id=frame.id,
            source_span=part.source_span,
            values=dict(self.values),
            occurrences=self.occurrences,
            trace=part.trace,
        )
        binding = binding.model_copy(
            update={"id": "bind:" + hash_payload(binding.payload())}
        )
        return frame, binding


class Classifier:
    def __init__(
        self, terms: tuple[Term, ...], references: SourceReferences, rules: Rules
    ) -> None:
        self.terms = {term.id: term for term in terms}
        if len(self.terms) != len(terms):
            raise ValueError("Source classifier terms must have unique IDs")
        self.references = references
        self.rules = rules
        expected: dict[str, list[tuple[str, str]]] = {}
        names: dict[str, list[str]] = {}
        for term in terms:
            expected.setdefault(term.source_ja, []).append((term.id, term.category))
            if term.category == "card_name":
                names.setdefault(term.source_ja, []).append(term.id)
        if references.terms != expected or references.card_names != names:
            raise ValueError(
                "Source classifier references differ from exact glossary closure"
            )

    def recognize(
        self,
        raw: str,
        source: SourceDescriptor,
        part: SourcePart,
        *,
        context: CardContext | None = None,
        field: SourceField | None = None,
    ) -> Recognized:
        """Source positions and enabled grammar determine leaf values; target text is never inspected."""
        source.verify(source, raw)
        if part.source_span.role in {"name", "label"}:
            named = self._named(raw, part)
            return Recognized(
                named.schema,
                named.values,
                named.occurrences,
                named.issues,
                named.low_confidence,
                classify_semantics(
                    source, part, named.schema, self.terms, context, field
                )
                if not named.issues
                else None,
            )
        candidate = self._candidate(raw, source, part)
        issues = set(candidate.issues) - {
            "legacy_parenthesis_classification_requires_review"
        }
        leaves = []
        for hint in candidate.slots:
            if hint.issues:
                continue
            if hint.type == "uint" and recognize_number(raw, part, hint) is None:
                issues.add(number_issue(raw, part, hint))
                continue
            if (
                hint.target is not None
                and hint.target.get("kind") == "vocabulary"
                and _vocabulary_role(part, hint) is None
            ):
                issues.add("n0_vocabulary_construction_unresolved")
                continue
            leaves.append(self._hint(raw, part, hint))
        leaves.sort(key=lambda item: item[0].occurrences[0].start)
        values: dict[str, TypedValue] = {}
        slots = []
        occurrences = []
        for index, (slot, value, presence) in enumerate(leaves):
            name = f"leaf_{index}"
            slots.append(slot.model_copy(update={"name": name}))
            values[name] = value
            occurrences.append(presence.model_copy(update={"slot": name}))
        enabled = {rule.rule_id: rule for rule in self.rules.rules}
        doubtful = any(
            enabled[h.rule_id].low_confidence
            for h in candidate.slots
            if h.rule_id in enabled
        )
        schema = LeafSchema(format=2, slots=tuple(slots))
        semantics = (
            classify_semantics(source, part, schema, self.terms, context, field)
            if not issues
            else None
        )
        return Recognized(
            schema,
            values,
            tuple(occurrences),
            tuple(sorted(issues)),
            doubtful,
            semantics,
        )

    def _candidate(
        self, raw: str, source: SourceDescriptor, part: SourcePart
    ) -> Candidate:
        role = part.source_span.role
        if role not in {"body", "reminder", "token_header", "layout"}:
            raise ValueError("Unsupported parameter source role")
        spans = tuple(
            Segment(start=s.start, end=s.end) for s in part.source_span.segments
        )
        projected = Part(part.line_ordinal, role, spans, part.canonical_source)
        ref = SourceRef.model_validate(
            source.source_ref.model_dump()
            | {"text_hash": "sha256:" + source.source_hash}
        )
        located = Located(
            part.ordinal,
            part.line_ordinal,
            ParameterSpan(
                role=role,
                segments=tuple(
                    Range(start=s.start, end=s.end) for s in part.source_span.segments
                ),
                anchor=part.source_span.anchor,
            ),
        )
        return classify(
            raw,
            projected,
            entry(ref, projected, VERSION),
            located,
            self.references,
            self.rules.enabled(),
            units=part.units,
        )[0]

    def _hint(
        self, raw: str, part: SourcePart, hint: Hint
    ) -> tuple[LeafSlot, TypedValue, LeafOccurrence]:
        bounds: Domain
        role = "layout"
        source_unit = None
        spelling = "".join(raw[s.start : s.end] for s in hint.source_segments)
        if hint.type == "uint":
            if hint.value is None:
                raise ValueError("Recognized source number lacks a value")
            number = recognize_number(raw, part, hint)
            assert number is not None
            type_name: LeafType = number.type
            role = number.role
            source_unit = number.source_unit
            bounds = (
                Domain(values=(f"quantity.{role}.constant.v1",), min=None, max=None)
                if type_name == "QuantitySpec"
                else Domain(
                    values=(),
                    min=1
                    if type_name == "Ordinal"
                    or role in {"arithmetic_multiplier", "group_divisor"}
                    else 0,
                    max=SAFE_INTEGER,
                )
            )
            value: TypedValue = number.value
        elif hint.type == "literal":
            type_name, bounds, value = (
                "LiteralLayout",
                Domain(values=("layout.whitespace.v1",), min=None, max=None),
                spelling,
            )
        elif hint.target is not None and hint.target.get("kind") == "term":
            identifier = hint.target.get("id")
            if not isinstance(identifier, str) or identifier not in self.terms:
                raise ValueError("Source reference requires a registered term")
            category = self.terms[identifier].category
            type_name = "CardName" if category == "card_name" else "Concept"
            value = (
                CardNameReference(kind="card_name", term_id=identifier)
                if type_name == "CardName"
                else GlossaryReference(kind="glossary", key=identifier)
            )
            bounds = self._term_domain(category)
            role = (
                (
                    "declared_name"
                    if part.source_span.role == "token_header"
                    else "name_reference"
                )
                if category == "card_name"
                else (
                    "declared_trait"
                    if part.source_span.role == "token_header" and category == "trait"
                    else category
                )
            )
        elif hint.target is not None and hint.target.get("kind") == "vocabulary":
            kind, code = (
                hint.target.get("vocabulary_kind"),
                hint.target.get("vocabulary_code"),
            )
            if not isinstance(kind, str) or not isinstance(code, str):
                raise ValueError("Source vocabulary reference is incomplete")
            type_name = "CardKind" if kind == "type" else "Concept"
            value = VocabularyReference(kind="vocabulary", key=(kind, code))
            found_role = _vocabulary_role(part, hint)
            assert found_role is not None
            role = found_role
            bounds = Domain(
                values=(
                    "card_kind.any.v1" if kind == "type" else "vocabulary.class.v1",
                ),
                min=None,
                max=None,
            )
        else:
            raise ValueError("Source leaf has an unsupported resolved type")
        if hint.type == "literal":
            role = "layout"
        spans = (Span(start=hint.occurrence.start, end=hint.occurrence.end),)
        slot = LeafSlot(
            name=hint.name,
            type=type_name,
            role=role,
            domain=bounds,
            required=True,
            occurrences=spans,
        )
        presence = LeafOccurrence(
            slot=hint.name,
            ordinal=0,
            raw_spans=tuple(
                Span(start=s.start, end=s.end) for s in hint.source_segments
            ),
            canonical_spans=spans,
            source_unit=source_unit,
            source_presence="explicit",
            resolution_rule=None,
        )
        return slot, value, presence

    @staticmethod
    def _term_domain(category: str) -> Domain:
        return Domain(
            values=(
                "card_name.any.v1"
                if category == "card_name"
                else "concept." + category + ".v1",
            ),
            min=None,
            max=None,
        )

    @property
    def domains(self) -> Mapping[str, ClosedDomain]:
        """Catalog membership is checked at binding time and never enters a frame hash."""
        result = dict(DOMAINS)
        for category in ("card_name", "keyword", "ability", "rule_term", "trait"):
            type_name = "CardName" if category == "card_name" else "Concept"
            references = tuple(
                sorted(
                    (
                        CardNameReference(kind="card_name", term_id=term.id)
                        if category == "card_name"
                        else GlossaryReference(kind="glossary", key=term.id)
                        for term in self.terms.values()
                        if term.category == category
                    ),
                    key=lambda reference: canonical(reference.model_dump(mode="json")),
                )
            )
            result[
                "card_name.any.v1"
                if category == "card_name"
                else "concept." + category + ".v1"
            ] = ReferenceDomain.model_validate(
                {"type": type_name, "category": category, "references": references}
            )
        vocabulary = self.references.vocabulary
        for kind, type_name, code in (
            ("class", "Concept", "vocabulary.class.v1"),
            ("type", "CardKind", "card_kind.any.v1"),
        ):
            refs = tuple(
                sorted(
                    (
                        VocabularyReference(
                            kind="vocabulary", key=(term.kind, term.code)
                        )
                        for term in (() if vocabulary is None else vocabulary.terms)
                        if term.kind == kind and term.active
                    ),
                    key=lambda reference: canonical(reference.model_dump(mode="json")),
                )
            )
            result[code] = ReferenceDomain.model_validate(
                {"type": type_name, "category": kind, "references": refs}
            )
        return result

    def _named(self, raw: str, part: SourcePart) -> Recognized:
        exact = "".join(raw[s.start : s.end] for s in part.source_span.segments)
        found = self.references.terms.get(exact, [])
        if len(found) != 1 or (
            part.source_span.role == "name" and found[0][1] != "card_name"
        ):
            return Recognized(
                LeafSchema(format=2, slots=()),
                {},
                (),
                ("missing_or_ambiguous_named_concept",),
                False,
            )
        identifier, category = found[0]
        reference: Reference = (
            CardNameReference(kind="card_name", term_id=identifier)
            if category == "card_name"
            else GlossaryReference(kind="glossary", key=identifier)
        )
        span = Span(start=0, end=len(exact))
        slot = LeafSlot(
            name="leaf_0",
            type="CardName" if category == "card_name" else "Concept",
            role="declared_name" if category == "card_name" else category,
            domain=self._term_domain(category),
            required=True,
            occurrences=(span,),
        )
        occurrence = LeafOccurrence(
            slot=slot.name,
            ordinal=0,
            raw_spans=part.source_span.segments,
            canonical_spans=(span,),
            source_unit=None,
            source_presence="explicit",
            resolution_rule=None,
        )
        return Recognized(
            LeafSchema(format=2, slots=(slot,)),
            {slot.name: reference},
            (occurrence,),
            (),
            False,
        )


def _vocabulary_role(part: SourcePart, hint: Hint) -> str | None:
    assert hint.target is not None
    kind = hint.target.get("vocabulary_kind")
    if kind not in {"type", "class"}:
        return None
    if part.source_span.role == "token_header":
        return "declared_kind" if kind == "type" else "declared_class"
    before = part.canonical_source[: hint.occurrence.start].removesuffix("{")
    after = part.canonical_source[hint.occurrence.end :].removeprefix("}")
    if kind == "type":
        if re.match(
            r"^N(?:枚|体|つ)(?:まで)?(?:を)?選(?:ぶ|び|んで)(?=[。:、）)\n]|$)", after
        ):
            return "counted_kind"
        if re.search(r"(?:元の)?コストN(?:以上|以下)の$", before):
            return "filter_kind"
    elif re.match(r"^(?:の|・)(?:カード|フォロワー|アミュレット|スペル)", after):
        return "class_filter"
    return None

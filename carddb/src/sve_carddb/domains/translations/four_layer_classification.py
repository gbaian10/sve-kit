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
from sve_carddb.contracts.source_binding import (
    LayoutDomain,
    LeafOccurrence,
    SourceBinding,
    ZoneDomain,
)
from sve_carddb.contracts.template_parameters import Range
from sve_carddb.contracts.template_parameters import SourceSpan as ParameterSpan
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.translations.four_layer_normalizer import VERSION, SourcePart
from sve_carddb.domains.translations.parameters.candidate_matching import classify
from sve_carddb.domains.translations.parameters.keyword_aliases import KEYWORD_ALIASES
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
    from sve_carddb.domains.translations.parameters.models import Candidate, Hint
    from sve_carddb.domains.translations.parameters.rules import Rules

type Leaf = tuple[LeafSlot, TypedValue, LeafOccurrence]

SAFE_INTEGER = 9007199254740991
ZONE_CODES = ("battlefield", "deck", "evolve_deck", "ex", "graveyard", "hand")
DOMAINS: Mapping[str, ClosedDomain] = {
    "layout.whitespace.v1": LayoutDomain(type="LiteralLayout"),
    "zones.v1": ZoneDomain(
        type="ZoneSet",
        zones=ZONE_CODES,
    ),
}
POSITIVE_ROLES = frozenset(
    {
        "choice_ordinal",
        "card_ordinal",
        "repetition_ordinal",
        "turn_ordinal",
        "deck_top_ordinal",
    }
)
_PHASES = {
    "term:phase.start": "start",
    "term:phase.main": "main",
    "term:phase.end": "end",
}
_ZONES = {"term:zone." + code: code for code in ZONE_CODES}


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
        role = part.source_span.role
        metadata = role in {"name", "label", "layout"}
        # Lexical leaves alone cannot settle timing, actor or cross-sentence scope.
        semantic = (
            SemanticVariant(
                state="resolved", key="metadata." + role + ".v1", scope=None
            )
            if metadata
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
            projection=Projection(
                projection_kind="none" if metadata else "pending",
                discriminator="metadata." + role + ".v1" if metadata else None,
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
        self, raw: str, source: SourceDescriptor, part: SourcePart
    ) -> Recognized:
        """Source positions and enabled grammar determine leaf values; target text is never inspected."""
        source.verify(source, raw)
        if part.source_span.role in {"name", "label"}:
            return self._named(raw, part)
        candidate = self._candidate(raw, source, part)
        issues = set(candidate.issues) - {
            "legacy_parenthesis_classification_requires_review"
        }
        leaves = []
        for hint in candidate.slots:
            if hint.issues:
                continue
            leaves.append(self._hint(raw, hint))
        leaves.extend(self._lexical(part, leaves))
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
        return Recognized(
            LeafSchema(format=2, slots=tuple(slots)),
            values,
            tuple(occurrences),
            tuple(sorted(issues)),
            doubtful,
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
        )[0]

    def _hint(
        self, raw: str, hint: Hint
    ) -> tuple[LeafSlot, TypedValue, LeafOccurrence]:
        bounds: Domain
        spelling = "".join(raw[s.start : s.end] for s in hint.source_segments)
        if hint.type == "uint":
            if hint.value is None:
                raise ValueError("Recognized source number lacks a value")
            type_name: LeafType = (
                "Ordinal" if hint.semantic_role in POSITIVE_ROLES else "Nat"
            )
            bounds = Domain(
                values=(), min=1 if type_name == "Ordinal" else 0, max=SAFE_INTEGER
            )
            value: TypedValue = hint.value
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
        elif hint.target is not None and hint.target.get("kind") == "vocabulary":
            kind, code = (
                hint.target.get("vocabulary_kind"),
                hint.target.get("vocabulary_code"),
            )
            if not isinstance(kind, str) or not isinstance(code, str):
                raise ValueError("Source vocabulary reference is incomplete")
            type_name = "CardKind" if kind == "type" else "Concept"
            value = VocabularyReference(kind="vocabulary", key=(kind, code))
            vocabulary = self.references.vocabulary
            assert vocabulary is not None
            refs = [
                VocabularyReference(kind="vocabulary", key=(t.kind, t.code))
                for t in vocabulary.terms
                if t.kind == kind and t.active
            ]
            ordered = tuple(
                sorted(refs, key=lambda v: canonical(v.model_dump(mode="json")))
            )
            bounds = Domain(
                values=ordered,
                min=None,
                max=None,
            )
        else:
            raise ValueError("Source leaf has an unsupported resolved type")
        spans = (Span(start=hint.occurrence.start, end=hint.occurrence.end),)
        slot = LeafSlot(
            name=hint.name,
            type=type_name,
            role=hint.semantic_role,
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
            source_unit=_source_unit(raw, hint),
            source_presence="explicit",
            resolution_rule=None,
        )
        return slot, value, presence

    def _term_domain(self, category: str) -> Domain:
        values: list[CardNameReference | GlossaryReference] = [
            CardNameReference(kind="card_name", term_id=t.id)
            if category == "card_name"
            else GlossaryReference(kind="glossary", key=t.id)
            for t in self.terms.values()
            if t.category == category
        ]
        ordered = tuple(
            sorted(values, key=lambda v: canonical(v.model_dump(mode="json")))
        )
        return Domain(
            values=ordered,
            min=None,
            max=None,
        )

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
            role="card_name" if category == "card_name" else "label",
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

    def _lexical(self, part: SourcePart, occupied: list[Leaf]) -> list[Leaf]:
        """Source grammar skips protected names before any phase, zone or alias recognition."""
        text = part.canonical_source
        covered = {
            i
            for slot, _, _ in occupied
            for span in slot.occurrences
            for i in range(span.start, span.end)
        }
        protected = tuple(re.finditer(r"『[^』]*』", text))
        candidates: list[tuple[int, int, LeafType, str, str]] = []
        for identifier, code in {**_PHASES, **_ZONES}.items():
            term = self.terms.get(identifier)
            if term is None or not term.source_ja or term.category != "rule_term":
                continue
            for match in re.finditer(re.escape(term.source_ja), text):
                role = _context_role(text[match.end() :], identifier in _PHASES)
                if role is not None:
                    candidates.append(
                        (
                            match.start(),
                            match.end(),
                            "Phase" if identifier in _PHASES else "ZoneSet",
                            role,
                            code,
                        )
                    )
        for alias in KEYWORD_ALIASES.values():
            term = self.terms.get(alias.target)
            if (
                term is not None
                and term.source_ja == alias.full_name
                and term.category == "ability"
            ):
                candidates.extend(
                    (m.start(1), m.end(1), "Concept", "ability", alias.target)
                    for m in re.finditer(
                        r"【(" + alias.spelling + r")_[N0-9０-９]+】", text
                    )
                )
        result: list[Leaf] = []
        for start, end, kind, role, code in sorted(
            candidates, key=lambda item: (-(item[1] - item[0]), item[0])
        ):
            if any(i in covered for i in range(start, end)) or any(
                m.start() < end and start < m.end() for m in protected
            ):
                continue
            covered.update(range(start, end))
            result.append(self._lexical_leaf(part, start, end, kind, role, code))
        return result

    def _lexical_leaf(
        self,
        part: SourcePart,
        start: int,
        end: int,
        kind: LeafType,
        role: str,
        code: str,
    ) -> Leaf:
        origins = sorted(
            {(s.start, s.end) for unit in part.units[start:end] for s in unit.origins}
        )
        merged: list[Span] = []
        for first, last in origins:
            if merged and merged[-1].end >= first:
                merged[-1] = Span(start=merged[-1].start, end=max(merged[-1].end, last))
            else:
                merged.append(Span(start=first, end=last))
        span = Span(start=start, end=end)
        if kind == "ZoneSet":
            domain = Domain(values=("zones.v1",), min=None, max=None)
            value: TypedValue = (code,)
        elif kind == "Phase":
            domain = Domain(values=("end", "main", "start"), min=None, max=None)
            value = code
        else:
            domain = self._term_domain("ability")
            value = GlossaryReference(kind="glossary", key=code)
        slot = LeafSlot(
            name="lexical",
            type=kind,
            role=role,
            domain=domain,
            required=True,
            occurrences=(span,),
        )
        presence = LeafOccurrence(
            slot="lexical",
            ordinal=0,
            raw_spans=tuple(merged),
            canonical_spans=(span,),
            source_unit=None,
            source_presence="explicit",
            resolution_rule=None,
        )
        return slot, value, presence


def _context_role(after: str, phase: bool) -> str | None:
    if phase:
        if after.startswith(("開始時", "終了時")):
            return "phase_trigger"
        if after.startswith("まで"):
            return "phase_duration"
        return None
    for suffix, role in (
        ("から", "source_zone"),
        ("に", "destination_zone"),
        ("の", "counted_zone"),
    ):
        if after.startswith(suffix):
            return role
    return None


def _source_unit(raw: str, hint: Hint) -> str | None:
    if hint.type != "uint":
        return None
    after = raw[hint.source_segments[-1].end :]
    for spelling in ("ターン", "PP", "ＰＰ", "枚", "体", "つ", "点", "回"):
        if after.startswith(spelling):
            return spelling
    return None

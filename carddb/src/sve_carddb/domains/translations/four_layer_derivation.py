"""Prepare N1 whole-line semantics without installing them in the N0 build."""

import re
from dataclasses import dataclass, replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    CardNP,
    FaceRevisionOwner,
    PrintingFaceOwner,
    Projection,
    UnionNP,
)
from sve_carddb.domains.translations.four_layer_classification import Recognized
from sve_carddb.domains.translations.four_layer_matching import bind_recognized
from sve_carddb.domains.translations.four_layer_n1 import (
    Attempt,
    omitted_owners,
    operands,
)
from sve_carddb.domains.translations.four_layer_n1_leaves import (
    Leaf,
    filters,
    leaf,
    lexical,
    quantity,
    structural,
    valid_unit,
)
from sve_carddb.domains.translations.four_layer_n1_transform import transform
from sve_carddb.domains.translations.four_layer_normalizer import (
    SourceField,
    normalize_source,
)
from sve_carddb.domains.translations.four_layer_numbers import (
    number_issue,
    recognize_number,
)
from sve_carddb.domains.translations.four_layer_semantics import Classified
from sve_carddb.domains.translations.recognition.candidate_matching import classify
from sve_carddb.domains.translations.recognition.lexical import Part
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame, Span, Target
    from sve_carddb.contracts.source_binding import SourceBinding, SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_n1 import Operand
    from sve_carddb.domains.translations.four_layer_normalizer import SourcePart
    from sve_carddb.domains.translations.four_layer_semantics import CardContext
    from sve_carddb.domains.translations.recognition.models import Hint

# This preparation version is deliberately absent from the authored reader.
VERSION = "four-layer-jp-v2"
# Separators follow only a keyword, so a braced flag line stays exactly the flag.
_KEYWORDS = re.compile(r"(?:【(?P<keyword>[^【】]+)】[ 、]*|\{(?P<ability>[^{}]+)\})")
_QUICK = "term:ability.quick"


@dataclass(frozen=True)
class DerivedField:
    field: SourceField
    recognized: tuple[Recognized, ...]
    registry_rows: tuple[tuple[str, ...], ...]
    attempts: tuple[tuple[Attempt, ...], ...]

    def verify(
        self,
        raw: str,
        classifier: Classifier,
        frame: Frame,
        binding: SourceBinding,
        *,
        context: CardContext | None = None,
        target: Target | None = None,
    ) -> None:
        """Reconstruct current source evidence, including empty-span operands and union aliases."""
        expected = derive_source(raw, self.field.source, classifier, context=context)
        if self.field != expected.field:
            raise ValueError(
                "Source field differs from the current N1 partition and transformation"
            )
        ordinal = binding.ordinal
        if ordinal >= len(expected.field.parts):
            raise ValueError("Binding ordinal lies outside the exact field")
        part = expected.field.parts[ordinal]
        found = expected.recognized[ordinal]
        if found.issues:
            raise ValueError("Unresolved source leaves cannot verify a binding")
        part.verify(raw, frame, binding, classifier.domains, normalizer_version=VERSION)
        expected_frame, _ = found.bind(
            expected.field.source, part, normalizer_version=VERSION
        )
        reconstructed = bind_recognized(frame, expected.field, part, found)
        if (
            reconstructed is None
            or reconstructed != binding
            or frame.semantic_variant != expected_frame.semantic_variant
            or frame.projection != expected_frame.projection
        ):
            raise ValueError(
                "N1 binding differs from the complete current source construction"
            )

        if target is not None:
            original = normalize_source(raw, self.field.source).parts[ordinal]
            _verify_card_operands(original, binding, target)

    def bind(self, ordinal: int) -> tuple[Frame, SourceBinding]:
        """An unresolved line can translate precisely, but cannot share its semantic scope."""
        return self.recognized[ordinal].bind(
            self.field.source,
            self.field.parts[ordinal],
            normalizer_version=VERSION,
        )

    @property
    def pending_causes(self) -> tuple[tuple[str, ...], ...]:
        """Lexical failure and unknown whole-line semantics remain separate diagnostics."""
        return tuple(
            found.issues
            or (() if found.semantics else ("unresolved_whole_line_semantics",))
            for found in self.recognized
        )


def derive_source(
    raw: str,
    source: SourceDescriptor,
    classifier: Classifier,
    *,
    context: CardContext | None = None,
) -> DerivedField:
    """Use one source partition and catalog; never infer a semantic key from target text."""
    if context is not None and context.source != source:
        raise ValueError("Semantic context belongs to another exact owner field")
    field = normalize_source(raw, source)
    parts = []
    recognized = []
    registry = []
    attempts = []
    for part in field.parts:
        tried: tuple[Attempt, ...]
        n0 = classifier.recognize(raw, source, part, field=field, context=context)
        if (
            part.source_span.role != "body"
            or source.field not in {"effect", "section"}
            or not isinstance(source.owner, (FaceRevisionOwner, PrintingFaceOwner))
            or (n0.semantics is not None and n0.semantics.key != "card_keywords.v1")
        ):
            updated, found, rows, tried = (
                part,
                n0,
                tuple("N0" for _ in n0.schema.slots),
                (),
            )
        else:
            updated, found, rows, tried = _body(
                raw, field, part, n0, classifier, context
            )
        parts.append(updated)
        recognized.append(found)
        registry.append(rows)
        attempts.append(tried)
    return DerivedField(
        SourceField(source, tuple(parts)),
        tuple(recognized),
        tuple(registry),
        tuple(attempts),
    )


def _body(
    raw: str,
    field: SourceField,
    part: SourcePart,
    n0: Recognized,
    classifier: Classifier,
    context: CardContext | None,
) -> tuple[SourcePart, Recognized, tuple[str, ...], tuple[Attempt, ...]]:
    candidate, _ = classify(
        raw,
        Part(
            part.source_span.role,
            part.source_span.segments,
            part.canonical_source,
            part.units,
        ),
        classifier.references,
        classifier.rules.enabled(),
        units=part.units,
    )
    found, tried = operands(part.canonical_source)
    found, additions, issues, dependencies = _source_additions(
        raw,
        part,
        classifier,
        candidate.slots,
        tuple(o for o in found if valid_unit(o)),
    )
    additions, inherited = _inherit(raw, field, part, n0, classifier, additions)
    issues.update(inherited)
    state = _keyword_semantics(
        part.canonical_source, field, part, additions, classifier, context
    )
    issues.update(state[1])
    result = transform(
        raw,
        part,
        additions,
        found,
        Recognized(
            n0.schema,
            {},
            (),
            tuple(sorted(issues)),
            _confidence(n0, classifier, dependencies),
            state[0] if not issues else None,
        ),
    )
    return *result, tried


def _confidence(n0: Recognized, classifier: Classifier, dependencies: set[str]) -> bool:
    return n0.low_confidence or any(
        r.low_confidence for r in classifier.rules.rules if r.rule_id in dependencies
    )


def _source_additions(
    raw: str,
    part: SourcePart,
    classifier: Classifier,
    hints: tuple[Hint, ...],
    accepted: tuple[Operand, ...],
) -> tuple[tuple[Operand, ...], list[Leaf], set[str], set[str]]:
    kept = []
    additions = []
    issues: set[str] = set()
    dependencies: set[str] = set()
    for operand in accepted:
        evidence = filters(operand, classifier)
        if evidence is None:
            continue
        kept.append(operand)
        additions.extend(structural(operand, part.canonical_source))
        selected, failed = evidence
        additions.extend(selected)
        issues.update(failed)
        for hint in hints:
            if item := quantity(operand, hint, part, raw):
                additions.append(item)
                if hint.rule_id:
                    dependencies.add(hint.rule_id)
    for operand, rule, row in omitted_owners(part.canonical_source, tuple(kept)):
        position = operand.position or (
            operand.zone_spans[0] if operand.zone_spans else None
        )
        assert position is not None
        additions.append(
            replace(
                leaf(
                    "Player",
                    operand.role + "_owner",
                    "player.self.v1",
                    "self",
                    (),
                    "N1-SRC07." + row,
                ),
                resolution_rule=rule,
                # Explicit occurrences sort by raw position, so the use point must too.
                use=part.units[position.start].origins[0].start,
            )
        )
    selected, failed, used = lexical(part, classifier, hints)
    additions.extend(selected)
    issues.update(failed)
    dependencies.update(used)
    return tuple(kept), additions, issues, dependencies


def _inherit(
    raw: str,
    field: SourceField,
    part: SourcePart,
    n0: Recognized,
    classifier: Classifier,
    additions: list[Leaf],
) -> tuple[list[Leaf], set[str]]:
    candidate, _ = classify(
        raw,
        Part(
            part.source_span.role,
            part.source_span.segments,
            part.canonical_source,
            part.units,
        ),
        classifier.references,
        classifier.rules.enabled(),
        units=part.units,
    )
    hints = candidate.slots
    issues = set(n0.issues)
    # Repaired outer numbers must not hide another unresolved number in a retained modifier.
    repaired = {s for item in additions for s in item.slot.occurrences}
    numeric_errors = {
        number_issue(raw, part, hint, references=classifier.references)
        for hint in hints
        if hint.type == "uint"
        and not hint.issues
        and hint.occurrence not in repaired
        and recognize_number(
            raw, part, hint, field=field, references=classifier.references
        )
        is None
    }
    issues.difference_update(
        {"n0_numeric_construction_unresolved", "source_unit_mismatch"} - numeric_errors
    )
    if not any(
        h.target
        and h.target.get("kind") == "vocabulary"
        and h.occurrence not in repaired
        for h in hints
    ):
        issues.discard("n0_vocabulary_construction_unresolved")
    leaves = []
    for slot in n0.schema.slots:
        if any(
            a.start < b.end and b.start < a.end
            for a in slot.occurrences
            for b in repaired
        ):
            continue
        occurrences = tuple(o for o in n0.occurrences if o.slot == slot.name)
        leaves.append(
            Leaf(
                slot,
                n0.values[slot.name],
                tuple(o.canonical_spans for o in occurrences),
                "N0",
                False,
                occurrences[0].source_unit,
            )
        )
    leaves.extend(additions)
    unique: list[Leaf] = []
    for item in leaves:
        if item.slot.occurrences and any(
            item.slot.occurrences == old.slot.occurrences for old in unique
        ):
            continue
        unique.append(item)
    return unique, issues


def _keyword_semantics(
    text: str,
    field: SourceField,
    part: SourcePart,
    leaves: list[Leaf],
    classifier: Classifier,
    context: CardContext | None,
) -> tuple[Classified | None, tuple[str, ...]]:
    matches = tuple(_KEYWORDS.finditer(text))
    if not matches or "".join(match[0] for match in matches) != text:
        return None, ()
    quick = any(match["ability"] == "クイック" for match in matches)
    if any(
        match["ability"] is not None and match["ability"] != "クイック"
        for match in matches
    ):
        return None, ()
    expected = "braced_ability_reference" if quick else "bracket_keyword_reference"
    if expected not in classifier.rules.enabled():
        return None, ()
    if quick and (reason := _quick_issue(classifier, leaves)):
        return None, (reason,)
    first = next(
        (p.line_ordinal for p in field.parts if p.source_span.role != "layout"), None
    )
    if len(leaves) != len(matches) or (
        quick
        and not (
            len(matches) == 1
            and field.source.field == "effect"
            and part.line_ordinal == first
            and context is not None
            and context.phase == "normal"
            and context.card_kind == "spell"
        )
    ):
        return None, ()
    key = "quick_card_field.v1" if quick else "card_keywords.v2"
    return Classified(
        key,
        Projection(
            projection_kind="card_field",
            discriminator=key,
            scopes=(),
            imports=(),
            exports=(),
        ),
    ), ()


def _quick_issue(classifier: Classifier, leaves: list[Leaf]) -> str | None:
    term = classifier.terms.get(_QUICK)
    if term is not None and term.category != "ability":
        return "invalid_quick_category"
    if not any(
        item.slot.role == "ability" and getattr(item.value, "key", None) == _QUICK
        for item in leaves
    ):
        return "missing_keyword_concept"
    return None


def _verify_card_operands(
    part: SourcePart, binding: SourceBinding, target: Target
) -> None:
    found, _ = operands(part.canonical_source)
    for node in target.nodes:
        cards = (
            (node,)
            if isinstance(node, CardNP)
            else node.args.branches
            if isinstance(node, UnionNP)
            else ()
        )
        for card in cards:
            refs = (
                card.args.kind,
                card.args.quantity,
                card.args.owner,
                card.args.zone,
                card.args.class_,
                card.args.token,
                *card.args.traits,
            )
            used = {ref.slot for ref in refs if ref is not None}
            positions = tuple(
                s for o in binding.occurrences if o.slot in used for s in o.raw_spans
            )
            if not any(_covers_operand(part, operand, positions) for operand in found):
                raise ValueError(
                    "CardNP leaves do not belong to one supported source operand"
                )


def _covers_operand(
    part: SourcePart, operand: Operand, positions: tuple[Span, ...]
) -> bool:
    if operand.noun is None or operand.noun.opaque or not valid_unit(operand):
        return False
    if sum(f.kind == "trait" for f in operand.noun.filters) > 1:
        return False
    spans = merged(
        tuple(
            s
            for u in part.units[operand.span.start : operand.span.end]
            for s in u.origins
        )
    )
    return bool(positions) and all(
        any(s.start <= p.start < p.end <= s.end for s in spans) for p in positions
    )

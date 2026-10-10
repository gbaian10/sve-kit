"""Prepare N1 whole-line semantics without installing them in the N0 build."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSchema,
    LeafSlot,
    PrintingFaceOwner,
    Projection,
    Span,
)
from sve_carddb.contracts.source_binding import LeafOccurrence
from sve_carddb.domains.translations.four_layer_classification import Recognized
from sve_carddb.domains.translations.four_layer_normalizer import normalize_source
from sve_carddb.domains.translations.four_layer_semantics import Classified
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame
    from sve_carddb.contracts.source_binding import SourceBinding, SourceDescriptor
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_normalizer import (
        SourceField,
        SourcePart,
    )
    from sve_carddb.domains.translations.four_layer_semantics import CardContext

# This preparation version is deliberately absent from the authored reader.
VERSION = "four-layer-jp-v2"
# Separators follow only a keyword, so a braced flag line stays exactly the flag.
_KEYWORDS = re.compile(r"(?:【(?P<keyword>[^【】]+)】[ 、]*|\{(?P<ability>[^{}]+)\})")
_QUICK = "term:ability.quick"


@dataclass(frozen=True)
class DerivedField:
    field: SourceField
    recognized: tuple[Recognized, ...]

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
    recognized = []
    for part in field.parts:
        found = _keyword_line(field, part, classifier, context)
        if found is None:
            found = classifier.recognize(
                raw, source, part, field=field, context=context
            )
        recognized.append(found)
    return DerivedField(field, tuple(recognized))


def _keyword_line(
    field: SourceField,
    part: SourcePart,
    classifier: Classifier,
    context: CardContext | None,
) -> Recognized | None:
    if (
        part.source_span.role != "body"
        or field.source.field not in {"effect", "section"}
        or not isinstance(field.source.owner, (FaceRevisionOwner, PrintingFaceOwner))
    ):
        return None
    matches = tuple(_KEYWORDS.finditer(part.canonical_source))
    if not matches or "".join(match[0] for match in matches) != part.canonical_source:
        return None
    slots = []
    values = {}
    occurrences = []
    issues: set[str] = set()
    for index, match in enumerate(matches):
        group = "keyword" if match["keyword"] is not None else "ability"
        if group == "ability" and match[group] != "クイック":
            return None
        if any(
            unit.transformation in {"digits", "quoted"}
            for unit in part.units[match.start(group) : match.end(group)]
        ):
            issues.add("keyword_source_replacement_unresolved")
            continue
        members = tuple(
            term
            for term in classifier.terms.values()
            if term.source_ja == match[group]
            and (
                term.category == "keyword" if group == "keyword" else term.id == _QUICK
            )
        )
        if len(members) != 1:
            issues.add(
                "missing_keyword_concept"
                if not members
                else "ambiguous_keyword_concept"
            )
            continue
        term = members[0]
        if term.id == _QUICK and term.category != "ability":
            issues.add("invalid_quick_category")
            continue
        name = f"leaf_{index}"
        span = Span(start=match.start(group), end=match.end(group))
        slots.append(
            LeafSlot(
                name=name,
                type="Concept",
                role=term.category,
                domain=Domain(
                    values=(f"concept.{term.category}.v1",), min=None, max=None
                ),
                required=True,
                occurrences=(span,),
            )
        )
        values[name] = GlossaryReference(kind="glossary", key=term.id)
        occurrences.append(
            LeafOccurrence(
                slot=name,
                ordinal=0,
                raw_spans=merged(
                    tuple(
                        origin
                        for unit in part.units[span.start : span.end]
                        for origin in unit.origins
                    )
                ),
                canonical_spans=(span,),
                source_unit=None,
                source_presence="explicit",
                resolution_rule=None,
            )
        )
    quick = any(value.key == _QUICK for value in values.values())
    # Only an intrinsic spell flag is settled here; a granted or conditional flag is a body.
    resolved = not issues and (
        not quick
        or (
            len(matches) == 1
            and field.source.field == "effect"
            and context is not None
            and context.phase == "normal"
            and context.card_kind == "spell"
        )
    )
    key = "quick_card_field.v1" if quick else "card_keywords.v2"
    semantics = (
        Classified(
            key,
            Projection(
                projection_kind="card_field",
                discriminator=key,
                scopes=(),
                imports=(),
                exports=(),
            ),
        )
        if resolved
        else None
    )
    return Recognized(
        LeafSchema(format=2, slots=tuple(slots)),
        values,
        tuple(occurrences),
        tuple(sorted(issues)),
        False,
        semantics,
    )

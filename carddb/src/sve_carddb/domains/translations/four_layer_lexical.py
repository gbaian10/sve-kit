"""Bind unquoted lexical terms so legacy prose follows the selected glossary."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Domain,
    GlossaryReference,
    LeafSchema,
    LeafSlot,
    Span,
)
from sve_carddb.contracts.source_binding import LeafOccurrence
from sve_carddb.domains.translations.four_layer_derivation import SourceFamily
from sve_carddb.domains.translations.recognition.provenance import merged

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


def lexical_leaves(
    text: str, schema: LeafSchema, *, trait_type: bool = False
) -> tuple[LeafSlot, ...]:
    """Quoted names and already bound noun filters keep their existing source owner."""
    occupied = [span for slot in schema.slots for span in slot.occurrences]
    occupied.extend(
        Span(start=m.start(), end=m.end())
        for m in re.finditer(r"『[^』]*』|\{[^{}]*\}", text)
    )
    tokens = tuple(
        LeafSlot(
            name=f"token_{match.start()}",
            type="TokenStatus",
            role="token_reference",
            domain=Domain(values=("token_status.token.v1",), min=None, max=None),
            required=True,
            occurrences=(Span(start=match.start(), end=match.end()),),
        )
        for match in re.finditer(r"トークン", text)
        if not any(s.start < match.end() and match.start() < s.end for s in occupied)
    )
    traits = (
        tuple(
            LeafSlot(
                name=f"trait_type_{match.start()}",
                type="Concept",
                role="rule_term",
                domain=Domain(values=("concept.rule_term.v1",), min=None, max=None),
                required=True,
                occurrences=(Span(start=match.start(), end=match.end()),),
            )
            for match in re.finditer(r"タイプ", text)
            if not any(
                s.start < match.end() and match.start() < s.end for s in occupied
            )
        )
        if trait_type
        else ()
    )
    return (*tokens, *traits)


def prepare_lexical(
    raw: str,
    prepared: DerivedField,
    classifier: Classifier,
    context: CardContext | None,
) -> DerivedField:
    """Lexical references add no executable semantics or new source acceptance."""
    del raw, context
    term = classifier.terms.get("term:rule.trait_type")
    trait_type = (
        term is not None and term.category == "rule_term" and term.source_ja == "タイプ"
    )
    if prepared.field.source.field not in {"effect", "section"}:
        return prepared
    found = list(prepared.recognized)
    rows = list(prepared.derived.registry_rows)
    for i, (part, recognized) in enumerate(
        zip(prepared.field.parts, found, strict=True)
    ):
        if part.source_span.role not in {"body", "reminder", "token_header"}:
            continue
        added = lexical_leaves(
            part.canonical_source, recognized.schema, trait_type=trait_type
        )
        if not added:
            continue
        ordered = sorted(
            zip(
                (*recognized.schema.slots, *added),
                (
                    *rows[i],
                    *(
                        "token.reference"
                        if s.type == "TokenStatus"
                        else "trait_type.reference"
                        for s in added
                    ),
                ),
                strict=True,
            ),
            key=lambda pair: (
                pair[0].occurrences[0].start
                if pair[0].occurrences
                else len(part.canonical_source)
            ),
        )
        occurrences = tuple(
            LeafOccurrence(
                slot=slot.name,
                ordinal=0,
                raw_spans=merged(
                    tuple(
                        origin
                        for span in slot.occurrences
                        for unit in part.units[span.start : span.end]
                        for origin in unit.origins
                    )
                ),
                canonical_spans=slot.occurrences,
                source_unit=None,
                source_presence="explicit",
                resolution_rule=None,
            )
            for slot in added
        )
        found[i] = replace(
            recognized,
            schema=LeafSchema(format=2, slots=tuple(slot for slot, _ in ordered)),
            values={
                **recognized.values,
                **{
                    s.name: "token"
                    if s.type == "TokenStatus"
                    else GlossaryReference(kind="glossary", key="term:rule.trait_type")
                    for s in added
                },
            },
            occurrences=tuple(
                sorted(
                    (*recognized.occurrences, *occurrences),
                    key=lambda o: (
                        o.raw_spans[0].start if o.raw_spans else len(part.units)
                    ),
                )
            ),
        )
        rows[i] = tuple(row for _, row in ordered)
    return replace(
        prepared,
        derived=replace(
            prepared.derived, recognized=tuple(found), registry_rows=tuple(rows)
        ),
    )


FAMILY = SourceFamily("lexical", (), prepare_lexical)

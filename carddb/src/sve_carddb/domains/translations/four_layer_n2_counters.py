"""The sealed, explicit capacity sentence is separate from counter placement."""

import re
from dataclasses import replace
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import (
    Domain,
    FaceRevisionOwner,
    GlossaryReference,
    LeafSlot,
    PrintingFaceOwner,
    Span,
)
from sve_carddb.contracts.n0 import SAFE_INTEGER
from sve_carddb.domains.translations.four_layer_n1_leaves import Leaf, leaf
from sve_carddb.domains.translations.four_layer_n1_transform import transform

if TYPE_CHECKING:
    from sve_carddb.domains.translations.four_layer_classification import Classifier
    from sve_carddb.domains.translations.four_layer_derivation import DerivedField

_CAPACITY = re.compile(
    r"これに(?P<name>[^。:：、{}【】「」『』\s]+)カウンターは(?P<value>N)個まで置ける。"
)


def prepare_capacities(
    raw: str, derived: DerivedField, classifier: Classifier
) -> DerivedField:
    """Keep the named kind literal and retain the existing counter reference."""
    if derived.field.source.field not in {"effect", "section"} or not isinstance(
        derived.field.source.owner, (FaceRevisionOwner, PrintingFaceOwner)
    ):
        return derived
    parts = list(derived.field.parts)
    recognized = list(derived.recognized)
    registry = list(derived.registry_rows)
    for index, (part, found) in enumerate(zip(parts, recognized, strict=True)):
        if part.source_span.role != "body":
            continue
        match = _CAPACITY.fullmatch(part.canonical_source)
        if match is None or match["name"] == "スタック":
            continue
        candidate = classifier.candidate(raw, part)
        numbers = tuple(h for h in candidate.slots if h.type == "uint")
        position = Span(start=match.start("value"), end=match.end("value"))
        if len(numbers) != 1:
            continue
        hint = numbers[0]
        if (
            hint.occurrence != position
            or hint.value is None
            or not 0 <= hint.value <= SAFE_INTEGER
            or set(hint.issues) - {"numeric_role_requires_review"}
        ):
            continue
        leaves = [
            Leaf(
                slot,
                found.values[slot.name],
                tuple(
                    o.canonical_spans for o in found.occurrences if o.slot == slot.name
                ),
                row,
                False,
                next(o.source_unit for o in found.occurrences if o.slot == slot.name),
            )
            for slot, row in zip(found.schema.slots, registry[index], strict=True)
        ]
        leaves.append(
            Leaf(
                LeafSlot(
                    name="capacity",
                    type="Nat",
                    role="counter_capacity",
                    domain=Domain(values=(), min=0, max=SAFE_INTEGER),
                    required=True,
                    occurrences=(position,),
                ),
                hint.value,
                ((position,),),
                "N2-B01.capacity",
                abstract=False,
                unit="個",
            )
        )
        counter = classifier.terms.get("term:object.counter")
        missing: tuple[str, ...] = ()
        if counter is None or (counter.category, counter.source_ja) != (
            "rule_term",
            "カウンター",
        ):
            missing = ("missing_counter_concept",)
        elif not any(
            item.value == GlossaryReference(kind="glossary", key=counter.id)
            for item in leaves
        ):
            end = match.start("value") - len("は")
            leaves.append(
                replace(
                    leaf(
                        "Concept",
                        "rule_term",
                        "concept.rule_term.v1",
                        GlossaryReference(kind="glossary", key=counter.id),
                        ((Span(start=end - len("カウンター"), end=end),),),
                        "N2-B01.capacity",
                    ),
                    abstract=False,
                )
            )
        base = replace(
            found,
            issues=missing
            + tuple(
                i
                for i in found.issues
                if i
                not in {
                    "numeric_role_requires_review",
                    "n0_numeric_construction_unresolved",
                }
            ),
        )
        parts[index], recognized[index], registry[index] = transform(
            raw, part, leaves, (), base
        )
    return replace(
        derived,
        field=replace(derived.field, parts=tuple(parts)),
        recognized=tuple(recognized),
        registry_rows=tuple(registry),
    )

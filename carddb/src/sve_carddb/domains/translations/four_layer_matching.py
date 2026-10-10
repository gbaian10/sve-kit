"""Match authored frames to independently classified exact source occurrences."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import hash_payload
from sve_carddb.contracts.n0 import VERSION
from sve_carddb.contracts.source_binding import SourceBinding
from sve_carddb.core.json import canonical

if TYPE_CHECKING:
    from collections.abc import Iterable

    from sve_carddb.contracts.four_layer import Frame, LeafSlot
    from sve_carddb.contracts.source_binding import LeafOccurrence, TypedValue
    from sve_carddb.domains.translations.four_layer_classification import (
        Classifier,
        Recognized,
    )
    from sve_carddb.domains.translations.four_layer_normalizer import (
        SourceField,
        SourcePart,
    )
    from sve_carddb.domains.translations.four_layer_semantics import CardContext


@dataclass(frozen=True)
class Matched:
    frame: Frame
    binding: SourceBinding


def _key(frame: Frame) -> bytes:
    """Pending scope prevents identical bytes on another owner from selecting a frame."""
    return canonical(
        {
            "source": frame.source.model_dump(mode="json"),
            "role": frame.role,
            "semantic_variant": frame.semantic_variant.model_dump(mode="json"),
            "projection": frame.projection.model_dump(mode="json"),
        }
    )


class Frames:
    def __init__(
        self, frames: Iterable[Frame], *, normalizer_version: str = VERSION
    ) -> None:
        self.normalizer_version = normalizer_version
        self.index: dict[bytes, list[Frame]] = {}
        identifiers = set()
        for frame in frames:
            if frame.id in identifiers:
                raise ValueError("Authored frame IDs must be unique")
            identifiers.add(frame.id)
            self.index.setdefault(_key(frame), []).append(frame)

    def match(
        self,
        raw: str,
        field: SourceField,
        part: SourcePart,
        recognized: Recognized,
        classifier: Classifier,
        *,
        context: CardContext | None = None,
    ) -> Matched | None:
        """Authored slot names and repeated occurrences cannot supply source roles or values."""
        derived, _ = recognized.bind(
            field.source, part, normalizer_version=self.normalizer_version
        )
        candidates = self.index.get(_key(derived), ())
        matches = []
        for frame in candidates:
            binding = bind_recognized(frame, field, part, recognized)
            if binding is not None:
                classifier.verify(raw, field, part, frame, binding, context=context)
                matches.append(Matched(frame, binding))
        if len(matches) > 1:
            raise ValueError("Ambiguous authored frames for exact source occurrence")
        if candidates and not matches:
            raise ValueError("Authored frame leaves differ from source classification")
        return matches[0] if matches else None


def _signature(slot: LeafSlot) -> tuple[object, ...]:
    return slot.type, slot.role, slot.domain, slot.required


def bind_recognized(
    frame: Frame, field: SourceField, part: SourcePart, recognized: Recognized
) -> SourceBinding | None:
    """Map complete signatures and construction order, including independent omitted operands."""
    expected_omitted = tuple(s for s in recognized.schema.slots if not s.occurrences)
    actual_omitted = tuple(s for s in frame.leaf_schema.slots if not s.occurrences)
    if len(actual_omitted) != len(expected_omitted) or any(
        _signature(a) != _signature(e)
        for a, e in zip(actual_omitted, expected_omitted, strict=True)
    ):
        return None
    omitted_names = {
        e.name: a.name for a, e in zip(actual_omitted, expected_omitted, strict=True)
    }
    expected = {
        (span.start, span.end): slot
        for slot in recognized.schema.slots
        for span in slot.occurrences
    }
    actual = {
        (span.start, span.end): slot
        for slot in frame.leaf_schema.slots
        for span in slot.occurrences
    }
    if actual.keys() != expected.keys() or any(
        _signature(slot) != _signature(expected[pos]) for pos, slot in actual.items()
    ):
        return None
    values: dict[str, TypedValue] = {}
    occurrences: list[LeafOccurrence] = []
    ordinals: dict[str, int] = {}
    for occurrence in recognized.occurrences:
        mapped = {
            actual[span.start, span.end].name for span in occurrence.canonical_spans
        }
        if not occurrence.canonical_spans:
            mapped = (
                {omitted_names[occurrence.slot]}
                if occurrence.slot in omitted_names
                else set()
            )
        if len(mapped) != 1:
            return None
        name = mapped.pop()
        value = recognized.values[occurrence.slot]
        if name in values and values[name] != value:
            return None
        values[name] = value
        ordinal = ordinals.get(name, 0)
        occurrences.append(
            occurrence.model_copy(update={"slot": name, "ordinal": ordinal})
        )
        ordinals[name] = ordinal + 1
    binding = SourceBinding(
        id="bind:" + "0" * 64,
        source=field.source,
        ordinal=part.ordinal,
        line_ordinal=part.line_ordinal,
        frame_id=frame.id,
        source_span=part.source_span,
        values=values,
        occurrences=tuple(occurrences),
        trace=part.trace,
    )
    return binding.model_copy(update={"id": "bind:" + hash_payload(binding.payload())})

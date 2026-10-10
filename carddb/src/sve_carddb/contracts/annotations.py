"""Content-addressed semantic ranges preserve exact text and repeated references."""

from typing import Annotated, Self

from pydantic import Field, model_validator

from sve_carddb.contracts.four_layer import Id, Reference, Span, disjoint, hash_payload
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import RecordData, UInt


class Annotation(RecordData):
    ordinal: UInt
    reference: Reference
    ranges: Annotated[tuple[Span, ...], Field(min_length=1)]
    bold: bool | None

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        disjoint(self.ranges)
        return self


class AnnotationSet(RecordData):
    id: Annotated[str, Field(pattern=r"^ann:[0-9a-f]{64}\Z")]
    text_unit_id: Id
    occurrences: tuple[Annotation, ...]

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        if tuple(a.ordinal for a in self.occurrences) != tuple(
            range(len(self.occurrences))
        ):
            raise ValueError("Annotation ordinals must be continuous from zero")
        order = tuple(
            (
                a.ranges[0].start,
                a.ranges[-1].end,
                canonical(a.reference.model_dump(mode="json")),
            )
            for a in self.occurrences
        )
        if order != tuple(sorted(order)):
            raise ValueError("Annotations must follow exact source range order")
        disjoint(
            tuple(
                sorted(
                    (r for a in self.occurrences for r in a.ranges),
                    key=lambda r: r.start,
                )
            )
        )
        if self.id != "ann:" + hash_payload(self.payload()):
            raise ValueError("Annotation set identity differs from semantic ranges")
        return self

    def payload(self) -> dict[str, object]:
        """The public tuple encoding cannot alter the logical hash recipe."""
        return {
            "recipe": "annotation-v1",
            **self.model_dump(mode="json", include={"text_unit_id", "occurrences"}),
        }

    def verify(self, text_unit_id: str, lang: str, text: str) -> None:
        """A matching string length cannot authorize ranges on another text identity."""
        expected = "t:" + lang + ":" + digest(text.encode())[7:23]
        if text_unit_id != self.text_unit_id or text_unit_id != expected:
            raise ValueError("Annotation set text identity mismatch")
        if any(r.end > len(text) for a in self.occurrences for r in a.ranges):
            raise ValueError("Annotation range is outside exact text")


class RenderLeafOccurrence(RecordData):
    translation_id: Id
    binding_id: Annotated[str, Field(pattern=r"^bind:[0-9a-f]{64}\Z")]
    node_path: Annotated[tuple[UInt, ...], Field(min_length=1)]
    slot: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")]
    source_ordinals: Annotated[tuple[UInt, ...], Field(min_length=1)]
    ranges: Annotated[tuple[Span, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        if self.source_ordinals != tuple(sorted(set(self.source_ordinals))):
            raise ValueError("Render source ordinals must be sorted and unique")
        disjoint(self.ranges)
        return self

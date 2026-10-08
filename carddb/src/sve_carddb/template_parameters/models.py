"""Translation-contract span/schema shapes, separate from candidate uncertainty."""

from itertools import pairwise
from typing import Annotated, Literal, Self

from pydantic import Field, JsonValue, field_validator, model_validator

from sve_carddb.core.models import Hash, RecordData, Text, UInt
from sve_carddb.template_sources.normalizer import Role

NumericRule = Literal[
    "suffix_unit_cards",
    "suffix_unit_entities",
    "suffix_unit_points",
    "suffix_unit_times",
    "suffix_unit_turns",
    "suffix_unit_pp",
    "prefix_field_cost",
    "prefix_field_attack",
    "prefix_field_health",
    "prefix_field_pp",
    "prefix_field_level",
]


class Range(RecordData):
    start: UInt
    end: UInt

    @model_validator(mode="after")
    def ordered(self) -> Self:
        """Do not accept empty intervals as evidence for a source fragment."""
        if self.end <= self.start:
            raise ValueError("Parameter range must be nonempty and increasing")
        return self


class SourceSpan(RecordData):
    role: Role
    segments: Annotated[tuple[Range, ...], Field(min_length=1)]
    anchor: UInt | None

    @model_validator(mode="after")
    def ordered(self) -> Self:
        """Only reminders can point to a body; top-level ranges cannot overlap."""
        if any(a.end > b.start for a, b in pairwise(self.segments)):
            raise ValueError("Source span segments must be sorted and disjoint")
        if self.role != "reminder" and self.anchor is not None:
            raise ValueError("Only reminder spans may have an anchor")
        return self


class Slot(RecordData):
    name: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_]*\Z")]
    type: Literal["uint", "literal", "reference"]
    occurrences: Annotated[tuple[Range, ...], Field(min_length=1)]
    reference_kind: Literal["card", "term", "vocabulary"] | None
    min: UInt | None
    max: UInt | None

    @model_validator(mode="after")
    def typed(self) -> Self:
        """A syntactic placeholder alone cannot authorize a reference kind or bounds."""
        if any(a.end > b.start for a, b in pairwise(self.occurrences)):
            raise ValueError("Slot occurrences must be sorted and disjoint")
        if (self.type == "reference") != (self.reference_kind is not None):
            raise ValueError("Slot reference kind disagrees with its type")
        if self.type == "uint":
            if self.min is None or self.max is None or self.min > self.max:
                raise ValueError("Unsigned slot requires ordered safe integer bounds")
        elif self.min is not None or self.max is not None:
            raise ValueError("Only unsigned slots may declare bounds")
        return self


class Schema(RecordData):
    format: Literal[1] = 1
    slots: tuple[Slot, ...]

    @field_validator("format", mode="before")
    @classmethod
    def integer(cls, value: object) -> object:
        """Literal validation would otherwise coerce bool/float into an integer version tag."""
        if type(value) is not int:
            raise ValueError("Parameter schema format must be integer one")
        return value

    @model_validator(mode="after")
    def ordered(self) -> Self:
        """Reject shared positions and duplicate names before matching source values."""
        if len({slot.name for slot in self.slots}) != len(self.slots):
            raise ValueError("Parameter slot names must be unique")
        if self.slots != tuple(
            sorted(self.slots, key=lambda s: s.occurrences[0].start)
        ):
            raise ValueError("Parameter slots must follow their first occurrence")
        positions = sorted(
            (span for slot in self.slots for span in slot.occurrences),
            key=lambda span: span.start,
        )
        if any(a.end > b.start for a, b in pairwise(positions)):
            raise ValueError("Parameter occurrences must not overlap")
        return self


class Hint(RecordData):
    name: Text
    occurrence: Range
    source_segments: tuple[Range, ...]
    transformation: Text
    semantic_role: Text
    numeric_rule: NumericRule | None
    rule_id: Text | None = None
    type: Literal["uint", "literal", "reference"] | None
    reference_kind: Literal["card", "term", "vocabulary"] | None
    value: UInt | None
    target: dict[str, JsonValue] | None
    issues: tuple[Text, ...]


class LiteralTrace(RecordData):
    occurrence: Range
    source_segments: tuple[Range, ...]


class Candidate(RecordData):
    inventory_id: Text
    ordinal: UInt
    line_ordinal: UInt
    source_span: SourceSpan
    normalizer_id: Text
    normalized_hash: Hash
    parameter_normalizer_id: Text
    template_normalized_hash: Hash
    parameter_schema: Schema | None
    slots: tuple[Hint, ...]
    literal_trace: tuple[LiteralTrace, ...]
    issues: tuple[Text, ...]
    signature_hash: Hash
    payload_hash: Hash | None
    adoption_status: Literal["candidate_only"] = "candidate_only"

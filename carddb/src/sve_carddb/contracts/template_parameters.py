"""Shared template span and parameter schema shapes."""

from itertools import pairwise
from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.core.models import RecordData, UInt

type Role = Literal["body", "reminder", "token_header", "layout"]


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

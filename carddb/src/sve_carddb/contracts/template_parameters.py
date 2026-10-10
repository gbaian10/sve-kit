"""Shared template span and parameter schema shapes."""

from typing import Literal, Self

from pydantic import model_validator

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

"""Exact machine-review bytes precede adoption; a bare draft verdict is not a final-text review."""

from typing import Literal, Self

from pydantic import model_validator

from sve_carddb.registry.records import Hash, Instant, RecordData, Text
from sve_carddb.snapshot.values import digest


class Resolution(RecordData):
    reviewed_by: Text
    reviewed_at: Instant
    note: Text


class ModelReview(RecordData):
    translated_by: Text
    reviewed_by: Text
    reviewed_at: Instant
    text_hash: Hash
    result: Literal["agreed", "disputed"]
    resolution: Resolution | None

    @model_validator(mode="after")
    def _review(self) -> Self:
        if not self.translated_by.strip() or not self.reviewed_by.strip():
            raise ValueError(
                "Template model review must identify both models and versions"
            )
        if self.translated_by.strip().casefold() == self.reviewed_by.strip().casefold():
            raise ValueError("Template translation and review require different models")
        if self.result == "agreed" and self.resolution is not None:
            raise ValueError(
                "Agreed template model review must not carry a dispute resolution"
            )
        return self


def verify(text: str, review: ModelReview) -> None:
    """Never carry a draft verdict across escaping, slot replacement or later text edits."""
    if digest(text.encode()) != review.text_hash:
        raise ValueError("Template model review must pin the final exact text hash")


def require_resolved_dispute(review: ModelReview, *, sampled: bool) -> None:
    """This guard only checks a supplied decision; it does not create an adoption receipt."""
    if review.result == "disputed" and (review.resolution is None or not sampled):
        raise ValueError(
            "Disputed template translation requires human resolution and an actual sample"
        )

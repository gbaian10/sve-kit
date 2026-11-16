"""Reviewable image evidence and explicit unknown results."""

from typing import Annotated, Literal
from urllib.parse import urlsplit

from pydantic import Field, field_validator, model_validator

from sve_carddb.core.models import Date, Hash, RecordData, Text, UInt
from sve_carddb.domains.digital.links.models import Game, Phase
from sve_carddb.domains.registry.records import CardId, FaceId, PrintingId
from sve_carddb.domains.registry.transitions.models import ArtId
from sve_carddb.images.art_matching import RECIPE, REVIEW_MAX, REVIEW_MIN

type Box = tuple[UInt, UInt, UInt, UInt]
type StyleKey = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_-]*\Z")]


class Witness(RecordData):
    printing_id: PrintingId
    face_id: FaceId
    image_url: Text
    image_sha256: Hash
    crop: Box


class CandidateKey(RecordData):
    official_id: Text
    phase: Phase
    style_key: StyleKey

    def key(self) -> tuple[str, Phase, str]:
        """Identify an image style independently of its bytes."""
        return self.official_id, self.phase, self.style_key


class Candidate(CandidateKey):
    image_url: Text
    image_sha256: Hash
    crop: Box
    inliers: UInt


class ManualReview(RecordData):
    source: Text
    reviewed_on: Date
    matched_candidates: tuple[CandidateKey, ...]

    @field_validator("source")
    @classmethod
    def _source(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Manual art review requires a nonblank source")
        return value


class Record(RecordData):
    art_id: ArtId
    card_id: CardId
    face_id: FaceId
    game: Game
    state: Literal["matched", "not_found", "unknown"]
    same_card_targets: Annotated[tuple[Text, ...], Field(min_length=1)]
    witness: Witness | None
    candidates: tuple[Candidate, ...]
    complete: bool
    review_level: Literal["model_reviewed", "sampled", "confirmed"]
    reason: Text
    manual_review: ManualReview | None = Field(
        default=None, exclude_if=lambda value: value is None
    )

    def matches(self) -> tuple[Candidate, ...]:
        """Return only candidates adopted by this conclusion."""
        if self.state != "matched":
            return ()
        if self.manual_review is not None:
            selected = {item.key() for item in self.manual_review.matched_candidates}
            return tuple(c for c in self.candidates if c.key() in selected)
        return tuple(c for c in self.candidates if c.inliers >= REVIEW_MAX)

    def _validate_manual_review(self, keys: set[tuple[str, Phase, str]]) -> None:
        manual = self.manual_review
        if manual is None:
            return
        if self.review_level != "confirmed" or self.state == "unknown":
            raise ValueError("Manual art review requires a confirmed conclusion")
        selected = {item.key() for item in manual.matched_candidates}
        if len(selected) != len(manual.matched_candidates) or not selected <= keys:
            raise ValueError("Manual art review must select unique existing candidates")
        if (self.state == "matched") != bool(selected):
            raise ValueError("Manual art review matches must agree with its state")

    @model_validator(mode="after")
    def _comparison(self) -> Record:
        if tuple(sorted(set(self.same_card_targets))) != self.same_card_targets:
            raise ValueError("Art targets must be sorted and unique")
        identifiers = (
            *self.same_card_targets,
            *(c.official_id for c in self.candidates),
        )
        if any(
            len(value) != (9 if self.game == "sv1" else 8)
            or not value.isascii()
            or not value.isdecimal()
            for value in identifiers
        ):
            raise ValueError("Digital art official ID width mismatch")
        keys = {c.key() for c in self.candidates}
        if len(keys) != len(self.candidates):
            raise ValueError("Duplicate digital art candidate")
        manual = self.manual_review
        self._validate_manual_review(keys)
        if self.state != "unknown" and (self.witness is None or not self.candidates):
            raise ValueError("Art conclusions require images and a witness")
        if self.state == "not_found" and (
            not self.complete
            or (
                manual is None and any(c.inliers >= REVIEW_MIN for c in self.candidates)
            )
        ):
            raise ValueError("Art absence requires all candidates below review")
        if (
            self.state == "matched"
            and manual is None
            and not any(c.inliers >= REVIEW_MAX for c in self.candidates)
        ):
            raise ValueError("Art match requires a decisive candidate")
        host = "shadowverse-portal.com" if self.game == "sv1" else "shadowverse-wb.com"
        if any(
            urlsplit(c.image_url).scheme != "https"
            or urlsplit(c.image_url).hostname != host
            for c in self.candidates
        ):
            raise ValueError("Digital art requires official image URLs")
        return self

    @field_validator("reason")
    @classmethod
    def _reason(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Art comparison reason must be nonblank")
        return value


class Index(RecordData):
    format: Literal[1]
    kind: Literal["digital_art_match_index"]
    recipe: Literal["sift800-ratio075-ransac5-v1"] = RECIPE
    opencv_version: Text
    as_of: Date
    includes: dict[str, Hash]


class Shard(RecordData):
    format: Literal[1]
    kind: Literal["digital_art_match_shard"]
    records: Annotated[tuple[Record, ...], Field(min_length=1)]

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


class Witness(RecordData):
    printing_id: PrintingId
    face_id: FaceId
    image_url: Text
    image_sha256: Hash
    crop: Box


class Candidate(RecordData):
    official_id: Text
    phase: Phase
    style_key: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_-]*\Z")]
    image_url: Text
    image_sha256: Hash
    crop: Box
    inliers: UInt


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
        keys = {(c.official_id, c.phase, c.style_key) for c in self.candidates}
        if len(keys) != len(self.candidates):
            raise ValueError("Duplicate digital art candidate")
        if self.state != "unknown" and (self.witness is None or not self.candidates):
            raise ValueError("Art conclusions require images and a witness")
        if self.state == "not_found" and (
            not self.complete or any(c.inliers >= REVIEW_MIN for c in self.candidates)
        ):
            raise ValueError("Art absence requires all candidates below review")
        if self.state == "matched" and not any(
            c.inliers >= REVIEW_MAX for c in self.candidates
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

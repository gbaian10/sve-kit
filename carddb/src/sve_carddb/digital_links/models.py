"""Strict digital-link-adoption format 1; coverage is deliberately unsupported."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves nested annotations at runtime

from typing import Annotated, Literal

from pydantic import Field, field_validator, model_validator

from sve_carddb.catalog.adoption_models import Predecessor, ReviewContext, SourceRef
from sve_carddb.products.models import Lang
from sve_carddb.registry.records import (
    CardId,
    FaceId,
    Hash,
    PrintingId,
    RecordData,
    Text,
)

Game = Literal["sv1", "svwb"]
Phase = Literal["normal", "evolved"]


class Subject(RecordData):
    card_id: CardId
    face_id: FaceId | None
    game: Game
    official_id: Text
    digital_phase: Phase | None

    @model_validator(mode="after")
    def _target(self) -> Subject:
        if (self.face_id is None) != (self.digital_phase is None):
            raise ValueError("Digital link requires both faces or neither face")
        if (
            len(self.official_id) != (9 if self.game == "sv1" else 8)
            or not self.official_id.isascii()
            or not self.official_id.isdecimal()
        ):
            raise ValueError("Digital link official ID width mismatch")
        return self


class SveName(RecordData):
    printing_id: PrintingId
    face_id: FaceId
    name_ref: SourceRef


class DigitalName(RecordData):
    phase: Phase
    lang: Lang
    name_ref: SourceRef


class Value(RecordData):
    relation: Literal["same_card", "same_character", "name_only"]
    effect_similarity: Literal["near_identical", "core_kept", "reworked"] | None
    sve_names: Annotated[tuple[SveName, ...], Field(min_length=1)]
    digital_names: Annotated[tuple[DigitalName, ...], Field(min_length=1)]


class Data(RecordData):
    subject: Subject
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None
    value: Value | None
    review_context_hash: Hash
    reason: Text

    @field_validator("reason")
    @classmethod
    def _reason(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Digital link reason must be nonblank")
        return value


class Evidence(RecordData):
    source_ref: SourceRef
    role: Literal["sve_name", "digital_name"]


class Record(RecordData):
    record_key: Text
    kind: Literal["digital_link_adoption"]
    filing_key: Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+\Z")]
    data: Data
    evidence: tuple[Evidence, ...]


class Decision(RecordData):
    id: Annotated[str, Field(pattern=r"^d:[0-9a-f]{64}\Z")]
    state: Literal["sampled", "confirmed"]
    scope: Literal["batch"]
    category: Literal["digital_link"]
    policy_id: Literal["digital-link-v1"]
    membership_hash: Hash
    members: tuple[tuple[Text, Hash], ...]
    sample_ids: tuple[Text, ...]
    note: str


class Envelope(RecordData):
    digital_link_authored_format: Literal[1]

    @field_validator("digital_link_authored_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Digital link format must be integer one")
        return value


class Index(Envelope):
    kind: Literal["digital_link_index"]
    includes: dict[str, Hash]


class Shard(Envelope):
    kind: Literal["digital_link_shard"]
    review_context: ReviewContext
    default_decision_id: Text
    records: Annotated[tuple[Record, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]

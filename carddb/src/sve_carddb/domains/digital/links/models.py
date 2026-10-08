"""Strict current digital-link format; coverage is deliberately unsupported."""

from typing import Annotated, Literal

from pydantic import Field, field_validator, model_validator

from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.products.models import Lang
from sve_carddb.domains.registry.records import CardId, FaceId, PrintingId

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


class Record(RecordData):
    subject: Subject
    value: Value
    # Batch-sampled links must not be shown as individually confirmed.
    review_level: Literal["sampled", "confirmed"]
    reason: Text

    @field_validator("reason")
    @classmethod
    def _reason(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Digital link reason must be nonblank")
        return value


class Envelope(RecordData):
    digital_link_authored_format: Literal[2]

    @field_validator("digital_link_authored_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Digital link format must be an integer")
        return value


class Index(Envelope):
    kind: Literal["digital_link_index"]
    includes: dict[str, Hash]


class Shard(Envelope):
    kind: Literal["digital_link_shard"]
    records: Annotated[tuple[Record, ...], Field(min_length=1)]

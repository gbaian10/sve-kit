"""Shared source, subject and owner types for current translation inputs."""

from typing import Annotated, Literal
from urllib.parse import urlsplit

from pydantic import Field, field_validator, model_validator

from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.domains.catalog.adoption_models import SourceRef


class Span(RecordData):
    start: Annotated[int, Field(ge=0)]
    end: Annotated[int, Field(gt=0)]


class SourceClaim(RecordData):
    source_work: Text | None = None
    source_urls: tuple[Text, ...] = ()
    claimed_source: Text | None = None

    @model_validator(mode="after")
    def _claim(self) -> SourceClaim:
        if any(
            value is not None and not value.strip()
            for value in (self.source_work, self.claimed_source)
        ):
            raise ValueError("Source claim text must be nonblank")
        if self.source_urls != tuple(sorted(set(self.source_urls))):
            raise ValueError("Source claim URLs must be sorted and unique")
        for url in self.source_urls:
            try:
                parts = urlsplit(url)
            except ValueError as error:
                raise ValueError("Source claim URL must be HTTP or HTTPS") from error
            if (
                parts.scheme not in {"http", "https"}
                or not parts.netloc
                or any(char.isspace() for char in url)
            ):
                raise ValueError("Source claim URL must be HTTP or HTTPS")
        return self


class AuthoredValue(RecordData):
    kind: Literal["authored"]
    text: Text


class SourceValue(RecordData):
    kind: Literal["source"]
    source_ref: SourceRef
    span: Span | None


class EffectTerm(RecordData):
    kind: Literal["effect_term"]
    jp_ref: SourceRef
    jp_span: Span
    target_ref: SourceRef
    target_span: Span
    concept_note: Text | None = None


class DictionaryEntry(RecordData):
    kind: Literal["dictionary_entry"]
    dictionary_kind: Literal["skill_names", "tribe_names"]
    entry_key: Text
    jp_ref: SourceRef
    target_ref: SourceRef
    concept_note: Text | None = None


class RevisionOwner(RecordData):
    kind: Literal["face_revision"]
    revision_id: Text


class PrintingOwner(RecordData):
    kind: Literal["printing_face"]
    printing_id: Text
    face_id: Text


class ConceptSubject(RecordData):
    card_id: Text
    face_id: Text
    source_lang: Literal["ja", "en"]
    source_hash: Hash


class Envelope(RecordData):
    format: Literal[2]

    @field_validator("format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Translation format must be an integer")
        return value


Origin = Literal["official", "project", "machine"]


class Quality(RecordData):
    origin: Origin = "project"
    low_confidence: bool = False
    note: str = ""

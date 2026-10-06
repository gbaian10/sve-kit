"""Editable current template values retain semantic identity without adoption receipts."""

from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.products.models import Code, Lang
from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.template_sources.normalizer import Role
from sve_carddb.template_translations.models import Definition, TemplateId


class Translation(RecordData):
    template_id: TemplateId
    lang: Lang
    text: Text


class DefinitionRecord(RecordData):
    record_key: Text
    kind: Literal["sentence_template"]
    data: Definition
    origin: Literal["official", "project", "machine"]
    low_confidence: bool
    note: str = ""


class TranslationRecord(RecordData):
    record_key: Text
    kind: Literal["template_translation"]
    data: Translation
    origin: Literal["official", "project", "machine"]
    low_confidence: bool
    note: str = ""


class Candidate(RecordData):
    source_kind: Literal["effect"]
    candidate_id: Text
    lang: Lang
    text: Text
    normalized_hash: Hash
    role: Role
    reasons: Annotated[
        tuple[Annotated[str, Field(pattern=r"^[a-z][a-z0-9_]*\Z")], ...],
        Field(min_length=1),
    ]

    @field_validator("text")
    @classmethod
    def _utf8(cls, value: str) -> str:
        value.encode("utf-8")
        return value

    @model_validator(mode="after")
    def _ordered(self) -> Self:
        if self.reasons != tuple(sorted(set(self.reasons))):
            raise ValueError("Candidate reasons must be sorted and unique")
        return self


class CandidateRecord(RecordData):
    record_key: Text
    kind: Literal["template_translation_candidate"]
    data: Candidate
    origin: Literal["project", "machine"]
    low_confidence: bool
    note: str = ""


class Variant(Translation):
    variant_key: Code

    @model_validator(mode="after")
    def _named(self) -> Self:
        if self.variant_key == "default":
            raise ValueError("Named template variant cannot be default")
        return self


class VariantRecord(RecordData):
    record_key: Text
    kind: Literal["template_translation_variant"]
    data: Variant
    origin: Literal["official", "project", "machine"]
    low_confidence: bool
    note: str = ""


Record = Annotated[
    DefinitionRecord | TranslationRecord | CandidateRecord | VariantRecord,
    Field(discriminator="kind"),
]


class Shard(RecordData):
    translation_authored_format: Literal[2]
    kind: Literal["translation_shard"]
    records: Annotated[tuple[Record, ...], Field(min_length=1)]

    @field_validator("translation_authored_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Current template format must be integer two")
        return value

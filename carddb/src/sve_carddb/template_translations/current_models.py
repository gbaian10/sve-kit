"""Editable current template values retain semantic identity without adoption receipts."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves constrained wire annotations

from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.catalog.adoption_models import Batch
from sve_carddb.products.models import Code, Lang
from sve_carddb.registry.records import RecordData, Text
from sve_carddb.template_sources.models import Entry
from sve_carddb.template_translations.flavor_models import FlavorEntry
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
    note: str


class TranslationRecord(RecordData):
    record_key: Text
    kind: Literal["template_translation"]
    data: Translation
    origin: Literal["official", "project", "machine"]
    low_confidence: bool
    note: str


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
    note: str


Record = Annotated[
    DefinitionRecord | TranslationRecord | VariantRecord, Field(discriminator="kind")
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


class Inventory(RecordData):
    template_source_format: Literal[3]
    kind: Literal["template_source_inventory"]
    source_batches: Annotated[tuple[Batch, ...], Field(min_length=1)]
    entries: tuple[Annotated[Entry | FlavorEntry, Field(discriminator="role")], ...]

    @field_validator("template_source_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Current inventory format must be integer three")
        return value

    @model_validator(mode="after")
    def _order(self) -> Self:
        keys = tuple((batch.store_id, batch.batch_id) for batch in self.source_batches)
        identifiers = tuple(entry.id for entry in self.entries)
        if keys != tuple(sorted(set(keys))) or identifiers != tuple(
            sorted(set(identifiers))
        ):
            raise ValueError(
                "Current inventory batches and entries must be sorted and unique"
            )
        if any(
            (entry.source_ref.store_id, entry.source_ref.batch_id) not in keys
            for entry in self.entries
        ):
            raise ValueError("Current inventory entry is outside its source batches")
        return self

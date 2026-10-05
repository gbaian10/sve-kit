"""Current vocabulary and language values; other catalog kinds retain format one."""

from typing import Annotated, Literal, Self

from pydantic import Field, model_validator

from sve_carddb.catalog.adoption_models import (
    Evidence,
    LanguageSubject,
    LanguageValue,
    VocabularySubject,
    VocabularyValue,
)
from sve_carddb.products.models import Lang
from sve_carddb.registry.records import RecordData, Text
from sve_carddb.snapshot.values import canonical
from sve_carddb.translations.current_models import Quality


class LabelTranslation(RecordData):
    lang: Lang
    text: Text
    origin: Literal["project", "machine"]
    low_confidence: bool


class CurrentVocabularyValue(VocabularyValue):
    translations: tuple[LabelTranslation, ...] = ()

    @model_validator(mode="after")
    def _translations(self) -> Self:
        """The label is the Japanese base; a translation never replaces it."""
        langs = [item.lang for item in self.translations]
        if len(set(langs)) != len(langs) or "ja" in langs:
            raise ValueError("Label translations must be unique and not Japanese")
        return self


class VocabularyData(RecordData):
    subject: VocabularySubject
    value: CurrentVocabularyValue | None
    evidence: tuple[Evidence, ...]


class LanguageData(RecordData):
    subject: LanguageSubject
    value: LanguageValue | None
    evidence: tuple[Evidence, ...]


class VocabularyRecord(Quality):
    kind: Literal["vocabulary_adoption"]
    data: VocabularyData

    @property
    def evidence(self) -> tuple[Evidence, ...]:
        """Reuse source checks without an adoption context or receipt adapter."""
        return self.data.evidence


class LanguageRecord(Quality):
    kind: Literal["language_adoption"]
    data: LanguageData

    @property
    def evidence(self) -> tuple[Evidence, ...]:
        """Keep evidence in the contractual data object."""
        return self.data.evidence


Record = Annotated[VocabularyRecord | LanguageRecord, Field(discriminator="kind")]


class Shard(RecordData):
    catalog_adoption_format: Literal[2]
    kind: Literal["catalog_adoption_shard"]
    records: Annotated[tuple[Record, ...], Field(min_length=1)]


def key(record: Record) -> str:
    """Subject identity excludes mutable labels and file sequence numbers."""
    return canonical(
        [record.kind, record.data.subject.model_dump(mode="json")]
    ).decode()

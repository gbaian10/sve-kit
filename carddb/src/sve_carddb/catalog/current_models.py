"""Current vocabulary and language values; other catalog kinds retain format one."""

from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.catalog.adoption_models import (
    Evidence,
    LanguageSubject,
    LanguageValue,
    VocabularySubject,
    VocabularyValue,
)
from sve_carddb.registry.records import RecordData
from sve_carddb.snapshot.values import canonical
from sve_carddb.translations.current_models import Quality


class VocabularyData(RecordData):
    subject: VocabularySubject
    value: VocabularyValue | None
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

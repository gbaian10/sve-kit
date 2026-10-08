"""Current catalog and display values without adoption histories."""

from typing import Annotated, Literal, Self

from pydantic import Field, model_validator

from sve_carddb.catalog.adoption_models import (
    AliasSubject,
    AliasValue,
    DefaultSubject,
    DefaultValue,
    Evidence,
    LanguageSubject,
    LanguageValue,
    NameSubject,
    NameValue,
    RouteSubject,
    RouteValue,
    SourceRef,
    SourceText,
    SymbolSubject,
    SymbolValue,
    TextEvidence,
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
    evidence: tuple[Evidence, ...] = ()


class LanguageData(RecordData):
    subject: LanguageSubject
    value: LanguageValue | None
    evidence: tuple[Evidence, ...] = ()


def _evidence(
    explicit: tuple[Evidence, ...], refs: tuple[SourceRef, ...]
) -> tuple[Evidence, ...]:
    unique = {canonical(item.model_dump(mode="json")): item for item in explicit}
    present = {
        canonical(item.source_ref.model_dump(mode="json"))
        for item in explicit
        if isinstance(item, TextEvidence)
    }
    for ref in refs:
        if canonical(ref.model_dump(mode="json")) not in present:
            item = TextEvidence(source_ref=ref, role="catalog value")
            unique[canonical(item.model_dump(mode="json"))] = item
    return tuple(unique[k] for k in sorted(unique))


class VocabularyRecord(Quality):
    kind: Literal["vocabulary_adoption"]
    data: VocabularyData

    @property
    def evidence(self) -> tuple[Evidence, ...]:
        """Each source referenced by a value is checked without duplicate authoring."""
        value = self.data.value
        refs = (
            ()
            if value is None
            else (
                *(
                    (value.label.source_ref,)
                    if isinstance(value.label, SourceText)
                    else ()
                ),
                *(mapping.source_ref for mapping in value.raw_mappings),
            )
        )
        return _evidence(self.data.evidence, refs)


class LanguageRecord(Quality):
    kind: Literal["language_adoption"]
    data: LanguageData

    @property
    def evidence(self) -> tuple[Evidence, ...]:
        """Retain additional sources that are not referenced by a value."""
        return self.data.evidence


class AliasData(RecordData):
    subject: AliasSubject
    value: AliasValue | None
    evidence: tuple[Evidence, ...] = ()


class AliasRecord(Quality):
    kind: Literal["search_alias_adoption"]
    data: AliasData


class SymbolData(RecordData):
    subject: SymbolSubject
    value: SymbolValue | None
    evidence: tuple[Evidence, ...] = ()


class SymbolRecord(Quality):
    kind: Literal["text_symbol_adoption"]
    data: SymbolData


class NameData(RecordData):
    subject: NameSubject
    value: NameValue | None
    evidence: tuple[Evidence, ...] = ()


class NameRecord(Quality):
    kind: Literal["rules_name_adoption"]
    data: NameData


class RouteData(RecordData):
    subject: RouteSubject
    value: RouteValue | None
    evidence: tuple[Evidence, ...] = ()


class RouteRecord(Quality):
    kind: Literal["route_override_adoption"]
    data: RouteData


class DefaultData(RecordData):
    subject: DefaultSubject
    value: DefaultValue | None
    evidence: tuple[Evidence, ...] = ()


class DefaultRecord(Quality):
    kind: Literal["default_printing_adoption"]
    data: DefaultData


Record = Annotated[
    VocabularyRecord
    | LanguageRecord
    | AliasRecord
    | SymbolRecord
    | NameRecord
    | RouteRecord
    | DefaultRecord,
    Field(discriminator="kind"),
]


class Shard(RecordData):
    catalog_adoption_format: Literal[2]
    kind: Literal["catalog_adoption_shard"]
    records: Annotated[tuple[Record, ...], Field(min_length=1)]


class DisplayShard(RecordData):
    display_override_format: Literal[2]
    kind: Literal["display_override_shard"]
    records: Annotated[tuple[Record, ...], Field(min_length=1)]


def key(record: Record) -> str:
    """Subject identity excludes mutable labels and file sequence numbers."""
    return canonical(
        [record.kind, record.data.subject.model_dump(mode="json")]
    ).decode()

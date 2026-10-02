"""Translation-contract §2/§5 wire types; generated sentences remain outside authored."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves annotations at runtime

from typing import Annotated, Literal

from pydantic import Field, field_validator, model_validator

from sve_carddb.catalog.adoption_models import Predecessor, SourceRef, TextEvidence
from sve_carddb.products.models import Instant, Lang
from sve_carddb.registry.records import Hash, RecordData, Text

Category = Literal["keyword", "ability", "trait", "rule_term", "card_name"]
Origin = Literal["official_svwb", "official_sv1", "project", "community", "machine"]


class Span(RecordData):
    start: Annotated[int, Field(ge=0)]
    end: Annotated[int, Field(gt=0)]


class TermData(RecordData):
    id: Text
    category: Category
    concept_key: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")]
    source_ref: SourceRef
    source_span: Span | None


class AuthoredValue(RecordData):
    kind: Literal["authored"]
    text: Text


class SourceValue(RecordData):
    kind: Literal["source"]
    source_ref: SourceRef
    span: Span | None


class DigitalName(RecordData):
    kind: Literal["digital_name"]
    digital_face_id: Text
    sve_owner: Text
    jp_ref: SourceRef
    target_ref: SourceRef
    decision_id: Text


class EffectTerm(RecordData):
    kind: Literal["effect_term"]
    jp_ref: SourceRef
    jp_span: Span
    target_ref: SourceRef
    target_span: Span
    concept_note: Text


class DictionaryEntry(RecordData):
    kind: Literal["dictionary_entry"]
    dictionary_kind: Literal["skill_names", "tribe_names"]
    entry_key: Text
    jp_ref: SourceRef
    target_ref: SourceRef
    concept_note: Text


ConceptEvidence = Annotated[
    DigitalName | EffectTerm | DictionaryEntry, Field(discriminator="kind")
]


class ChoiceData(RecordData):
    term_id: Text
    lang: Lang
    value: Annotated[AuthoredValue | SourceValue, Field(discriminator="kind")] | None
    origin: Origin
    concept_evidence: tuple[ConceptEvidence, ...]
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None


class VocabularyData(RecordData):
    vocabulary_kind: Literal["class", "type"]
    vocabulary_code: Text
    lang: Lang
    value: Annotated[AuthoredValue | SourceValue, Field(discriminator="kind")] | None
    origin: Origin
    concept_evidence: tuple[ConceptEvidence, ...]
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None


class TermRecord(RecordData):
    record_key: Text
    kind: Literal["glossary_term"]
    filing_key: Text
    data: TermData
    evidence: tuple[TextEvidence, ...]


class ChoiceRecord(RecordData):
    record_key: Text
    kind: Literal["glossary_choice"]
    filing_key: Text
    data: ChoiceData
    evidence: tuple[TextEvidence, ...]


class VocabularyRecord(RecordData):
    record_key: Text
    kind: Literal["vocabulary_choice"]
    filing_key: Text
    data: VocabularyData
    evidence: tuple[TextEvidence, ...]


Record = Annotated[
    TermRecord | ChoiceRecord | VocabularyRecord, Field(discriminator="kind")
]


class Decision(RecordData):
    id: Annotated[str, Field(pattern=r"^d:[0-9a-f]{64}\Z")]
    scope: Literal["batch"]
    membership_hash: Hash
    members: tuple[tuple[Text, Hash], ...]
    sample_ids: tuple[Text, ...]
    authored_by: Text
    authored_at: Instant
    reviewed_by: Text
    reviewed_at: Instant
    reviewed_precision: Literal["day", "instant"]
    note: str
    state: Literal["sampled", "confirmed"]
    category: Literal["glossary_term", "glossary_choice", "vocabulary_choice"]
    policy_id: Text

    @model_validator(mode="after")
    def _review(self) -> Decision:
        if not self.authored_by.strip() or not self.reviewed_by.strip():
            raise ValueError("Adopted glossary requires author and human reviewer")
        if self.reviewed_precision == "day" and not self.reviewed_at.endswith(
            "T00:00:00Z"
        ):
            raise ValueError("Day precision must use UTC midnight encoding")
        return self


class Envelope(RecordData):
    translation_authored_format: Literal[1]

    @field_validator("translation_authored_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Translation format must be integer one")
        return value


class Shard(Envelope):
    translation_authored_format: Literal[1]
    kind: Literal["translation_shard"]
    default_decision_id: Text
    records: Annotated[tuple[Record, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]


class Index(Envelope):
    translation_authored_format: Literal[1]
    kind: Literal["translation_index"]
    includes: dict[str, Hash]
    inventories: dict[str, Hash]

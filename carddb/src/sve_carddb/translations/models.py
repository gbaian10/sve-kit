"""Translation-contract §2/§5 wire types; generated sentences remain outside authored."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves annotations at runtime

from typing import Annotated, Literal
from urllib.parse import urlsplit

from pydantic import Field, field_validator, model_validator

from sve_carddb.build_inputs import Revision
from sve_carddb.catalog.adoption_models import Predecessor, SourceRef, TextEvidence
from sve_carddb.products.models import Code, Date, Instant, Lang
from sve_carddb.registry.records import Hash, RecordData, Text

Category = Literal["keyword", "ability", "trait", "rule_term", "card_name"]
Origin = Literal["official_svwb", "official_sv1", "project", "community", "machine"]


class Span(RecordData):
    start: Annotated[int, Field(ge=0)]
    end: Annotated[int, Field(gt=0)]


class Delegation(RecordData):
    authorized_by: Text
    authorization_basis: Text
    authorization_date: Date
    scope: Annotated[tuple[Text, ...], Field(min_length=1)]
    decided_by: Text
    decided_at: Instant
    decided_precision: Literal["day", "instant"]
    decision_basis: Text

    @model_validator(mode="after")
    def _receipt(self) -> Delegation:
        if any(
            not value.strip()
            for value in (
                self.authorized_by,
                self.authorization_basis,
                self.decided_by,
                self.decision_basis,
            )
        ):
            raise ValueError("Delegation receipt text must be nonblank")
        if self.scope != tuple(sorted(set(self.scope))):
            raise ValueError("Delegation scope must be sorted and unique")
        if self.decided_precision == "day" and not self.decided_at.endswith(
            "T00:00:00Z"
        ):
            raise ValueError("Delegation day precision must use UTC midnight")
        return self


class AdoptionReview(RecordData):
    mode: Literal["human", "delegated_glossary"]
    delegation: Delegation | None

    @model_validator(mode="after")
    def _mode(self) -> AdoptionReview:
        if (self.mode == "human") != (self.delegation is None):
            raise ValueError("Glossary review mode and delegation disagree")
        return self


class SourceClaim(RecordData):
    source_work: Text | None
    source_urls: tuple[Text, ...]
    claimed_source: Text | None = None
    note: Text

    @model_validator(mode="after")
    def _claim(self) -> SourceClaim:
        if any(
            value is not None and not value.strip()
            for value in (self.source_work, self.claimed_source, self.note)
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


class TermData(RecordData):
    id: Text
    category: Category
    concept_key: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")]
    source_ref: SourceRef | None
    source_span: Span | None
    authored_source_ja: Text | None
    missing_source_reason: Text | None
    adoption_review: AdoptionReview

    @model_validator(mode="after")
    def _source_mode(self) -> TermData:
        if self.source_ref is not None:
            if (
                self.authored_source_ja is not None
                or self.missing_source_reason is not None
            ):
                raise ValueError(
                    "Frozen glossary source forbids authored source metadata"
                )
        elif (
            self.source_span is not None
            or self.authored_source_ja is None
            or self.missing_source_reason is None
        ):
            raise ValueError(
                "Authored glossary source requires name and reason without span"
            )
        elif (
            not self.authored_source_ja.strip()
            or not self.missing_source_reason.strip()
        ):
            raise ValueError(
                "Authored glossary source name and reason must be nonblank"
            )
        return self


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
    source_claim: SourceClaim | None
    adoption_review: AdoptionReview
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None


class VocabularyData(RecordData):
    vocabulary_kind: Literal["class", "type"]
    vocabulary_code: Text
    lang: Lang
    value: Annotated[AuthoredValue | SourceValue, Field(discriminator="kind")] | None
    origin: Origin
    concept_evidence: tuple[ConceptEvidence, ...]
    source_claim: SourceClaim | None
    adoption_review: AdoptionReview
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


class EmphasisData(RecordData):
    term_id: Text
    value: bool | None
    adoption_review: AdoptionReview
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None


class EmphasisRecord(RecordData):
    record_key: Text
    kind: Literal["glossary_emphasis_choice"]
    filing_key: Text
    data: EmphasisData
    evidence: tuple[TextEvidence, ...]


class RevisionOwner(RecordData):
    kind: Literal["face_revision"]
    revision_id: Text


class PrintingOwner(RecordData):
    kind: Literal["printing_face"]
    printing_id: Text
    face_id: Text


Owner = Annotated[RevisionOwner | PrintingOwner, Field(discriminator="kind")]


class IdentityBasis(RecordData):
    authored_revision: Revision
    registry_index_hash: Hash
    transition_index_hash: Hash | None


class AssignmentData(RecordData):
    owner: Owner
    field: Literal["name"]
    ordinal: None
    source_hash: Hash
    variant: Code
    concept_key: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")]
    identity_basis: IdentityBasis
    reason: Text
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None

    @field_validator("reason")
    @classmethod
    def _reason(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Name assignment reason must be nonblank")
        return value


class ConceptSubject(RecordData):
    card_id: Text
    face_id: Text
    source_lang: Literal["ja", "en"]
    source_hash: Hash


class ConceptData(RecordData):
    subject: ConceptSubject
    term_id: Text | None
    source_ref: SourceRef
    identity_basis: IdentityBasis
    reason: Text
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None

    @model_validator(mode="after")
    def _subject(self) -> ConceptData:
        if self.source_ref.text_hash != self.subject.source_hash:
            raise ValueError("Name concept subject and frozen name hash disagree")
        if not self.reason.strip():
            raise ValueError("Name concept reason must be nonblank")
        if self.adoption_no == 1 and self.term_id is None:
            raise ValueError("Initial name concept cannot be withdrawn")
        return self


class AssignmentRecord(RecordData):
    record_key: Text
    kind: Literal["context_assignment"]
    filing_key: Text
    data: AssignmentData
    evidence: tuple[TextEvidence, ...]


class ConceptRecord(RecordData):
    record_key: Text
    kind: Literal["card_name_concept"]
    filing_key: Text
    data: ConceptData
    evidence: tuple[TextEvidence, ...]


Record = Annotated[
    TermRecord
    | ChoiceRecord
    | VocabularyRecord
    | EmphasisRecord
    | AssignmentRecord
    | ConceptRecord,
    Field(discriminator="kind"),
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
    category: Literal[
        "glossary_term",
        "glossary_choice",
        "vocabulary_choice",
        "glossary_emphasis_choice",
        "context_assignment",
        "card_name_concept",
    ]
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
    translation_authored_format: Literal[1, 2]

    @field_validator("translation_authored_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Translation format must be an integer")
        return value


class Shard(Envelope):
    translation_authored_format: Literal[1]
    kind: Literal["translation_shard"]
    default_decision_id: Text
    records: Annotated[tuple[Record, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]


class Index(Envelope):
    translation_authored_format: Literal[1, 2]
    kind: Literal["translation_index"]
    includes: dict[str, Hash]
    inventories: dict[str, Hash]

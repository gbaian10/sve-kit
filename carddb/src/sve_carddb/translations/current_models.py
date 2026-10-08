"""Current translation values; source and semantic keys are independent of review."""

from typing import Annotated, Literal

from pydantic import Field, computed_field, field_validator, model_validator

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.products.models import Code, Lang
from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.snapshot.values import canonical
from sve_carddb.translations.models import (
    AuthoredValue,
    ConceptSubject,
    DictionaryEntry,
    EffectTerm,
    PrintingOwner,
    RevisionOwner,
    SourceClaim,
    SourceValue,
    Span,
)

Category = Literal["keyword", "ability", "trait", "rule_term", "card_name"]
Origin = Literal["official", "project", "machine"]


class TermData(RecordData):
    id: Text
    category: Category
    concept_key: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")]
    source_ref: SourceRef | None
    source_span: Span | None
    authored_source_ja: Text | None
    missing_source_reason: Text | None = None

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


class DigitalName(RecordData):
    kind: Literal["digital_name"]
    digital_face_id: Text
    sve_owner: Text
    jp_ref: SourceRef
    target_ref: SourceRef


ConceptEvidence = Annotated[
    DigitalName | EffectTerm | DictionaryEntry, Field(discriminator="kind")
]


class ChoiceData(RecordData):
    term_id: Text
    lang: Lang
    value: Annotated[AuthoredValue | SourceValue, Field(discriminator="kind")] | None
    concept_evidence: tuple[ConceptEvidence, ...]
    source_claim: SourceClaim | None = None


class EmphasisData(RecordData):
    term_id: Text
    value: bool | None


Owner = Annotated[RevisionOwner | PrintingOwner, Field(discriminator="kind")]


class AssignmentData(RecordData):
    owner: Owner
    field: Literal["name"]
    ordinal: None
    source_hash: Hash
    variant: Code
    concept_key: Annotated[str, Field(pattern=r"^[a-z][a-z0-9_.-]*\Z")] | None
    reason: Text

    @field_validator("reason")
    @classmethod
    def _reason(cls, value: str) -> str:
        if not value.strip():
            raise ValueError("Name assignment reason must be nonblank")
        return value


class ConceptData(RecordData):
    subject: ConceptSubject
    term_id: Text | None
    source_ref: SourceRef
    reason: Text

    @model_validator(mode="after")
    def _subject(self) -> ConceptData:
        if self.source_ref.text_hash != self.subject.source_hash:
            raise ValueError("Name concept subject and frozen name hash disagree")
        if not self.reason.strip():
            raise ValueError("Name concept reason must be nonblank")
        return self


class Quality(RecordData):
    origin: Origin = "project"
    low_confidence: bool = False
    note: str = ""


class TermRecord(Quality):
    kind: Literal["glossary_term"]
    data: TermData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical([self.kind, self.data.id]).decode()


class ChoiceRecord(Quality):
    kind: Literal["glossary_choice"]
    data: ChoiceData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical([self.kind, self.data.term_id, self.data.lang]).decode()


class EmphasisRecord(Quality):
    kind: Literal["glossary_emphasis_choice"]
    data: EmphasisData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical([self.kind, self.data.term_id]).decode()


class AssignmentRecord(Quality):
    kind: Literal["context_assignment"]
    data: AssignmentData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical(
            [
                self.kind,
                self.data.owner.model_dump(mode="json"),
                self.data.field,
                self.data.ordinal,
            ]
        ).decode()


class ConceptRecord(Quality):
    kind: Literal["card_name_concept"]
    data: ConceptData

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical(
            [self.kind, self.data.subject.model_dump(mode="json")]
        ).decode()


Record = Annotated[
    TermRecord | ChoiceRecord | EmphasisRecord | AssignmentRecord | ConceptRecord,
    Field(discriminator="kind"),
]


class Shard(RecordData):
    translation_authored_format: Literal[2]
    kind: Literal["translation_shard"]
    records: Annotated[tuple[Record, ...], Field(min_length=1)]

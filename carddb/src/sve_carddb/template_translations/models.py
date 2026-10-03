"""Closed template definitions and adopted translations, separate from private candidates."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves constrained wire annotations

from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.build_inputs import Revision
from sve_carddb.catalog.adoption_models import TextEvidence
from sve_carddb.products.models import Code, Lang
from sve_carddb.registry.records import Hash, Instant, RecordData, Text
from sve_carddb.template_parameters.models import Schema, SourceSpan
from sve_carddb.template_sources.models import Entry, Recipe
from sve_carddb.template_translations.review import ModelReview
from sve_carddb.translations.models import Envelope

TemplateId = Annotated[
    str, Field(pattern=r"^[TC](?:[0-9a-f]{10}|(?:[0-9a-f]{2}){8,32})\Z")
]


class Definition(RecordData):
    id: TemplateId
    inventory_id: Text
    source_span: SourceSpan
    source_lang: Literal["ja", "en"]
    normalizer_version: Code
    semantic_variant: Code
    parameter_schema: Schema
    content_hash: Hash
    supersedes_id: TemplateId | None


class PolicyPin(RecordData):
    policy_id: Code
    authored_revision: Revision
    path: Text
    hash: Hash
    approval_receipt_hash: Hash


class SampleDecision(RecordData):
    decision_id: Annotated[str, Field(pattern=r"^d:[0-9a-f]{64}\Z")]
    membership_hash: Hash


class AdoptionReview(RecordData):
    mode: Literal["human", "approved_policy"]
    policy: PolicyPin | None
    initial_sample_decisions: tuple[SampleDecision, ...]

    @model_validator(mode="after")
    def _mode(self) -> Self:
        if self.mode == "human":
            if self.policy is not None or self.initial_sample_decisions:
                raise ValueError("Human template review cannot borrow policy samples")
        elif self.policy is None or not self.initial_sample_decisions:
            raise ValueError(
                "Policy template review requires a pin and initial samples"
            )
        keys = tuple(s.decision_id for s in self.initial_sample_decisions)
        if keys != tuple(sorted(set(keys))):
            raise ValueError(
                "Template initial sample decisions must be sorted and unique"
            )
        return self


class Translation(RecordData):
    template_id: TemplateId
    lang: Lang
    revision: Annotated[int, Field(ge=1, le=9007199254740991)]
    text: Text
    origin: Literal["project", "machine"]
    model_review: ModelReview | None
    adoption_review: AdoptionReview

    @model_validator(mode="after")
    def _origin(self) -> Self:
        if (self.origin == "machine") != (self.model_review is not None):
            raise ValueError(
                "Template machine origin requires model review exclusively"
            )
        return self


class DefinitionRecord(RecordData):
    record_key: Text
    kind: Literal["sentence_template"]
    filing_key: Text
    data: Definition
    evidence: tuple[TextEvidence, ...]


class TranslationRecord(RecordData):
    record_key: Text
    kind: Literal["template_translation"]
    filing_key: Text
    data: Translation
    evidence: tuple[TextEvidence, ...]


Record = Annotated[DefinitionRecord | TranslationRecord, Field(discriminator="kind")]


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
    category: Literal["sentence_template", "template_translation"]
    policy_id: Text

    @model_validator(mode="after")
    def _human(self) -> Self:
        if not self.authored_by.strip() or not self.reviewed_by.strip():
            raise ValueError("Adopted templates require author and human reviewer")
        if self.reviewed_precision == "day" and not self.reviewed_at.endswith(
            "T00:00:00Z"
        ):
            raise ValueError("Template day precision must use UTC midnight encoding")
        return self


class Shard(Envelope):
    kind: Literal["translation_shard"]
    default_decision_id: Text
    records: Annotated[tuple[Record, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]


class Inventory(RecordData):
    template_source_format: Literal[1]
    kind: Literal["template_source_inventory"]
    recipes: Annotated[tuple[Recipe, ...], Field(min_length=1)]
    entries: tuple[Entry, ...]

    @field_validator("template_source_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Template inventory format must be integer one")
        return value

    @model_validator(mode="after")
    def _order(self) -> Self:
        for keys in (
            tuple(r.id for r in self.recipes),
            tuple(e.id for e in self.entries),
        ):
            if keys != tuple(sorted(set(keys))):
                raise ValueError(
                    "Template recipes and entries must be sorted and unique"
                )
        return self

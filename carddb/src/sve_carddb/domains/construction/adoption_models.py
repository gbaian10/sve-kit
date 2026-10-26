"""Human-confirmed construction envelopes for current-list launch adoption."""

from typing import Annotated, Literal

from pydantic import Field, field_validator, model_validator

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Date, Hash, Instant, RecordData, Text, UInt
from sve_carddb.core.provenance import (
    BuildContext,
    DateSourceUse,
    FilePin,
    Revision,
    SourceUse,
)
from sve_carddb.core.regions import Region
from sve_carddb.domains.construction.models import Member
from sve_carddb.domains.products.models import LocalizedText


def ordered(values: tuple[RecordData, ...]) -> None:
    """Require authored sets to have one canonical representation."""
    keys = [canonical(value.model_dump(mode="json")) for value in values]
    if keys != sorted(set(keys)):
        raise ValueError("Construction arrays must be sorted and unique")


class BatchPin(RecordData):
    batch_id: Hash


class ReviewContext(RecordData):
    context: BuildContext
    source_batches: Annotated[tuple[BatchPin, ...], Field(min_length=1)]


class Dependency(RecordData):
    kind: Literal["profile", "rules_name", "card"]
    id: Text
    region: Region
    basis_hash: Hash


class Predecessor(RecordData):
    record_key: Text
    record_hash: Hash
    decision_id: Text


class Data(RecordData):
    adoption_no: Annotated[UInt, Field(ge=1)]
    predecessor: Predecessor | None
    review_context_hash: Hash
    dependencies: tuple[Dependency, ...]
    reason: Text


class ProfileSubject(RecordData):
    profile_id: Text


class RestrictionSubject(RecordData):
    restriction_id: Text


class CoverageSubject(ProfileSubject):
    from_date: Date


class ProfileValue(RecordData):
    region: Region
    format_code: Literal["standard"]
    name: LocalizedText


class RestrictionValue(RecordData):
    profile_id: Text
    announced_on: Date | None
    effective_from: Date
    effective_until: Date | None
    kind: Literal["copy_limit"]
    state: Literal["confirmed"]
    max_copies: Annotated[UInt, Field(le=1)]
    max_selected_groups: None
    members: Annotated[tuple[Member, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def current_copy_limit(self) -> RestrictionValue:
        """Keep the launch scope limited to the approved main copy limits."""
        if (
            self.effective_until is not None
            and self.effective_until <= self.effective_from
        ):
            raise ValueError("Restriction interval must be nonempty")
        ordered(self.members)
        if any(
            member.choice_option != 0 or member.deck_scope != "main"
            for member in self.members
        ):
            raise ValueError("Current-list launch supports main copy limits only")
        return self


class SourceSet(RecordData):
    baseline: Annotated[tuple[Text, ...], Field(min_length=1, max_length=1)]
    notices: tuple[Text, ...]
    discovery: tuple[Text, ...]
    excluded: tuple[Text, ...]

    @model_validator(mode="after")
    def list_only(self) -> SourceSet:
        """Separate list observations from a news discovery completeness claim."""
        if self.notices or self.discovery or self.excluded:
            raise ValueError("Current-list coverage cannot claim news discovery")
        return self


class CurrentListReconciliation(RecordData):
    mode: Literal["current-list-day-v1", "observed-list-day-v1"]
    observed_on: Date
    observed_timezone: Text
    projection_hash: Hash
    excluded_entry_pointers: tuple[Text, ...]


class CoverageValue(RecordData):
    until_date: Date
    as_of: Date
    state: Literal["complete"]
    source_set: SourceSet
    reconciliation: CurrentListReconciliation


class ProfileData(Data):
    subject: ProfileSubject
    value: ProfileValue | None


class RestrictionData(Data):
    subject: RestrictionSubject
    value: RestrictionValue | None


class CoverageData(Data):
    subject: CoverageSubject
    value: CoverageValue | None


class Evidence(RecordData):
    role: Literal["profile", "restriction", "coverage", "reconciliation"]
    primary: bool
    use: SourceUse | DateSourceUse


class ModelReview(RecordData):
    vendor: Literal["anthropic", "openai"]
    model: Text
    version: Text
    reviewed_at: Instant
    basis_hash: Hash
    outcome: Literal["agreed", "disputed"]
    report_hash: Hash


class MaintainerEvent(RecordData):
    reviewed_by: Text
    reviewed_at: Instant
    reviewed_precision: Literal["day", "instant"]
    basis_hash: Hash
    outcome: Literal["adopted", "declined", "deferred"]
    note: Text

    @model_validator(mode="after")
    def day_encoding(self) -> MaintainerEvent:
        """Preserve day precision without inventing a human event time."""
        if self.reviewed_precision == "day" and not self.reviewed_at.endswith(
            "T00:00:00Z"
        ):
            raise ValueError("Day event must use its explicit UTC day encoding")
        return self


class Review(RecordData):
    basis_hash: Hash
    model_reviews: Annotated[tuple[ModelReview, ...], Field(min_length=2, max_length=2)]
    maintainer_resolution: MaintainerEvent | None
    maintainer_samples: tuple[MaintainerEvent, ...]


class AdoptionReview(RecordData):
    mode: Literal["human"]
    policy: None
    initial_sample_decisions: tuple[Text, ...]

    @model_validator(mode="after")
    def no_policy(self) -> AdoptionReview:
        """Keep first-round human confirmation independent of future policy."""
        if self.initial_sample_decisions:
            raise ValueError("Human adoption cannot borrow policy authorization")
        return self


class BaseRecord(RecordData):
    record_key: Text
    filing_key: Literal["jp-standard", "en-standard"]
    evidence: Annotated[tuple[Evidence, ...], Field(min_length=1)]
    review: Review
    adoption_review: AdoptionReview


class ProfileRecord(BaseRecord):
    kind: Literal["construction_profile"]
    data: ProfileData


class RestrictionRecord(BaseRecord):
    kind: Literal["construction_restriction"]
    data: RestrictionData


class CoverageRecord(BaseRecord):
    kind: Literal["construction_coverage"]
    data: CoverageData


Record = Annotated[
    ProfileRecord | RestrictionRecord | CoverageRecord, Field(discriminator="kind")
]


def basis(record: Record) -> str:
    """Exclude review metadata so two vendors can sign the same final value."""
    return digest(
        canonical(record.model_dump(mode="json", exclude={"review", "adoption_review"}))
    )


class Decision(RecordData):
    id: Text
    state: Literal["confirmed"]
    scope: Literal["batch"]
    category: Literal[
        "construction_profile", "construction_restriction", "construction_coverage"
    ]
    members: tuple[tuple[Text, Hash], ...]
    membership_hash: Hash
    policy_id: Literal["construction-human-v1"]
    sample_ids: tuple[Text, ...]
    confidence: Literal["high", "medium", "low"] | None
    note: Text


class Shard(RecordData):
    construction_adoption_format: Literal[1]
    kind: Literal["construction_adoption_shard"]
    review_context: ReviewContext
    default_decision_id: Text
    records: Annotated[tuple[Record, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]

    @field_validator("construction_adoption_format", mode="before")
    @classmethod
    def integer_format(cls, value: object) -> object:
        """Reject booleans before Literal validation can coerce them."""
        if type(value) is not int:
            raise ValueError("Construction format must be integer one")
        return value


class Index(RecordData):
    construction_adoption_format: Literal[1]
    kind: Literal["construction_adoption_index"]
    includes: dict[str, Hash]

    @field_validator("construction_adoption_format", mode="before")
    @classmethod
    def integer_format(cls, value: object) -> object:
        """Reject booleans before Literal validation can coerce them."""
        if type(value) is not int:
            raise ValueError("Construction format must be integer one")
        return value


class ReviewReport(RecordData):
    construction_review_report_format: Literal[1]
    vendor: Literal["anthropic", "openai"]
    model: Text
    version: Text
    reviewed_at: Instant
    basis_hash: Hash
    outcome: Literal["agreed", "disputed"]
    source_uses: tuple[SourceUse | DateSourceUse, ...]
    program_revision: Revision
    dependencies: tuple[FilePin, ...]
    note: Text

    @field_validator("construction_review_report_format", mode="before")
    @classmethod
    def integer_format(cls, value: object) -> object:
        """Reject booleans before Literal validation can coerce them."""
        if type(value) is not int:
            raise ValueError("Review report format must be integer one")
        return value

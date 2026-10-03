"""Closed YAML envelopes preserve historical answers rather than fabricating clicks."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves nested constrained types

from datetime import datetime
from typing import Annotated, Literal, Self

from pydantic import Field, JsonValue, field_validator, model_validator

from sve_carddb.build_inputs import Revision
from sve_carddb.registry.records import Hash, Instant, RecordData, Text
from sve_carddb.template_parameters.models import Range
from sve_carddb.template_sources.normalizer import Role

Code = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_-]*\Z")]
RuleId = Annotated[str, Field(pattern=r"^[a-z][a-z0-9_]*\Z")]

LEGACY_IDS = (
    "prefix_field_attack",
    "prefix_field_cost",
    "prefix_field_health",
    "suffix_unit_cards",
    "suffix_unit_entities",
    "suffix_unit_pp",
    "suffix_unit_times",
    "suffix_unit_turns",
)
RESTRICTION = "reminder_fullwidth_sign_exclusion"
PRECEDENCE = (
    "validate_source",
    "existing_numeric_rules",
    "unowned_pending_only",
    "recognition_rules",
    "reject_conflicts",
)


def ordered(keys: tuple[str, ...]) -> None:
    """Never interpret duplicates or serialization order as additional authorization."""
    if keys != tuple(sorted(set(keys))):
        raise ValueError("Recognition items must be sorted and unique")


class Batch(RecordData):
    store_id: Code
    batch_id: Hash


class Scope(RecordData):
    region: Literal["jp"]
    roles: Annotated[tuple[Literal["body", "reminder"], ...], Field(min_length=1)]
    source_batches: Annotated[tuple[Batch, ...], Field(min_length=1, max_length=1)]
    parser_ids: Annotated[tuple[Code, ...], Field(min_length=1)]
    normalizer_ids: Annotated[tuple[Code, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _order(self) -> Self:
        for values in (self.roles, self.parser_ids, self.normalizer_ids):
            ordered(values)
        return self


class RoleRange(Range):
    role: Role


class Reference(RecordData):
    source_name: Text
    id: Text
    category: Text
    record_hash: Hash


class CaseInput(RecordData):
    region: Literal["jp", "en"]
    role: Role
    field_text: Text
    role_spans: Annotated[tuple[RoleRange, ...], Field(min_length=1)]
    slot_segments: Annotated[tuple[Range, ...], Field(min_length=1)]
    original_issues: tuple[Code, ...]
    existing_numeric_rule: RuleId | None
    reference_candidates: tuple[Reference, ...]

    @model_validator(mode="after")
    def _spans(self) -> Self:
        ordered(self.original_issues)
        position = 0
        for span in self.role_spans:
            if span.start != position or span.end > len(self.field_text):
                raise ValueError("Recognition case roles must partition the full field")
            position = span.end
        if position != len(self.field_text):
            raise ValueError("Recognition case roles must partition the full field")
        previous = 0
        for selected in self.slot_segments:
            if selected.start < previous or selected.end > len(self.field_text):
                raise ValueError(
                    "Recognition case slots must be sorted inside the field"
                )
            if not any(
                role.role == self.role
                and role.start <= selected.start < selected.end <= role.end
                for role in self.role_spans
            ):
                raise ValueError("Recognition case slot disagrees with its role")
            previous = selected.end
        return self


class Example(RecordData):
    case_id: Code
    input: CaseInput
    expected_match: bool


class Examples(RecordData):
    positive: Annotated[tuple[Example, ...], Field(min_length=1)]
    negative: Annotated[tuple[Example, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _answers(self) -> Self:
        all_cases = self.positive + self.negative
        if len({case.case_id for case in all_cases}) != len(all_cases):
            raise ValueError("Recognition fixed case IDs must be unique")
        if any(not c.expected_match for c in self.positive) or any(
            c.expected_match for c in self.negative
        ):
            raise ValueError("Recognition fixed case answers disagree with their group")
        return self


class Rule(RecordData):
    rule_id: RuleId
    matcher_version: Text
    matcher_commit: Revision
    recognized_role: Code
    match_conditions: dict[str, JsonValue]
    condition_hash: Hash
    finite_transform: Literal["preserve_source"]
    examples: Examples


class Envelope(RecordData):
    @field_validator(
        "parameter_rule_policy_format",
        "parameter_rule_approval_format",
        mode="before",
        check_fields=False,
    )
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Recognition envelope format must be integer one")
        return value


class Policy(Envelope):
    parameter_rule_policy_format: Literal[1]
    kind: Literal["template_parameter_rule_policy"]
    policy_id: Code
    scope: Scope
    precedence: tuple[Code, ...]
    rules: Annotated[tuple[Rule, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _rules(self) -> Self:
        ordered(tuple(rule.rule_id for rule in self.rules))
        if self.precedence != PRECEDENCE:
            raise ValueError("Recognition policy precedence cannot change")
        return self


class Identity(RecordData):
    rule_id: RuleId
    matcher_version: Text
    condition_hash: Hash | None
    matcher_commit: Revision | None

    @model_validator(mode="after")
    def _historical(self) -> Self:
        if (self.condition_hash is None) != (self.matcher_commit is None):
            raise ValueError("Recognition historical identity cannot be half null")
        if self.condition_hash is None and (
            self.rule_id not in LEGACY_IDS
            or self.matcher_version != "numeric-rule-proposals-v2:" + self.rule_id
        ):
            raise ValueError(
                "Recognition null identity is only supported for the old eight"
            )
        return self


class Authorization(RecordData):
    event_locator: Text
    source_locator: Text | None
    evidence_hash: Hash
    statement: Text


class Disclosure(RecordData):
    id: Code
    text: Text
    delivery: Literal["page", "conversation", "page_and_conversation"]
    evidence_hash: Hash


class Presentation(RecordData):
    page_hash: Hash | None
    page_payload_hash: Hash | None
    presented_rules: Annotated[tuple[Identity, ...], Field(min_length=1)]
    disclosures: tuple[Disclosure, ...]

    @model_validator(mode="after")
    def _presentation(self) -> Self:
        ordered(tuple(item.rule_id for item in self.presented_rules))
        ordered(tuple(item.id for item in self.disclosures))
        if self.page_hash is None and any(
            item.condition_hash is None for item in self.presented_rules
        ):
            raise ValueError("Recognition old identity requires the original page hash")
        return self


class Event(RecordData):
    form: Literal["conversation_bulk", "page_bulk", "page_rule"]
    reviewed_by: Text
    reviewed_at: Instant
    reviewed_precision: Literal["instant", "day"]
    authorization_basis: Authorization
    presentation: Presentation
    authorized_rules: Annotated[tuple[Identity, ...], Field(min_length=1)]
    note: Text

    @model_validator(mode="after")
    def _event(self) -> Self:
        datetime.fromisoformat(self.reviewed_at)
        ordered(tuple(item.rule_id for item in self.authorized_rules))
        if self.form == "page_rule" and len(self.authorized_rules) != 1:
            raise ValueError(
                "Recognition per-rule click cannot authorize multiple rules"
            )
        if self.reviewed_precision == "day" and not self.reviewed_at.endswith(
            "T00:00:00Z"
        ):
            raise ValueError("Recognition day event requires UTC midnight")
        if any(
            item not in self.presentation.presented_rules
            for item in self.authorized_rules
        ):
            raise ValueError(
                "Recognition authorization must be part of the original presentation"
            )
        if any(item.condition_hash is None for item in self.authorized_rules) and (
            tuple(item.rule_id for item in self.authorized_rules) != LEGACY_IDS
            or any(item.condition_hash is not None for item in self.authorized_rules)
        ):
            raise ValueError("Recognition old event must authorize the complete eight")
        return self


class Answer(RecordData):
    rule_id: RuleId
    condition_hash: Hash
    matcher_commit: Revision
    event_id: Code
    presented_in: Code
    restriction_ids: tuple[Code, ...]
    note: str

    @field_validator("restriction_ids")
    @classmethod
    def _restrictions(cls, value: tuple[str, ...]) -> tuple[str, ...]:
        ordered(value)
        return value


class Approval(Envelope):
    parameter_rule_approval_format: Literal[1]
    kind: Literal["template_parameter_rule_approval"]
    policy_id: Code
    policy_hash: Hash
    events: dict[Code, Event]
    rules: Annotated[tuple[Answer, ...], Field(min_length=1)]
    note: Text

    @model_validator(mode="after")
    def _answers(self) -> Self:
        ordered(tuple(item.rule_id for item in self.rules))
        if not self.events:
            raise ValueError("Recognition approval requires a real event")
        return self


class Pin(RecordData):
    policy_id: Code
    authored_revision: Revision
    path: Text
    hash: Hash
    approval_receipt_hash: Hash

    @model_validator(mode="after")
    def _path(self) -> Self:
        if (
            self.path
            != "authored/template-parameter-rules/" + self.policy_id + ".policy.yaml"
        ):
            raise ValueError("Recognition policy pin path must derive from its ID")
        return self

"""Closed ruling shapes with explicit nullable reference boundaries."""

from typing import Annotated, Literal, Self

from pydantic import Field, model_validator

from sve_carddb.contracts.four_layer import (
    Code,
    FrameId,
    OccurrenceKey,
    SemanticVariant,
)
from sve_carddb.core.json import canonical
from sve_carddb.core.models import Date, Hash, RecordData, Text, UInt

PATH_COMPONENTS = 4
MIN_AMBIGUOUS_CANDIDATES = 2

Revision = Annotated[int, Field(ge=1, le=9007199254740991)]
RulingId = Annotated[str, Field(pattern=r"^R-[0-9]{4,}\Z")]


class Evidence(RecordData):
    ref: Text
    kind: Literal[
        "direct", "supporting", "terminology", "community", "project", "counter"
    ]
    quote: str | None = None
    question: str | None = None
    date: Date | None = None
    note: str | None = None
    reason: str | None = None
    version: str | None = None
    url: str | None = None


class Ruling(RecordData):
    format: Literal[2]
    kind: Literal["ruling"]
    id: RulingId
    revision: Revision
    question: Text
    decision: Text
    evidence: tuple[Evidence, ...]
    strength: Literal["official", "generalized", "inferred", "undecided"]
    decided_on: Date
    applies_to: tuple[Text, ...]
    hint: dict[Text, Text]
    note: str = ""
    supersedes: Text | None = None


class RulingRef(RecordData):
    id: RulingId
    revision: Revision
    reference_ordinal: UInt


class RulingSource(RecordData):
    path: Text
    sha256: Hash

    @model_validator(mode="after")
    def _safe_path(self) -> Self:
        parts = self.path.split("/")
        if (
            parts[:3] != ["authored", "rules", "rulings"]
            or len(parts) != PATH_COMPONENTS
            or any(p in {"", ".", ".."} for p in parts)
            or "\\" in self.path
            or not parts[-1].endswith(".yaml")
        ):
            raise ValueError("Ruling source requires its safe repository-relative path")
        return self


class Scope(RecordData):
    role: Code
    domain: Code


class Target(RecordData):
    frame_id: FrameId
    semantic_variant: SemanticVariant
    scope: Scope
    occurrence: OccurrenceKey


class Mapping(RecordData):
    legacy_namespace: Code
    legacy_template_id: Text
    occurrence: OccurrenceKey
    frame_id: FrameId
    semantic_variant: SemanticVariant
    scope: Scope


class Legacy(RecordData):
    namespace: Code | None
    template_id: Text
    occurrence: OccurrenceKey | None

    @model_validator(mode="after")
    def _real_namespace(self) -> Self:
        if self.namespace == "unknown":
            raise ValueError("Unknown ruling namespace must be explicit null")
        return self


PendingReason = Literal[
    "unknown_legacy_scope",
    "no_candidate",
    "ambiguous_variant",
    "source_changed",
    "unsupported_relation",
]


class Resolution(RecordData):
    ruling_ref: RulingRef
    ruling_source: RulingSource
    level: Literal["reference", "occurrence"]
    legacy: Legacy
    status: Literal["resolved", "pending"]
    target: Target | None
    candidates: tuple[Target, ...]
    reason: PendingReason | None

    @model_validator(mode="after")
    def _disposition(self) -> Self:
        keys = tuple(canonical(c.model_dump(mode="json")) for c in self.candidates)
        if keys != tuple(sorted(set(keys))):
            raise ValueError("Ruling candidates must be sorted and unique")
        if self.level == "reference":
            if (
                self.legacy.occurrence is not None
                or self.status != "pending"
                or self.reason != "unknown_legacy_scope"
                or self.target is not None
                or self.candidates
            ):
                raise ValueError(
                    "Reference pending cannot claim an active occurrence or target"
                )
        elif self.legacy.namespace is None or self.legacy.occurrence is None:
            raise ValueError(
                "Occurrence resolution requires its verified namespace and occurrence"
            )
        elif self.status == "resolved":
            if self.target is None or self.candidates or self.reason is not None:
                raise ValueError("Resolved ruling requires exactly one active target")
        elif self.target is not None or self.reason in {None, "unknown_legacy_scope"}:
            raise ValueError("Occurrence pending requires an inactive explicit reason")
        elif self.reason == "no_candidate" and self.candidates:
            raise ValueError("No-candidate pending must have no candidates")
        elif (
            self.reason == "ambiguous_variant"
            and len(self.candidates) < MIN_AMBIGUOUS_CANDIDATES
        ):
            raise ValueError(
                "Ambiguous ruling requires at least two verified candidates"
            )
        return self


class RetainedReference(RecordData):
    ruling_ref: RulingRef
    ruling_source: RulingSource
    reference_kind: Literal["ir_element"]
    reference_id: Text
    disposition: Literal["retained_non_template"]

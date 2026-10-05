"""Closed wording-adoption-v1 receipts, independent of runtime reconstruction."""

from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.build_inputs import BuildContext, FilePin, Revision, Version
from sve_carddb.products.models import DecisionMetadata, Evidence
from sve_carddb.registry.records import (
    FaceId,
    Hash,
    PrintingId,
    RecordData,
    Region,
    Text,
    UInt,
)
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.text_observations.presence import EffectPresence

POLICY = "wording-adoption-v1"
Positive = Annotated[int, Field(gt=0)]


def ordered(values: tuple[str, ...]) -> None:
    """Reject noncanonical receipts rather than rewriting their checked membership."""
    if values != tuple(sorted(set(values))):
        raise ValueError("Receipt keys must be sorted and unique")


def ordered_objects(values: tuple[RecordData, ...]) -> None:
    """Preserve the normative canonical byte order for structured arrays."""
    keys = [canonical(item.model_dump(mode="json")) for item in values]
    if keys != sorted(set(keys)):
        raise ValueError("Receipt objects must be canonically sorted and unique")


class SourceBatch(RecordData):
    batch_id: Hash


class ReviewContext(RecordData):
    context: BuildContext
    source_batches: Annotated[tuple[SourceBatch, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _batches(self) -> Self:
        ordered_objects(self.source_batches)
        return self


class Correction(RecordData):
    correction_id: Text
    record_hash: Hash
    decision_id: Text
    status: Literal["applied", "already_fixed"]


class Observation(RecordData):
    observation_key: Text
    printing_id: PrintingId
    source_index: UInt
    source_version_id: Version
    raw_hash: Hash
    parser_version: Text
    raw_face_hash: Hash
    effect_presence: EffectPresence
    corrections: tuple[Correction, ...]
    content_hash: Hash

    @model_validator(mode="after")
    def _pins(self) -> Self:
        proof = self.effect_presence.result
        if (
            proof.source_version_id != self.source_version_id
            or proof.source_index != self.source_index
        ):
            raise ValueError("Presence proof disagrees with observation source/index")
        ordered(tuple(c.correction_id for c in self.corrections))
        return self

    def key(self, face: str, region: Region) -> str:
        """Cover raw, projection and every correction pin without source deduplication."""
        return canonical(
            [
                "wording-observation-v1",
                self.printing_id,
                face,
                region,
                self.source_index,
                self.source_version_id,
                self.raw_hash,
                self.parser_version,
                self.raw_face_hash,
                self.effect_presence.model_dump(mode="json"),
                [c.model_dump(mode="json") for c in self.corrections],
                self.content_hash,
            ]
        ).decode()


class OrderReceipt(RecordData):
    before_observation_keys: Annotated[tuple[Text, ...], Field(min_length=1)]
    after_observation_keys: Annotated[tuple[Text, ...], Field(min_length=1)]
    note: Text

    @model_validator(mode="after")
    def _review(self) -> Self:
        if not self.note.strip():
            raise ValueError("Order receipt requires an actual answer")
        ordered(self.before_observation_keys)
        ordered(self.after_observation_keys)
        return self


OrderBasis = Literal["printing_availability", "source_update", "reviewed_order"]


def _order_support(
    basis: str, indexes: tuple[int, ...], receipt: OrderReceipt | None
) -> None:
    if indexes != tuple(sorted(set(indexes))):
        raise ValueError("Order evidence indexes must be sorted and unique")
    if basis == "reviewed_order":
        if indexes or receipt is None:
            raise ValueError(
                "Reviewed order requires an explicit answer, without date evidence"
            )
    elif basis == "same_content":
        if indexes or receipt is not None:
            raise ValueError("Same-content previous order has no evidence or answer")
    elif not indexes or receipt is not None:
        raise ValueError("Official order requires evidence and no invented answer")


class OrderEvidence(RecordData):
    before_level: UInt
    after_level: UInt
    basis: OrderBasis
    evidence_indexes: tuple[UInt, ...]
    review_receipt: OrderReceipt | None

    @model_validator(mode="after")
    def _support(self) -> Self:
        _order_support(self.basis, self.evidence_indexes, self.review_receipt)
        if self.after_level != self.before_level + 1:
            raise ValueError("Order evidence must join adjacent levels")
        return self


class PreviousOrder(RecordData):
    basis: Literal[
        "same_content", "printing_availability", "source_update", "reviewed_order"
    ]
    evidence_indexes: tuple[UInt, ...]
    review_receipt: OrderReceipt | None

    @model_validator(mode="after")
    def _support(self) -> Self:
        _order_support(self.basis, self.evidence_indexes, self.review_receipt)
        return self


class Mechanical(RecordData):
    kind: Literal["mechanical"]
    review_context: ReviewContext
    observations: Annotated[tuple[Observation, ...], Field(min_length=1)]
    observations_hash: Hash
    selected_observation_key: Text


class PreviousAdoption(RecordData):
    kind: Literal["adoption"]
    record_key: Text
    record_hash: Hash
    decision_id: Text


Previous = Annotated[Mechanical | PreviousAdoption, Field(discriminator="kind")]


class RuleSet(RecordData):
    policy_id: Text
    authored_revision: Revision
    path: Text
    hash: Hash
    approval_receipt_hash: Hash

    @model_validator(mode="after")
    def _relative_path(self) -> Self:
        FilePin(name=self.path, sha256=self.hash)
        return self


class RuleMatch(RecordData):
    from_observation_key: Text
    to_observation_key: Text
    rule_id: Text
    field: Text
    before_range: tuple[UInt, UInt]
    after_range: tuple[UInt, UInt]

    @model_validator(mode="after")
    def _ranges(self) -> Self:
        if any(start > end for start, end in (self.before_range, self.after_range)):
            raise ValueError("Rule range must be a forward half-open interval")
        return self


class Review(RecordData):
    mode: Literal["human", "approved_rules"]
    rule_set: RuleSet | None
    rule_matches: tuple[RuleMatch, ...]

    @model_validator(mode="after")
    def _mode_fields(self) -> Self:
        if self.mode == "human":
            if self.rule_set is not None or self.rule_matches:
                raise ValueError("Human review cannot impersonate a policy receipt")
        elif self.rule_set is None:
            raise ValueError(
                "Approved rules require the exact policy and approval receipt"
            )
        ordered_objects(self.rule_matches)
        return self


def check_observations(
    observations: tuple[Observation, ...], checksum: str, face: str, region: Region
) -> None:
    """Require complete identities and the exact array hash before any reconstruction."""
    ordered(tuple(item.observation_key for item in observations))
    if checksum != digest(
        canonical([item.model_dump(mode="json") for item in observations])
    ):
        raise ValueError("Observations hash mismatch")
    if any(item.observation_key != item.key(face, region) for item in observations):
        raise ValueError("Observation key does not match exact pins")


class AdoptionData(RecordData):
    face_id: FaceId
    region: Region
    adoption_no: Positive
    review_context: ReviewContext
    observations: Annotated[tuple[Observation, ...], Field(min_length=1)]
    observations_hash: Hash
    checked_observation_keys: tuple[Text, ...]
    previous: Previous | None
    equivalence: Literal["equivalent"]
    review: Review
    wording_order: Annotated[tuple[tuple[Text, ...], ...], Field(min_length=1)]
    order_evidence: tuple[OrderEvidence, ...]
    selected_observation_key: Text
    previous_order: PreviousOrder | None

    @model_validator(mode="after")
    def _membership(self) -> Self:
        check_observations(
            self.observations, self.observations_hash, self.face_id, self.region
        )
        keys = tuple(item.observation_key for item in self.observations)
        if self.checked_observation_keys != keys:
            raise ValueError(
                "Checked observations must cover every exact source member"
            )
        if (
            self.previous is None or isinstance(self.previous, Mechanical)
        ) and self.adoption_no != 1:
            raise ValueError("Only the first adoption can have a null/mechanical root")
        if isinstance(self.previous, PreviousAdoption) and self.adoption_no == 1:
            raise ValueError(
                "The first adoption cannot reference an adoption predecessor"
            )
        if (self.previous is None) != (self.previous_order is None):
            raise ValueError("Previous content requires its independent order proof")
        if isinstance(self.previous, Mechanical):
            check_observations(
                self.previous.observations,
                self.previous.observations_hash,
                self.face_id,
                self.region,
            )
            if self.previous.selected_observation_key not in {
                o.observation_key for o in self.previous.observations
            }:
                raise ValueError(
                    "Mechanical selected observation is outside its full root"
                )
        self._levels()
        return self

    def _levels(self) -> None:
        flattened: list[str] = []
        contents = {o.observation_key: o.content_hash for o in self.observations}
        for level in self.wording_order:
            if not level:
                raise ValueError("Wording order levels must be nonempty")
            ordered(level)
            flattened.extend(level)
            if (
                not set(level) <= contents.keys()
                or len({contents[k] for k in level}) != 1
            ):
                raise ValueError("Same wording level must have one exact content hash")
        if sorted(flattened) != sorted(contents) or len(flattened) != len(contents):
            raise ValueError(
                "Wording order must partition all observations exactly once"
            )
        if self.selected_observation_key not in self.wording_order[-1]:
            raise ValueError(
                "Selected observation must belong to the final wording level"
            )
        if [(edge.before_level, edge.after_level) for edge in self.order_evidence] != [
            (i, i + 1) for i in range(len(self.wording_order) - 1)
        ]:
            raise ValueError(
                "Every adjacent wording level requires exactly one order proof"
            )
        for edge in self.order_evidence:
            if edge.review_receipt is not None and (
                edge.review_receipt.before_observation_keys
                != self.wording_order[edge.before_level]
                or edge.review_receipt.after_observation_keys
                != self.wording_order[edge.after_level]
            ):
                raise ValueError(
                    "Order answer disagrees with the exact adjacent members"
                )


class AdoptionRecord(RecordData):
    record_key: Text
    kind: Literal["wording_adoption"]
    filing_key: Region
    data: AdoptionData
    evidence: Annotated[tuple[Evidence, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _primary(self) -> Self:
        d = self.data
        if (
            self.record_key
            != canonical([self.kind, d.face_id, d.region, d.adoption_no]).decode()
            or self.filing_key != d.region
        ):
            raise ValueError("Adoption record key/filing region mismatch")
        ordered_objects(self.evidence)
        for proof in (
            *d.order_evidence,
            *((d.previous_order,) if d.previous_order else ()),
        ):
            if any(index >= len(self.evidence) for index in proof.evidence_indexes):
                raise ValueError("Order evidence index is outside the complete record")
        batches = {b.batch_id for b in d.review_context.source_batches}
        for observation in d.observations:
            locator = canonical(
                {
                    "printing_id": observation.printing_id,
                    "face_id": d.face_id,
                    "source_index": observation.source_index,
                }
            ).decode()
            if not any(
                e.role == "wording_observation"
                and e.source_version_id == observation.source_version_id
                and e.locator == locator
                and e.batch_id in batches
                for e in self.evidence
            ):
                raise ValueError(
                    "Every observation requires its exact frozen evidence locator"
                )
        return self


class Decision(DecisionMetadata):
    state: Literal["confirmed"]
    category: Literal["wording_adoption"]
    policy_id: Literal["wording-adoption-v1"]


class _Envelope(RecordData):
    wording_adoption_format: Literal[1]

    @field_validator("wording_adoption_format", mode="before")
    @classmethod
    def format_integer(cls, value: object) -> object:
        """Literal coercion must not turn a boolean into an approved format."""
        if type(value) is not int:
            raise ValueError("Wording adoption format must be an integer")
        return value


class Index(_Envelope):
    kind: Literal["wording_adoption_index"]
    includes: dict[str, Hash]


class Shard(_Envelope):
    kind: Literal["wording_adoption_shard"]
    default_decision_id: Text
    records: Annotated[tuple[AdoptionRecord, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]

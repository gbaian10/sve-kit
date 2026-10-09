"""Required, strict wire fields for complete identity transition envelopes."""

from typing import Annotated, Literal

from pydantic import Field, JsonValue, computed_field, field_validator, model_validator

from sve_carddb.core.json import canonical
from sve_carddb.core.models import Hash, RecordData, Text, UInt
from sve_carddb.core.provenance import BuildContext, Revision, Version
from sve_carddb.domains.registry.records import (
    DATA_MODELS,
    CardId,
    EnglishPrintingData,
    FaceId,
    PrintingId,
)
from sve_carddb.domains.routes.codec import card_path

ArtId = Annotated[str, Field(pattern=r"^a:[0-9a-f]{32}\Z")]
TargetKey = Annotated[
    str,
    Field(
        pattern=r"^(?:card:c|face:f|printing:p|art:a|region_mapping_review:c|card_related:r):[0-9a-f]{32}\Z"
    ),
]
Sequence = Annotated[UInt, Field(ge=1)]
SPLIT_MIN_TARGETS = 2


def ordered(values: tuple[RecordData, ...]) -> None:
    """Reject noncanonical arrays instead of silently sorting authored content."""
    keys = tuple(canonical(value.model_dump(mode="json")) for value in values)
    if keys != tuple(sorted(set(keys))):
        raise ValueError("Transition arrays must be sorted and unique")


def names_ordered(values: tuple[str, ...]) -> None:
    """Keep scalar sets explicitly ordered on the wire."""
    if values != tuple(sorted(set(values))):
        raise ValueError("Transition identifiers must be sorted and unique")


class Reference(RecordData):
    record_key: Text
    record_hash: Hash


class Before(Reference):
    transition_key: Text | None


class RegistryBasis(RecordData):
    authored_revision: Revision
    index_path: Literal["ids/index.yaml"]
    index_hash: Hash


class Batch(RecordData):
    batch_id: Hash


class Evidence(Batch):
    source_version_id: Version
    locator: Text
    role: Text


class ReviewContext(RecordData):
    context: BuildContext
    source_batches: Annotated[tuple[Batch, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _order(self) -> ReviewContext:
        ordered(self.source_batches)
        return self


class After(RecordData):
    kind: Literal[
        "card", "face", "printing", "art", "region_mapping_review", "card_related"
    ]
    owner: Text
    data: dict[str, JsonValue]

    @model_validator(mode="after")
    def _data(self) -> After:
        model = DATA_MODELS[self.kind]
        if self.kind == "printing" and self.data.get("region") == "en":
            model = EnglishPrintingData
        model.model_validate_json(canonical(self.data))

        return self

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return (
            self.kind
            + ":"
            + str(
                self.data["card_id" if self.kind == "region_mapping_review" else "id"]
            )
        )


class Update(RecordData):
    target_key: TargetKey
    before: Before | None
    after: After | None
    allocation_anchor: Text | None

    @model_validator(mode="after")
    def _refs(self) -> Update:
        kind = self.target_key.split(":", maxsplit=1)[0]
        if self.before is None:
            if kind not in {"card", "face", "art"} or self.after is None:
                raise ValueError("Only card/face/art can be newly allocated")
            if self.allocation_anchor is None:
                raise ValueError("New transition entities require an allocation anchor")
        elif (
            self.before.record_key != self.target_key
            or self.allocation_anchor is not None
        ):
            raise ValueError("Existing update requires its exact key and no anchor")
        if self.after is None:
            if kind not in {"region_mapping_review", "card_related"}:
                raise ValueError("Only review/relation can be deactivated")
        elif self.after.record_key != self.target_key:
            raise ValueError("Transition after must match target key")
        return self


class FaceMove(RecordData):
    source_index: UInt
    from_face_id: FaceId
    to_face_id: FaceId
    from_art_id: ArtId | None
    to_art_id: ArtId | None


class PrintingMove(RecordData):
    printing_id: PrintingId
    from_card_id: CardId
    to_card_id: CardId
    faces: Annotated[tuple[FaceMove, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _faces(self) -> PrintingMove:
        if self.from_card_id == self.to_card_id:
            raise ValueError("Printing move must change its parent card")
        indexes = tuple(face.source_index for face in self.faces)
        if indexes != tuple(range(len(indexes))):
            raise ValueError("Printing move source faces must be complete and ordered")
        if any(
            face.from_art_id is not None and face.to_art_id is None
            for face in self.faces
        ):
            raise ValueError("Known art cannot be discarded during a printing move")
        return self


class FaceTransfer(RecordData):
    from_face_id: FaceId
    to_face_ids: Annotated[tuple[FaceId, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _targets(self) -> FaceTransfer:
        names_ordered(self.to_face_ids)
        if self.from_face_id in self.to_face_ids:
            raise ValueError("Cross-card transfer cannot reuse the old face ID")
        return self


class Use(RecordData):
    printing_id: PrintingId
    face_id: FaceId


class ArtTarget(RecordData):
    to_art_id: ArtId
    to_face_id: FaceId
    uses: tuple[Use, ...]

    @model_validator(mode="after")
    def _uses(self) -> ArtTarget:
        ordered(self.uses)
        return self


class ArtTransfer(RecordData):
    from_art_id: ArtId
    targets: tuple[ArtTarget, ...]
    remaining_uses: tuple[Use, ...]

    @model_validator(mode="after")
    def _order(self) -> ArtTransfer:
        ordered(self.targets)
        ordered(self.remaining_uses)
        return self


class Repair(RecordData):
    id: Text
    kind: Literal["merge", "split", "reassign_printing"]
    old_card_id: CardId
    new_card_ids: Annotated[tuple[CardId, ...], Field(min_length=1)]
    printing_moves: Annotated[tuple[PrintingMove, ...], Field(min_length=1)]
    face_moves: Annotated[tuple[FaceTransfer, ...], Field(min_length=1)]
    art_moves: tuple[ArtTransfer, ...]
    retire_old: bool
    reason: Text

    @model_validator(mode="after")
    def _shape(self) -> Repair:
        names_ordered(self.new_card_ids)
        for moves in (self.printing_moves, self.face_moves, self.art_moves):
            ordered(moves)
        if self.old_card_id in self.new_card_ids:
            raise ValueError("Repair cannot target its old card")
        if self.kind == "split":
            if len(self.new_card_ids) < SPLIT_MIN_TARGETS or not self.retire_old:
                raise ValueError("Split requires multiple destinations and retirement")
        elif len(self.new_card_ids) != 1:
            raise ValueError("Merge/reassign requires one destination")
        if self.kind == "merge" and not self.retire_old:
            raise ValueError("Merge requires retirement")
        if self.kind == "reassign_printing" and len(self.printing_moves) != 1:
            raise ValueError("Reassign requires exactly one printing")
        if any(
            move.from_card_id != self.old_card_id
            or move.to_card_id not in self.new_card_ids
            for move in self.printing_moves
        ):
            raise ValueError("Printing moves disagree with repair destinations")
        if {move.to_card_id for move in self.printing_moves} != set(self.new_card_ids):
            raise ValueError("Every repair destination must receive a printing")
        return self


class Route(RecordData):
    namespace: Literal["official", "provisional"]
    route_key: Text

    @model_validator(mode="after")
    def _key(self) -> Route:
        card_path(self.namespace, self.route_key)
        return self


class Alias(Route):
    reason: Literal["renumbered", "merged", "provisional_corrected"]


class RouteState(RecordData):
    canonical: Route
    aliases: tuple[Alias, ...]

    @model_validator(mode="after")
    def _aliases(self) -> RouteState:
        ordered(self.aliases)
        keys = [(alias.namespace, alias.route_key) for alias in self.aliases]
        if (
            len(set(keys)) != len(keys)
            or (
                self.canonical.namespace,
                self.canonical.route_key,
            )
            in keys
        ):
            raise ValueError("Route aliases must be unique and exclude canonical")
        return self


class RouteUpdate(RecordData):
    printing_id: PrintingId
    before: RouteState
    after: RouteState

    @model_validator(mode="after")
    def _changed(self) -> RouteUpdate:
        if self.before == self.after:
            raise ValueError("Routes must describe an actual projection change")
        return self


class Transition(RecordData):
    kind: Literal["identity_transition"]
    action: Literal["apply", "revert"]
    reverts: Reference | None
    sequence: Sequence
    previous: Reference | None
    registry_basis: RegistryBasis
    review_context: ReviewContext
    updates: Annotated[tuple[Update, ...], Field(min_length=1)]
    repairs: tuple[Repair, ...]
    routes: tuple[RouteUpdate, ...]
    evidence: Annotated[tuple[Evidence, ...], Field(min_length=1)]
    reason: Text

    @model_validator(mode="after")
    def _structure(self) -> Transition:
        if (self.action == "apply" and self.reverts is not None) or (
            self.action == "revert" and (self.reverts is None or self.repairs)
        ):
            raise ValueError("Apply/revert fields disagree")
        names_ordered(tuple(update.target_key for update in self.updates))
        names_ordered(tuple(repair.id for repair in self.repairs))
        ordered(self.routes)
        ordered(self.evidence)
        batches = set(self.review_context.source_batches)
        if any(Batch(batch_id=item.batch_id) not in batches for item in self.evidence):
            raise ValueError("Transition evidence batch missing from review context")
        return self

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical([self.kind, self.sequence]).decode()


class Shard(RecordData):
    identity_transition_format: Literal[1]
    kind: Literal["identity_transition_shard"]
    records: Annotated[tuple[Transition, ...], Field(min_length=1, max_length=1)]

    @field_validator("identity_transition_format", mode="before")
    @classmethod
    def _format(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Transition format must be integer 1")
        return value

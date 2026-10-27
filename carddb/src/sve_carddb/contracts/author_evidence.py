"""Closed author provenance and review shapes, without source admission."""

from typing import Annotated, Literal, NotRequired, TypedDict

from pydantic import ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import ParamName, ParamValue, ParamValues
from sve_carddb.contracts.author_references import BindingName
from sve_carddb.contracts.four_layer import (
    Code,
    FaceRevisionOwner,
    FrameId,
    Hash,
    Id,
    OccurrenceKey,
)
from sve_carddb.core.models import Date, RecordData, Text, UInt

FaceId = Annotated[str, Field(pattern=r"^f:[0-9a-f]{32}$(?![\s\S])")]
Digest = Annotated[str, Field(pattern=r"^sha256:[0-9a-f]{64}$(?![\s\S])")]
BodyVersion = Annotated[
    str, Field(pattern=r"^effect-body-v1:sha256:[0-9a-f]{64}$(?![\s\S])")
]
SourceVersion = Annotated[
    str, Field(pattern=r"^effect-source-v1:sha256:[0-9a-f]{64}$(?![\s\S])")
]
Positive = Annotated[int, Field(ge=1, le=9007199254740991)]
type StructuralPath = tuple[str | UInt, ...]


class SourceUse(RecordData):
    binding_id: Id
    occurrence: OccurrenceKey
    ability_scope: Code


class CardKeywordSourceUse(RecordData):
    binding_id: Id
    occurrence: OccurrenceKey


class DirectSourceUse(RecordData):
    admission: Literal["light", "shared"]
    owner: FaceRevisionOwner
    face_id: FaceId
    field: Literal["effect", "section"]
    ordinal: UInt | None
    line: Positive
    scope: Text
    covers: Annotated[tuple[Positive, ...], Field(min_length=1)]


Sources = Annotated[
    tuple[SourceUse | DirectSourceUse, ...], Field(min_length=1, max_length=1)
]
CardKeywordSources = Annotated[
    tuple[CardKeywordSourceUse | DirectSourceUse, ...],
    Field(min_length=1, max_length=1),
]


class LocalBinding(RecordData):
    path: StructuralPath
    bind: BindingName


class ImportBinding(RecordData):
    path: tuple[()]
    bind: BindingName


class Ports(RecordData):
    imports: dict[Code, ImportBinding]
    exports: dict[Code, LocalBinding]


class EmptyPorts(Ports):
    imports: Annotated[dict[Code, ImportBinding], Field(max_length=0)]
    exports: Annotated[dict[Code, LocalBinding], Field(max_length=0)]


class ImportUse(RecordData):
    binding_id: Id
    ability_scope: Code
    port: Code


ImportMap = Annotated[dict[Code, ImportUse], Field(min_length=1)]


class Actor(RecordData):
    who: Text
    model: Text | None


class WrittenBy(RecordData):
    actor: Actor
    date: Date


Writers = Annotated[tuple[WrittenBy, ...], Field(min_length=1)]


class Domain(RecordData):
    regions: Annotated[tuple[Literal["jp", "en"], ...], Field(min_length=1)]


class RecipePin(RecordData):
    id: Code
    hash: Digest


class InterfaceDependency(RecordData):
    frame_id: FrameId
    interface_key: Hash


class RulingDependency(RecordData):
    id: Text
    revision: Positive


class ImportedDependency(ImportUse):
    dependency_key: Digest


class ResolvedVariant(RecordData):
    state: Literal["resolved"]
    key: Code
    scope: None


class UseDependencyFields(RecordData):
    sources: Sources
    frame_id: FrameId
    ability_scope: Code
    semantic_variant: ResolvedVariant
    body_version: BodyVersion
    interfaces: tuple[InterfaceDependency, ...]
    source: SourceVersion
    rulings: tuple[RulingDependency, ...]
    expansion_recipe: RecipePin
    adapter_recipe: RecipePin
    imports: dict[Code, ImportedDependency]


class InlineDependencies(UseDependencyFields):
    applicability_scope: None
    domain_hash: None
    params: Annotated[dict[ParamName, ParamValue], Field(max_length=0)]


class MacroDependencies(UseDependencyFields):
    applicability_scope: Code
    domain_hash: Digest
    params: ParamValues


type UseDependencies = InlineDependencies | MacroDependencies


class ReviewedSource(SourceUse):
    source: SourceVersion


class BodyDependencies(RecordData):
    frame_id: FrameId
    ability_scope: Code
    body_version: BodyVersion
    applicability_scope: Code
    domain_hash: Digest
    interfaces: tuple[InterfaceDependency, ...]
    sources: tuple[ReviewedSource, ...]
    rulings: tuple[RulingDependency, ...]
    expansion_recipe: RecipePin
    adapter_recipe: RecipePin


class OriginalEvidence(RecordData):
    input_hash: Digest
    record_key: Text


class BodyRekey(RecordData):
    original: OriginalEvidence
    before: BodyDependencies
    after: BodyDependencies
    mapping_recipe: RecipePin


class UseRekey(RecordData):
    original: OriginalEvidence
    before: UseDependencies
    after: UseDependencies
    mapping_recipe: RecipePin


@with_config(ConfigDict(extra="forbid", strict=True, regex_engine="python-re"))
class ReviewFields(TypedDict):
    reviewer: Actor
    date: Date
    verdict: Literal["ok", "revise", "disputed"]
    reason: Text | None


AnchorVersion = Annotated[
    str, Field(pattern=r"^effect-anchor-v1:sha256:[0-9a-f]{64}$(?![\s\S])")
]


class BodyReview(ReviewFields):
    kind: Literal["body"]
    anchor: NotRequired[AnchorVersion]
    dependencies: BodyDependencies
    rekey: NotRequired[BodyRekey]


class UseReview(ReviewFields):
    kind: Literal["use"]
    anchor: NotRequired[AnchorVersion]
    dependencies: UseDependencies
    rekey: NotRequired[UseRekey]


class BodyReference(RecordData):
    frame_id: FrameId
    ability_scope: Code
    body_version: BodyVersion
    applicability_scope: Code


class MacroMechanical(RecordData):
    kind: Literal["macro_mechanical"]
    body: BodyReference
    # Mechanical checks only exist for referenced bodies; inline uses have no domain.
    use_dependencies: MacroDependencies


class Registration(RecordData):
    id: Code
    domain: Domain
    reviews: tuple[BodyReview, ...]


class Meta(RecordData):
    dsl: Literal["1.0"]
    source: SourceVersion
    written_by: Writers
    reviewed_by: tuple[UseReview | MacroMechanical, ...]
    status: Literal["draft", "reviewed", "verified", "disputed"]
    verified_by_exam: tuple[()]
    rulings: tuple[Text, ...]


class DirectMeta(Meta):
    source: Literal["direct"]
    admission: Literal["light", "shared"]

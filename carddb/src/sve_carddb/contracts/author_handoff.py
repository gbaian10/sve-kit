"""Closed internal current-source obligations and keyword declaration evidence."""

from typing import Annotated, Literal, Self

from pydantic import Field, JsonValue, model_validator

from sve_carddb.contracts.author_evidence import (
    CardKeywordSourceUse,
    Digest,
    EmptyPorts,
    FaceId,
    SourceUse,
    StructuralPath,
)
from sve_carddb.contracts.four_layer import FrameId, Hash, Id, OccurrenceKey
from sve_carddb.core.models import RecordData

type PrintedKeyword = Literal["ward", "aura"]
type MetadataKey = Literal[
    "metadata.name.v1", "metadata.label.v1", "metadata.layout.v1", "pure_reminder.v1"
]


class KeywordDeclarationPayload(RecordData):
    keywords: Annotated[tuple[PrintedKeyword, ...], Field(min_length=1, max_length=2)]
    ports: EmptyPorts

    @model_validator(mode="after")
    def _unique(self) -> Self:
        if len(set(self.keywords)) != len(self.keywords):
            raise ValueError("Printed keywords must be unique within a declaration")
        return self


class MetadataObligation(RecordData):
    kind: Literal["metadata"]
    binding_id: Id
    occurrence: OccurrenceKey
    semantic_key: MetadataKey


class PrintedKeywordsObligation(RecordData):
    kind: Literal["printed_keywords"]
    source_use: CardKeywordSourceUse
    face_id: FaceId
    frame_id: FrameId
    interface_key: Hash
    semantic_key: Literal["card_keywords.v2"]
    declaration: KeywordDeclarationPayload
    source_adapter_recipe: Digest


class BodyObligation(RecordData):
    source_use: SourceUse
    body: dict[str, JsonValue]


class SourceInventory(RecordData):
    abilities: tuple[BodyObligation, ...]
    non_body: tuple[
        Annotated[
            MetadataObligation | PrintedKeywordsObligation, Field(discriminator="kind")
        ],
        ...,
    ]


class CurrentKeywordDeclaration(RecordData):
    kind: Literal["printed_keywords"]
    path: StructuralPath
    source_proof: Literal["current_source_checked"]
    input_hash: Digest
    source_input_hash: Digest
    source_use: CardKeywordSourceUse
    face_id: FaceId
    frame_id: FrameId
    interface_key: Hash
    semantic_key: Literal["card_keywords.v2"]
    declaration: KeywordDeclarationPayload
    source_adapter_recipe: Digest
    expansion_recipe: Digest
    adapter_recipe: Digest

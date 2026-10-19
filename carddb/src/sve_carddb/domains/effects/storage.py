"""Readable authored evidence resolves to the unchanged classification identities."""

from typing import Annotated, Literal, Self

from pydantic import Field, field_validator, model_validator

from sve_carddb.contracts.four_layer import Span
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.models import Hash, RecordData, Text, UInt
from sve_carddb.domains.effects.classifications import (
    Ability,
    Basis,
    Classification,
    Code,
    Member,
    Position,
)

Owner = Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+\Z")]


class TextVersionMismatchError(ValueError):
    """A safe validation category distinguishable without returning source input."""


class SourceText(RecordData):
    id: Hash
    raw: str
    body: str | None = None

    @property
    def body_text(self) -> str:
        """Omitted body uses the exact raw bytes, without normalization."""
        return self.raw if self.body is None else self.body

    @model_validator(mode="after")
    def _version(self) -> Self:
        if self.id != digest(canonical({"raw": self.raw, "body": self.body_text})):
            raise TextVersionMismatchError("Classification text version hash mismatch")
        return self


class SourceShard(RecordData):
    format: Literal[1]
    kind: Literal["classification_source_shard"]
    namespace: Code
    basis: Basis
    owner: Owner
    records: Annotated[tuple[SourceText, ...], Field(min_length=1)]

    @field_validator("format", mode="before")
    @classmethod
    def _integer(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Source format must be integer one")
        return value


class StoredPosition(Position):
    ordinal: UInt | None = None


class StoredClauseSource(RecordData):
    template: Code
    body: Span
    raw: Span | None = None


class StoredMember(RecordData):
    kind: Literal["member"]
    id: Text
    template: Code
    position: StoredPosition
    source: Hash
    body_length: UInt
    raw_length: UInt | None = None
    token_header_hash: Hash | None = None
    block: Text | None = None
    evidence_hash: Hash
    clauses: Annotated[tuple[StoredClauseSource, ...], Field(min_length=1)]

    @property
    def owner(self) -> str:
        """Source versions stay in the member's original card-pack scope."""
        return self.position.card_no.split("-", 1)[0]

    def resolve(self, text: SourceText) -> Member:
        """Recover old identities so existing scope checks remain independent."""
        raw_length = self.body_length if self.raw_length is None else self.raw_length
        if self.body_length != len(text.body_text) or raw_length != len(text.raw):
            raise ValueError("Classification text length mismatch")
        return Member.model_validate_json(
            canonical(
                {
                    "kind": self.kind,
                    "id": digest(self.id.encode()),
                    "template": self.template,
                    "position": self.position.model_dump(mode="json"),
                    "body_hash": digest(text.body_text.encode()),
                    "raw_hash": digest(text.raw.encode()),
                    "body_length": self.body_length,
                    "raw_length": raw_length,
                    "token_header_hash": self.token_header_hash,
                    "block": digest(
                        (self.id if self.block is None else self.block).encode()
                    ),
                    "evidence_hash": self.evidence_hash,
                    "clauses": [
                        {
                            "id": digest(f"{self.id}~{ordinal}".encode()),
                            "template": clause.template,
                            "body": clause.body.model_dump(mode="json"),
                            "raw": (
                                clause.body if clause.raw is None else clause.raw
                            ).model_dump(mode="json"),
                        }
                        for ordinal, clause in enumerate(self.clauses)
                    ],
                }
            )
        )


class StoredAbility(Ability):
    cost_timing: Code | None = None
    note_hash: Hash | None = None


class StoredClause(RecordData):
    id: Text
    ability: Code
    stage: Code
    ir: tuple[Code, ...]
    author: tuple[Code, ...]
    note_hash: Hash
    decision: Code | None = None
    capture: Code | None = None


class StoredEndpoint(RecordData):
    clause: Text
    ability: Code | None = None


class StoredRelation(RecordData):
    type: Literal[
        "sequence",
        "branch",
        "cost-of",
        "result-of",
        "capture-of",
        "scope-of",
        "simultaneous",
    ]
    note_hash: Hash
    sources: Annotated[tuple[StoredEndpoint, ...], Field(min_length=1)]
    targets: Annotated[tuple[StoredEndpoint, ...], Field(min_length=1)]


class StoredClassification(RecordData):
    kind: Literal["classification"]
    id: Code
    status: Literal["candidate"]
    family: Code
    normalized_hash: Hash
    definition_hash: Hash
    label_hash: Hash
    members: Annotated[tuple[Text, ...], Field(min_length=1)]
    abilities: Annotated[tuple[StoredAbility, ...], Field(min_length=1)]
    clauses: Annotated[tuple[StoredClause, ...], Field(min_length=1)]
    relations: tuple[StoredRelation, ...]

    def resolve(self) -> Classification:
        """Retain the original representative binding through exact CID hashes."""
        return Classification.model_validate_json(
            canonical(
                {
                    **self.model_dump(mode="json"),
                    "members": [digest(uid.encode()) for uid in self.members],
                    "clauses": [
                        {
                            **clause.model_dump(mode="json"),
                            "id": digest(clause.id.encode()),
                        }
                        for clause in self.clauses
                    ],
                    "relations": [
                        {
                            **relation.model_dump(mode="json"),
                            "sources": [
                                {
                                    "clause": digest(endpoint.clause.encode()),
                                    "ability": endpoint.ability,
                                }
                                for endpoint in relation.sources
                            ],
                            "targets": [
                                {
                                    "clause": digest(endpoint.clause.encode()),
                                    "ability": endpoint.ability,
                                }
                                for endpoint in relation.targets
                            ],
                        }
                        for relation in self.relations
                    ],
                }
            )
        )


StoredRecord = Annotated[
    StoredClassification | StoredMember, Field(discriminator="kind")
]


class StoredShard(RecordData):
    format: Literal[2]
    kind: Literal["effect_classification_shard"]
    namespace: Code
    basis: Basis
    records: Annotated[tuple[StoredRecord, ...], Field(min_length=1)]

    @field_validator("format", mode="before")
    @classmethod
    def _integer(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Classification format must be integer two")
        return value

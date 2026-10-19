"""Resolved candidate records retain the original classification identities and scope."""

from typing import Annotated, Literal, Self

from pydantic import Field, model_validator

from sve_carddb.contracts.four_layer import Span
from sve_carddb.core.models import Hash, RecordData, UInt

Code = Annotated[str, Field(pattern=r"^[\x21-\x7e]+\Z")]


class Position(RecordData):
    card_no: Annotated[str, Field(min_length=1)]
    region: Literal["jp"]
    face: UInt
    field: Literal["effect", "section"]
    ordinal: UInt | None
    line: UInt

    @model_validator(mode="after")
    def _ordinal(self) -> Self:
        if (self.field == "effect") != (self.ordinal is None):
            raise ValueError("Only effect has no section ordinal")
        return self


class ClauseSource(RecordData):
    id: Hash
    template: Code
    body: Span
    raw: Span


class Member(RecordData):
    kind: Literal["member"]
    id: Hash
    template: Code
    position: Position
    body_hash: Hash
    raw_hash: Hash
    body_length: UInt
    raw_length: UInt
    token_header_hash: Hash | None
    block: Hash
    evidence_hash: Hash
    clauses: Annotated[tuple[ClauseSource, ...], Field(min_length=1)]

    @model_validator(mode="after")
    def _spans(self) -> Self:
        cursor = 0
        raw_end = 0
        for clause in self.clauses:
            if (
                clause.body.start != cursor
                or clause.body.end > self.body_length
                or clause.raw.start < raw_end
                or clause.raw.end > self.raw_length
            ):
                raise ValueError("Clause spans do not partition member body")
            cursor = clause.body.end
            raw_end = clause.raw.end
        if cursor != self.body_length:
            raise ValueError("Clause spans do not cover member body")
        if len({c.id for c in self.clauses}) != len(self.clauses):
            raise ValueError("Duplicate member clause")
        return self


class Ability(RecordData):
    id: Code
    head: Code
    ir: Code
    cost_timing: Code | None
    note_hash: Hash | None


class Clause(RecordData):
    id: Hash
    ability: Code
    decision: Code | None
    capture: Code | None
    stage: Code
    ir: tuple[Code, ...]
    author: tuple[Code, ...]
    note_hash: Hash


class Endpoint(RecordData):
    clause: Hash
    ability: Code | None


class Relation(RecordData):
    type: Literal[
        "sequence",
        "branch",
        "cost-of",
        "result-of",
        "capture-of",
        "scope-of",
        "simultaneous",
    ]
    sources: Annotated[tuple[Endpoint, ...], Field(min_length=1)]
    targets: Annotated[tuple[Endpoint, ...], Field(min_length=1)]
    note_hash: Hash


class Classification(RecordData):
    kind: Literal["classification"]
    id: Code
    status: Literal["candidate"]
    family: Code
    normalized_hash: Hash
    definition_hash: Hash
    label_hash: Hash
    members: Annotated[tuple[Hash, ...], Field(min_length=1)]
    abilities: Annotated[tuple[Ability, ...], Field(min_length=1)]
    clauses: Annotated[tuple[Clause, ...], Field(min_length=1)]
    relations: tuple[Relation, ...]

    @model_validator(mode="after")
    def _endpoints(self) -> Self:
        abilities = {a.id for a in self.abilities}
        if len(abilities) != len(self.abilities):
            raise ValueError("Duplicate local ability")
        if len(set(self.members)) != len(self.members):
            raise ValueError("Duplicate classification member")
        keys = {(c.id, c.ability) for c in self.clauses}
        if len(keys) != len(self.clauses):
            raise ValueError("Duplicate local clause assignment")
        if any(c.ability not in abilities for c in self.clauses):
            raise ValueError("Clause references absent local ability")
        for relation in self.relations:
            for endpoint in (*relation.sources, *relation.targets):
                matches = [
                    c
                    for c in self.clauses
                    if c.id == endpoint.clause
                    and (endpoint.ability is None or c.ability == endpoint.ability)
                ]
                if len(matches) != 1:
                    raise ValueError("Relation endpoint is absent or ambiguous locally")
        return self


Record = Annotated[Classification | Member, Field(discriminator="kind")]


class Basis(RecordData):
    recipe: Code
    inputs_hash: Hash


class Shard(RecordData):
    namespace: Code
    basis: Basis
    records: Annotated[tuple[Record, ...], Field(min_length=1)]

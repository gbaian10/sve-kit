"""Read candidate classifications and validate their complete member closure."""

import re
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import ValidationError

from sve_carddb.core.authored import check_path, require_directory, shards
from sve_carddb.core.json import canonical
from sve_carddb.core.yaml import JSON_VALUE, MAX_BYTES, parse_yaml
from sve_carddb.domains.effects.classifications import (
    Basis,
    Classification,
    Member,
    Shard,
)

if TYPE_CHECKING:
    from pathlib import Path


def document(raw: bytes) -> Shard:
    """Diagnostics never echo potentially private source text from malformed inputs."""
    if len(raw) >= MAX_BYTES:
        raise ValueError("Oversized classification shard")
    try:
        return _validated(
            canonical(JSON_VALUE.validate_python(parse_yaml(raw), strict=True))
        )
    except ValidationError, ValueError, TypeError:
        raise ValueError("Invalid classification shard") from None


def _validated(content: bytes) -> Shard:
    try:
        return Shard.model_validate_json(content)
    except ValidationError:
        raise ValueError("Invalid classification shard") from None


@dataclass(frozen=True)
class Catalog:
    namespace: str
    basis: Basis
    classifications: tuple[Classification, ...]
    members: tuple[Member, ...]


def catalog(documents: tuple[Shard, ...]) -> Catalog:
    """Membership is a set equality, not a count or representative-card heuristic."""
    namespaces = {d.namespace for d in documents}
    if len(namespaces) != 1 or len({d.basis for d in documents}) != 1:
        raise ValueError(
            "Expected one nonempty classification namespace and input basis"
        )
    classifications = tuple(
        r for d in documents for r in d.records if isinstance(r, Classification)
    )
    members = tuple(r for d in documents for r in d.records if isinstance(r, Member))
    by_id = _member_index(members)
    if len({c.id for c in classifications}) != len(classifications):
        raise ValueError("Duplicate classification identity")
    claimed: set[str] = set()
    clauses: set[str] = set()
    for classification in classifications:
        if any(
            mid not in by_id or by_id[mid].template != classification.id
            for mid in classification.members
        ):
            raise ValueError("Missing member or incorrect template ownership")
        local = {c.id for mid in classification.members for c in by_id[mid].clauses}
        if any(c.id not in local for c in classification.clauses):
            raise ValueError("Classified clause is outside template member scope")
        if claimed.intersection(classification.members) or clauses.intersection(local):
            raise ValueError("Classification scopes overlap")
        claimed.update(classification.members)
        clauses.update(local)
    if claimed != by_id.keys():
        raise ValueError("Unclaimed classification members")
    if len(clauses) != sum(len(m.clauses) for m in members):
        raise ValueError("Duplicate clause identity across members")
    for classification in classifications:
        _representative(classification, by_id)
    return Catalog(next(iter(namespaces)), documents[0].basis, classifications, members)


def _representative(classification: Classification, by_id: dict[str, Member]) -> None:
    local_members = tuple(by_id[mid] for mid in classification.members)
    assigned = {c.id for c in classification.clauses}
    representatives = [
        m for m in local_members if {c.id for c in m.clauses} == assigned
    ]
    if len(representatives) != 1:
        raise ValueError("Expected one complete representative member")
    sequence = tuple(c.template for c in representatives[0].clauses)
    if any(tuple(c.template for c in m.clauses) != sequence for m in local_members):
        raise ValueError("Member clause template sequences differ")


def _member_index(members: tuple[Member, ...]) -> dict[str, Member]:
    by_id = {m.id: m for m in members}
    if len(by_id) != len(members):
        raise ValueError("Duplicate member identity")
    if len({m.position for m in members}) != len(members):
        raise ValueError("Duplicate source position in classification namespace")
    for member in members:
        block = by_id.get(member.block)
        if block is None or (
            block.position.model_copy(update={"line": member.position.line})
            != member.position
            or block.position.line > member.position.line
            or block.token_header_hash != member.token_header_hash
        ):
            raise ValueError("Member block is outside its source owner scope")
    return by_id


def load(authored: Path) -> Catalog:
    """Explicit admission keeps candidates out of snapshot and runtime consumers."""
    directory = authored / "rules/classifications"
    require_directory(authored, directory)
    for path in directory.rglob("*"):
        check_path(authored, path)
        name = path.relative_to(directory).as_posix()
        pattern = (
            r"[A-Za-z0-9_-]+" if path.is_dir() else r"[A-Za-z0-9_-]+/[0-9]{3,}\.yaml"
        )
        if re.fullmatch(pattern, name) is None:
            raise ValueError("Unsupported classification shard path")
    return catalog(
        tuple(
            _validated(content)
            for _, _, content in shards(authored, ("rules/classifications",))
        )
    )

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
from sve_carddb.domains.effects.storage import (
    SourceShard,
    SourceText,
    StoredClassification,
    StoredMember,
    StoredShard,
    TextVersionMismatchError,
)

if TYPE_CHECKING:
    from pathlib import Path


def document(raw: bytes, sources: tuple[SourceShard, ...] = ()) -> Shard:
    """Decode readable evidence without exposing wording in validation diagnostics."""
    stored = _validated(_content(raw))
    return _resolve(stored, sources)


def source_document(raw: bytes) -> SourceShard:
    """Invalid text versions report no source wording."""
    return _source_validated(_content(raw, source=True))


def _content(raw: bytes, *, source: bool = False) -> bytes:
    kind = "classification source shard" if source else "classification shard"
    if len(raw) >= MAX_BYTES:
        raise ValueError("Oversized " + kind)
    try:
        return canonical(JSON_VALUE.validate_python(parse_yaml(raw), strict=True))
    except ValueError, TypeError:
        raise ValueError("Invalid " + kind) from None


def _validated(content: bytes) -> StoredShard:
    try:
        return StoredShard.model_validate_json(content)
    except ValidationError:
        raise ValueError("Invalid classification shard") from None


def _source_validated(content: bytes) -> SourceShard:
    try:
        return SourceShard.model_validate_json(content)
    except ValidationError as error:
        if any(
            isinstance(detail.get("ctx", {}).get("error"), TextVersionMismatchError)
            for detail in error.errors(include_input=False)
        ):
            raise ValueError("Classification text version hash mismatch") from None
        raise ValueError("Invalid classification source shard") from None


def _resolve(stored: StoredShard, sources: tuple[SourceShard, ...]) -> Shard:
    texts: dict[tuple[str, str], SourceText] = {}
    for source in sources:
        if source.namespace != stored.namespace or source.basis != stored.basis:
            raise ValueError("Classification source namespace or input basis differs")
        for text in source.records:
            key = (source.owner, text.id)
            if key in texts:
                raise ValueError("Duplicate classification text version")
            texts[key] = text
    records: list[Classification | Member] = []
    for record in stored.records:
        resolved_text = (
            texts.get((record.owner, record.source))
            if isinstance(record, StoredMember)
            else None
        )
        if isinstance(record, StoredMember) and resolved_text is None:
            raise ValueError("Missing classification text version")
        try:
            if isinstance(record, StoredMember) and resolved_text is not None:
                records.append(record.resolve(resolved_text))
            elif isinstance(record, StoredClassification):
                records.append(record.resolve())
        except ValidationError:
            raise ValueError("Invalid resolved classification record") from None
    return Shard(
        namespace=stored.namespace,
        basis=stored.basis,
        records=tuple(records),
    )


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


def _check_area(authored: Path, area: str) -> None:
    directory = authored / area
    require_directory(authored, directory)
    for path in directory.rglob("*"):
        check_path(authored, path)
        name = path.relative_to(directory).as_posix()
        pattern = (
            r"[A-Za-z0-9_-]+" if path.is_dir() else r"[A-Za-z0-9_-]+/[0-9]{3,}\.yaml"
        )
        if re.fullmatch(pattern, name) is None:
            raise ValueError("Unsupported classification shard path")


def load(authored: Path) -> Catalog:
    """Explicit admission keeps candidates out of snapshot and runtime consumers."""
    _check_area(authored, "rules/classifications")
    stored = tuple(
        (name, _validated(content))
        for name, _, content in shards(authored, ("rules/classifications",))
    )
    _check_area(authored, "rules/classification-sources")
    sources = []
    for name, _, content in shards(authored, ("rules/classification-sources",)):
        source = _source_validated(content)
        if name.split("/")[2] != source.owner:
            raise ValueError("Classification text owner differs from shard path")
        sources.append(source)
    used = {
        (record.owner, record.source)
        for _, shard in stored
        for record in shard.records
        if isinstance(record, StoredMember)
    }
    available = {
        (source.owner, text.id) for source in sources for text in source.records
    }
    documents = []
    for name, shard in stored:
        if any(
            isinstance(record, StoredMember) and record.owner != name.split("/")[2]
            for record in shard.records
        ):
            raise ValueError("Classification member owner differs from shard path")
        documents.append(_resolve(shard, tuple(sources)))
    if available - used:
        raise ValueError("Unreferenced classification text version")
    return catalog(tuple(documents))

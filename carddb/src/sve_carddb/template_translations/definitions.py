"""Semantic identity and family validation for current template definitions."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.analysis import VERSION_PARAMETERS
from sve_carddb.template_sources.flavor import VERSION as FLAVOR_VERSION
from sve_carddb.template_translations.current_models import DefinitionRecord
from sve_carddb.template_translations.flavor_models import FlavorEntry

if TYPE_CHECKING:
    from sve_carddb.template_translations.members import Reconstructed
DefinitionLike = DefinitionRecord
LEGACY_WIDTH = 10


def payload(member: Reconstructed, record: DefinitionLike) -> bytes:
    """Source locators are provenance; the six semantic fields identify content."""
    data = record.data
    return canonical(
        {
            "level": member.entry.level,
            "source_lang": data.source_lang,
            "normalized_text": member.normalized,
            "normalizer_version": data.normalizer_version,
            "semantic_variant": data.semantic_variant,
            "parameter_schema": data.parameter_schema.model_dump(mode="json"),
        }
    )


def _definitions(
    records: tuple[tuple[DefinitionRecord, str | None], ...],
    members: dict[str, Reconstructed],
) -> tuple[
    dict[str, DefinitionLike],
    tuple[tuple[str, int], ...],
    tuple[tuple[str, str], ...],
]:
    definitions = {}
    legacy: dict[str, str] = {}
    for member in members.values():
        identifier = member.candidate.legacy_id
        if identifier is not None:
            checksum = digest(member.normalized.encode())
            if identifier in legacy and legacy[identifier] != checksum:
                raise ValueError(
                    "Legacy template fingerprint collision across the full inventory"
                )
            legacy[identifier] = checksum
    payloads: dict[str, bytes] = {}
    allocations: dict[str, str] = {}
    matches: dict[str, tuple[str, ...]] = {}
    for record, _ in records:
        if not isinstance(record, DefinitionRecord):
            continue
        representative, content, old = _definition(record, members)
        data = record.data
        if data.content_hash in payloads and payloads[data.content_hash] != content:
            raise ValueError("Template full payload hash collision")
        if (
            data.content_hash in allocations
            and allocations[data.content_hash] != data.id
        ):
            raise ValueError("Template payload hash must have exactly one allocated ID")
        payloads[data.content_hash] = content
        allocations[data.content_hash] = data.id
        matches[data.id] = _matching_members(representative, record, members, old=old)
        definitions[data.id] = record
    unadopted = _parent_chains(definitions, members)
    return definitions, _current_frequencies(definitions, matches), unadopted


def _current_frequencies(
    definitions: dict[str, DefinitionLike], matches: dict[str, tuple[str, ...]]
) -> tuple[tuple[str, int], ...]:
    retired = {r.data.supersedes_id for r in definitions.values()}
    claimed: dict[str, str] = {}
    frequencies = []
    for identifier, ids in matches.items():
        if identifier in retired:
            continue
        for member_id in ids:
            if member_id in claimed:
                raise ValueError(
                    "Template source member has multiple current definitions"
                )
            claimed[member_id] = identifier
        frequencies.append((identifier, len(ids)))
    return tuple(sorted(frequencies, key=lambda p: (-p[1], p[0])))


def _definition(
    record: DefinitionLike, members: dict[str, Reconstructed]
) -> tuple[Reconstructed, bytes, bool]:
    data = record.data
    representative = members.get(data.inventory_id)
    if representative is None:
        raise ValueError("Template definition references an absent inventory entry")
    if (
        data.source_lang != "ja"
        or data.normalizer_version
        != (
            FLAVOR_VERSION
            if isinstance(representative.entry, FlavorEntry)
            else VERSION_PARAMETERS
        )
        or data.semantic_variant != "default"
    ):
        raise ValueError(
            "Template definition language normalizer or semantic variant is unsupported"
        )
    if data.source_span != representative.candidate.source_span:
        raise ValueError("Template definition span must match its exact inventory part")
    representative.verify_schema(data.parameter_schema)
    content = payload(representative, record)
    if digest(content) != data.content_hash:
        raise ValueError(
            "Template definition content hash differs from its six-field payload"
        )
    old = len(data.id.removeprefix("T")) == LEGACY_WIDTH
    if (old and data.id != representative.candidate.legacy_id) or (
        not old and data.id != "T" + data.content_hash[7 : 7 + len(data.id) - 1]
    ):
        raise ValueError(
            "Template ID differs from its legacy fingerprint or allocated payload hash"
        )
    return representative, content, old


def _matching_members(
    representative: Reconstructed,
    record: DefinitionLike,
    members: dict[str, Reconstructed],
    *,
    old: bool,
) -> tuple[str, ...]:
    data = record.data
    family = [
        (identifier, m)
        for identifier, m in members.items()
        if m.normalized == representative.normalized
        and m.entry.role == representative.entry.role
    ]
    matched = []
    for identifier, member in family:
        try:
            member.verify_schema(data.parameter_schema)
        except ValueError:
            if old:
                raise ValueError(
                    "Legacy template members require one fully resolved schema"
                ) from None
            continue
        if member.role_signature() != representative.role_signature():
            if old:
                raise ValueError(
                    "Legacy template members disagree on slot semantic roles"
                )
            continue
        matched.append(identifier)
    return tuple(matched)


def _parent_chains(
    definitions: dict[str, DefinitionLike], members: dict[str, Reconstructed]
) -> tuple[tuple[str, str], ...]:
    unadopted = []
    for record in definitions.values():
        representative = members[record.data.inventory_id]
        direct = record.data.supersedes_id
        if direct is not None and direct not in definitions:
            if direct != representative.candidate.legacy_id:
                raise ValueError(
                    "Template supersedes requires an adopted parent or its verified legacy family"
                )
            unadopted.append((record.data.id, direct))
        elif direct is not None and direct != record.data.id:
            parent_member = members[definitions[direct].data.inventory_id]
            same_family = representative.entry.role == parent_member.entry.role and (
                (
                    representative.candidate.legacy_id is not None
                    and representative.candidate.legacy_id
                    == parent_member.candidate.legacy_id
                )
                or representative.normalized == parent_member.normalized
            )
            if not same_family:
                raise ValueError(
                    "Template supersedes adopted parent belongs to another source family"
                )
        seen = {record.data.id}
        parent = record.data.supersedes_id
        while parent is not None:
            if parent in seen:
                raise ValueError("Template supersedes chain must not contain a cycle")
            if parent not in definitions:
                break
            seen.add(parent)
            parent = definitions[parent].data.supersedes_id
    return tuple(sorted(unadopted))

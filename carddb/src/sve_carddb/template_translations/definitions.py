"""Semantic identity and source matching for current template definitions."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.analysis import VERSION_PARAMETERS

if TYPE_CHECKING:
    from sve_carddb.template_translations.current_models import DefinitionRecord
    from sve_carddb.template_translations.members import Reconstructed


def payload(member: Reconstructed, record: DefinitionRecord) -> bytes:
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
    records: tuple[DefinitionRecord, ...], members: dict[str, Reconstructed]
) -> tuple[
    dict[str, DefinitionRecord],
    dict[str, tuple[str, ...]],
    tuple[tuple[str, int], ...],
]:
    definitions = {}
    payloads: dict[str, bytes] = {}
    allocations: dict[str, str] = {}
    matches: dict[str, tuple[str, ...]] = {}
    for record in records:
        representative, content = _definition(record, members)
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
        matches[data.id] = _matching_members(representative, record, members)
        definitions[data.id] = record
    return definitions, matches, _frequencies(matches)


def _frequencies(
    matches: dict[str, tuple[str, ...]],
) -> tuple[tuple[str, int], ...]:
    claimed: dict[str, str] = {}
    frequencies = []
    for identifier, ids in matches.items():
        for member_id in ids:
            if member_id in claimed:
                raise ValueError(
                    "Template source member has multiple current definitions"
                )
            claimed[member_id] = identifier
        frequencies.append((identifier, len(ids)))
    return tuple(sorted(frequencies, key=lambda p: (-p[1], p[0])))


def _definition(
    record: DefinitionRecord, members: dict[str, Reconstructed]
) -> tuple[Reconstructed, bytes]:
    data = record.data
    representative = members.get(data.inventory_id)
    if representative is None:
        raise ValueError("Template definition references an absent inventory entry")
    if (
        data.source_lang != "ja"
        or data.normalizer_version != VERSION_PARAMETERS
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
    if data.id != "T" + data.content_hash[7 : 7 + len(data.id) - 1]:
        raise ValueError("Template ID differs from its allocated payload hash")
    return representative, content


def _matching_members(
    representative: Reconstructed,
    record: DefinitionRecord,
    members: dict[str, Reconstructed],
) -> tuple[str, ...]:
    """Equal text with another schema or slot role stays unmatched, not merged."""
    schema = record.data.parameter_schema
    signature = representative.role_signature()
    matched = []
    for identifier, member in members.items():
        if (
            member.normalized != representative.normalized
            or member.entry.role != representative.entry.role
        ):
            continue
        try:
            member.verify_schema(schema)
        except ValueError:
            continue
        if member.role_signature() == signature:
            matched.append(identifier)
    return tuple(matched)

"""Semantic identity and source matching for current template definitions."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.translations.parameters.analysis import VERSION_PARAMETERS

if TYPE_CHECKING:
    from sve_carddb.domains.translations.templates.members import Reconstructed
    from sve_carddb.domains.translations.templates.records import DefinitionRecord

type Groups = dict[tuple[str, str], list[Reconstructed]]


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


def groups(members: dict[str, Reconstructed]) -> Groups:
    """Definitions bind to a normalized pattern and role, not to one source position."""
    result: Groups = {}
    for member in members.values():
        key = (member.candidate.template_normalized_hash, member.entry.role)
        result.setdefault(key, []).append(member)
    return result


def _definitions(
    records: tuple[DefinitionRecord, ...], patterns: Groups
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
        matched, content = _definition(record, patterns)
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
        matches[data.id] = tuple(member.entry.id for member in matched)
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
    record: DefinitionRecord, patterns: Groups
) -> tuple[tuple[Reconstructed, ...], bytes]:
    data = record.data
    group = patterns.get((data.normalized_hash, data.role))
    if not group:
        raise ValueError("Template definition pattern has no current source position")
    if (
        data.source_lang != "ja"
        or data.normalizer_version != VERSION_PARAMETERS
        or data.semantic_variant != "default"
    ):
        raise ValueError(
            "Template definition language normalizer or semantic variant is unsupported"
        )
    matched = _matching_members(record, group)
    if not matched:
        # Re-run one check so the build reports why the schema fits no position.
        group[0].verify_schema(data.parameter_schema)
    if len({member.role_signature() for member in matched}) != 1:
        raise ValueError(
            "Template definition positions disagree on slot semantic roles"
        )
    content = payload(matched[0], record)
    if digest(content) != data.content_hash:
        raise ValueError(
            "Template definition content hash differs from its six-field payload"
        )
    if data.id != "T" + data.content_hash[7 : 7 + len(data.id) - 1]:
        raise ValueError("Template ID differs from its allocated payload hash")
    return matched, content


def _matching_members(
    record: DefinitionRecord, group: list[Reconstructed]
) -> tuple[Reconstructed, ...]:
    """Equal text with another schema stays unmatched, not merged."""
    matched = []
    for member in group:
        try:
            member.verify_schema(record.data.parameter_schema)
        except ValueError:
            continue
        matched.append(member)
    return tuple(matched)

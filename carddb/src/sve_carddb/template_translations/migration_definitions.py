"""Derive compatible current definitions while preserving every existing semantic ID."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.analysis import SAFE_INTEGER, VERSION_PARAMETERS
from sve_carddb.template_parameters.models import Schema, Slot
from sve_carddb.template_sources.flavor import VERSION as FLAVOR_VERSION
from sve_carddb.template_translations.current import LEGACY_ID_LENGTH
from sve_carddb.template_translations.current_models import DefinitionRecord
from sve_carddb.template_translations.flavor_models import FlavorEntry
from sve_carddb.template_translations.loader import (
    _definitions,
    _matching_members,
    payload,
)
from sve_carddb.template_translations.models import Definition
from sve_carddb.template_translations.sources import ORDINALS

FULL_HASH_WIDTH = 64

if TYPE_CHECKING:
    from sve_carddb.template_translations.sources import Reconstructed


@dataclass(frozen=True)
class Definitions:
    records: tuple[DefinitionRecord, ...]
    representatives: tuple[tuple[str, str], ...]
    issues: tuple[tuple[str, tuple[str, ...]], ...]


def schema(member: Reconstructed, family: tuple[Reconstructed, ...] = ()) -> Schema:
    """Each recognized source position keeps its identity until equality is verified."""
    if member.pending:
        raise ValueError("definition_source_pending")
    value = Schema(
        slots=tuple(
            Slot(
                name=hint.name,
                type=hint.type or "literal",
                occurrences=(hint.occurrence,),
                reference_kind=hint.reference_kind,
                min=(1 if role in ORDINALS else 0) if hint.type == "uint" else None,
                max=SAFE_INTEGER if hint.type == "uint" else None,
            )
            for hint, role in zip(member.hints, member.roles, strict=True)
        )
    )
    if family and all(not item.pending for item in family):
        value = _merge_equal_slots(value, family)
    for item in family or (member,):
        if not item.pending:
            item.verify_schema(value)
    return value


def _merge_equal_slots(value: Schema, family: tuple[Reconstructed, ...]) -> Schema:
    groups: dict[bytes, list[int]] = {}
    separate = []
    for index, slot in enumerate(value.slots):
        if slot.type == "literal":
            separate.append(slot)
            continue
        identities: list[JsonValue] = [
            [
                item.roles[index],
                item.hints[index].numeric_rule,
                item.hints[index].value
                if slot.type == "uint"
                else item.hints[index].target,
            ]
            for item in family
        ]
        identity = canonical(
            [slot.type, slot.reference_kind, slot.min, slot.max, identities]
        )
        groups.setdefault(identity, []).append(index)
    merged = [
        value.slots[positions[0]].model_copy(
            update={
                "occurrences": tuple(
                    occurrence
                    for index in positions
                    for occurrence in value.slots[index].occurrences
                )
            }
        )
        for positions in groups.values()
    ]
    return Schema(
        slots=tuple(
            sorted((*merged, *separate), key=lambda slot: slot.occurrences[0].start)
        )
    )


def record(member: Reconstructed, parameters: Schema) -> DefinitionRecord:
    """The semantic hash excludes physical locator, target wording and notes."""
    data = Definition(
        id="T" + "0" * 16,
        inventory_id=member.entry.id,
        source_span=member.candidate.source_span,
        source_lang="ja",
        normalizer_version=FLAVOR_VERSION
        if isinstance(member.entry, FlavorEntry)
        else VERSION_PARAMETERS,
        semantic_variant="default",
        parameter_schema=parameters,
        content_hash="sha256:" + "0" * 64,
        supersedes_id=member.candidate.legacy_id,
    )
    typed = DefinitionRecord(
        record_key="pending",
        kind="sentence_template",
        data=data,
        origin="project",
        low_confidence=False,
        note="",
    )
    checksum = digest(payload(member, typed))
    identifier = "T" + checksum[7:23]
    return typed.model_copy(
        update={
            "record_key": canonical([typed.kind, identifier]).decode(),
            "data": data.model_copy(
                update={"id": identifier, "content_hash": checksum}
            ),
        }
    )


def derive(  # ruff: ignore[complex-structure,too-many-locals] -- existing matches and distinct semantic collision checks must precede allocation
    existing: tuple[DefinitionRecord, ...], members: tuple[Reconstructed, ...]
) -> Definitions:
    """Current definitions take priority over new drafts, including their slot names."""
    actual = {member.entry.id: member for member in members}
    if len(actual) != len(members):
        raise ValueError("migration_duplicate_source_identity")
    verified, _, _ = _definitions(tuple((r, None) for r in existing), actual)
    definitions = {r.data.id: r for r in existing}
    representative = {r.data.id: r.data.inventory_id for r in existing}
    retired = {r.data.supersedes_id for r in existing}
    assigned = {}
    contents = {
        r.data.content_hash: payload(actual[r.data.inventory_id], r) for r in existing
    }
    hashes = {r.data.content_hash: r.data.id for r in existing}
    signatures = {
        r.data.id: actual[r.data.inventory_id].role_signature() for r in existing
    }
    for identifier, current in verified.items():
        if identifier not in retired:
            for match in _matching_members(
                actual[current.data.inventory_id],
                current,
                actual,
                old=len(identifier) == LEGACY_ID_LENGTH,
            ):
                assigned[match] = identifier
    families: dict[tuple[str, str, bytes], list[Reconstructed]] = {}
    for member in members:
        families.setdefault(
            (member.normalized, member.entry.role, member.role_signature()), []
        ).append(member)
    schemas: dict[tuple[str, str, bytes], Schema] = {}
    issues = []
    for member in sorted(members, key=lambda m: m.entry.id):
        if member.entry.id in assigned:
            continue
        if member.pending:
            issues.append((member.entry.id, member.pending))
            continue
        family_key = member.normalized, member.entry.role, member.role_signature()
        if family_key not in schemas:
            schemas[family_key] = schema(member, tuple(families[family_key]))
        current = record(member, schemas[family_key])
        content = payload(member, current)
        checksum = current.data.content_hash
        known = hashes.get(checksum)
        if known is not None:
            if contents[checksum] != content:
                raise ValueError("migration_full_payload_hash_collision")
            if (
                signatures[known] != member.role_signature()
                or actual[representative[known]].entry.role != member.entry.role
            ):
                issues.append((member.entry.id, ("definition_role_conflict",)))
                continue
            assigned[member.entry.id] = known
            continue
        identifier = _allocate(current, definitions)
        current = current.model_copy(
            update={
                "record_key": canonical([current.kind, identifier]).decode(),
                "data": current.data.model_copy(update={"id": identifier}),
            }
        )
        definitions[identifier] = current
        representative[identifier] = member.entry.id
        hashes[checksum] = identifier
        contents[checksum] = content
        signatures[identifier] = member.role_signature()
        assigned[member.entry.id] = identifier
    return Definitions(
        tuple(definitions[k] for k in sorted(definitions)),
        tuple(sorted(representative.items())),
        tuple(sorted(issues)),
    )


def _allocate(
    current: DefinitionRecord, definitions: dict[str, DefinitionRecord]
) -> str:
    width = 16
    checksum = current.data.content_hash
    while width <= FULL_HASH_WIDTH:
        identifier = "T" + checksum[7 : 7 + width]
        previous = definitions.get(identifier)
        if previous is None or previous.data.content_hash == checksum:
            return identifier
        width += 2
    raise ValueError("migration_template_id_collision")

"""Read editable template values without raw reconstruction or adoption-history gates."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import JsonValue, ValidationError

from sve_carddb.snapshot.values import canonical, object_value, parse
from sve_carddb.template_translations.current_models import (
    DefinitionRecord,
    Inventory,
    Record,
    Shard,
    TranslationRecord,
    VariantRecord,
)
from sve_carddb.template_translations.files import INVENTORY, SHARD, Files, read
from sve_carddb.template_translations.loader import (
    _definitions,
    _inventory,
    _matching_members,
    _shard,
)
from sve_carddb.template_translations.models import DefinitionRecord as LegacyDefinition
from sve_carddb.template_translations.models import (
    TranslationRecord as LegacyTranslation,
)
from sve_carddb.template_translations.text import parse as parse_text
from sve_carddb.template_translations.text import verify_flavor
from sve_carddb.translations.loader import Snapshot as Glossary
from sve_carddb.translations.loader import validate_snapshot

LEGACY_ID_LENGTH = 11
CURRENT_FORMAT = 2
CURRENT_INVENTORY_FORMAT = 3

if TYPE_CHECKING:
    from sve_carddb.catalog.adoption_sources import PinnedRepository
    from sve_carddb.template_translations.current_sources import Sources
    from sve_carddb.template_translations.sources import Reconstructed


def key(record: Record) -> str:
    """Current selection keys exclude translated text, notes and adoption metadata."""
    if isinstance(record, VariantRecord):
        return canonical(
            [
                record.kind,
                record.data.template_id,
                record.data.lang,
                record.data.variant_key,
            ]
        ).decode()
    return canonical(
        [record.kind, record.data.id]
        if isinstance(record, DefinitionRecord)
        else [record.kind, record.data.template_id, record.data.lang]
    ).decode()


def shard(raw: bytes) -> Shard:
    """Closed current records do not permit stray old receipts or private evidence fields."""
    try:
        result = Shard.model_validate_json(raw)
    except ValidationError:
        raise ValueError("Invalid current template shard") from None
    keys = tuple(record.record_key for record in result.records)
    if keys != tuple(sorted(set(keys))) or any(
        record.record_key != key(record) for record in result.records
    ):
        raise ValueError("Current template selection keys must be unique and exact")
    return result


def inventory(raw: bytes) -> Inventory:
    """Format three describes only current source batches and positions."""
    try:
        return Inventory.model_validate_json(raw)
    except ValidationError:
        raise ValueError("Invalid current template inventory") from None


def migrate_shard(raw: bytes) -> Shard:
    """Keep the latest actual values; removing receipts does not upgrade machine origin."""
    previous = _shard(raw)
    selected: dict[str, LegacyDefinition | LegacyTranslation] = {}
    for record in previous.records:
        name = canonical(
            [record.kind, record.data.id]
            if isinstance(record, LegacyDefinition)
            else [record.kind, record.data.template_id, record.data.lang]
        ).decode()
        existing = selected.get(name)
        if existing is not None and isinstance(record, LegacyDefinition):
            raise ValueError("Duplicate legacy template definition")
        if existing is None or (
            isinstance(record, LegacyTranslation)
            and isinstance(existing, LegacyTranslation)
            and record.data.revision > existing.data.revision
        ):
            selected[name] = record
    records: list[JsonValue] = []
    for name, record in sorted(selected.items()):
        data = object_value(parse(canonical(record.data.model_dump(mode="json"))))
        origin = "project"
        if isinstance(record, LegacyTranslation):
            data = {k: data[k] for k in ("template_id", "lang", "text")}
            origin = record.data.origin
        records.append(
            {
                "record_key": name,
                "kind": record.kind,
                "data": data,
                "origin": origin,
                "low_confidence": False,
                "note": "",
            }
        )
    return shard(
        canonical(
            {
                "translation_authored_format": 2,
                "kind": "translation_shard",
                "records": records,
            }
        )
    )


def validate_foreign(path: str, raw: bytes) -> None:
    """Glossary readers may validate template shapes without reading frozen sources."""
    value = object_value(parse(raw))
    if path.startswith("translations/template-sources/"):
        if value.get("template_source_format") == CURRENT_INVENTORY_FORMAT:
            inventory(raw)
        else:
            _inventory(raw)
    elif value.get("translation_authored_format") == CURRENT_FORMAT:
        shard(raw)
    else:
        _shard(raw)


@dataclass(frozen=True)
class Inputs:
    files: Files
    glossary: Glossary
    records: tuple[Record, ...]
    inventories: tuple[Inventory, ...]

    def translations(self) -> tuple[TranslationRecord, ...]:
        """Low confidence affects presentation, not structural eligibility."""
        return tuple(
            record for record in self.records if isinstance(record, TranslationRecord)
        )


def from_files(files: Files) -> Inputs:
    """A current-tree snapshot never becomes a successful build just by parsing."""
    glossary = Glossary(
        files.index,
        tuple(
            f
            for f in files.content
            if f[0].startswith(("translations/glossary/", "translations/overrides/"))
        ),
        files.content,
    )
    validate_snapshot(glossary)
    records, inventories, entries = _collect(files)
    _references(records, entries)
    return Inputs(
        files, glossary, tuple(records[k] for k in sorted(records)), tuple(inventories)
    )


def _collect(files: Files) -> tuple[dict[str, Record], list[Inventory], set[str]]:
    records: dict[str, Record] = {}
    inventories = []
    entries = set()
    for path, _, raw in files.content:
        if INVENTORY.fullmatch(path):
            current = inventory(raw)
            for entry in current.entries:
                if entry.id in entries:
                    raise ValueError("Duplicate current template inventory entry")
                entries.add(entry.id)
            inventories.append(current)
        elif SHARD.fullmatch(path) and path.startswith("translations/templates/"):
            for record in shard(raw).records:
                if record.record_key in records:
                    raise ValueError("Duplicate current template selection key")
                records[record.record_key] = record
    return records, inventories, entries


def _references(records: dict[str, Record], entries: set[str]) -> None:
    definitions = {
        record.data.id: record
        for record in records.values()
        if isinstance(record, DefinitionRecord)
    }
    payloads: dict[str, str] = {}
    for identifier, definition in definitions.items():
        data = definition.data
        if data.inventory_id not in entries:
            raise ValueError(
                "Current template definition references an absent inventory entry"
            )
        if data.content_hash in payloads and payloads[data.content_hash] != identifier:
            raise ValueError("Template payload hash must have exactly one allocated ID")
        payloads[data.content_hash] = identifier
        visited = {identifier}
        parent = data.supersedes_id
        while parent in definitions:
            if parent in visited:
                raise ValueError("Template supersedes chain must not contain a cycle")
            visited.add(parent)
            parent = definitions[parent].data.supersedes_id
    _texts(records, definitions)


def _texts(
    records: dict[str, Record], definitions: dict[str, DefinitionRecord]
) -> None:
    for record in records.values():
        if isinstance(record, (TranslationRecord, VariantRecord)):
            target = definitions.get(record.data.template_id)
            if target is None:
                raise ValueError("Current template translation requires a definition")
            if record.data.lang == target.data.source_lang:
                raise ValueError(
                    "Template translation language must differ from its source"
                )
            parse_text(record.data.text, target.data.parameter_schema)
            if target.data.source_span.role == "flavor":
                verify_flavor(record.data.text, target.data.parameter_schema)


def read_templates(repository: PinnedRepository, revision: str) -> Inputs:
    """Read only current Git blobs; source/owner checks belong to full build validation."""
    return from_files(read(repository, revision))


@dataclass(frozen=True)
class Validated:
    inputs: Inputs
    frequencies: tuple[tuple[str, int], ...]
    unadopted_parents: tuple[tuple[str, str], ...]
    unmatched_entries: tuple[str, ...]
    missing_translations: tuple[str, ...]
    low_confidence: tuple[str, ...]
    source_report: bytes
    members: tuple[Reconstructed, ...]
    matches: tuple[tuple[str, str], ...]


def validate_templates(inputs: Inputs, sources: Sources) -> Validated:
    """One current build verifies every declared source before exposing usable definitions."""
    batches = {
        (batch.store_id, batch.batch_id): batch
        for item in inputs.inventories
        for batch in item.source_batches
    }
    generated = sources.generate(tuple(batches[key] for key in sorted(batches)))
    actual = {member.entry.id: member for member in generated.entries}
    declared = {
        entry.id: entry for item in inputs.inventories for entry in item.entries
    }
    if set(actual) != set(declared):
        raise ValueError(
            "Current template inventory must cover every source batch entry"
        )
    if any(declared[key] != member.entry for key, member in actual.items()):
        raise ValueError(
            "Current template inventory differs from its regenerated source"
        )
    definitions, frequencies, parents = _definitions(
        tuple(
            (record, None)
            for record in inputs.records
            if isinstance(record, DefinitionRecord)
        ),
        actual,
    )
    retired = {record.data.supersedes_id for record in definitions.values()}
    matched: set[str] = set()
    matches: list[tuple[str, str]] = []
    for identifier, record in definitions.items():
        if identifier in retired:
            continue
        member = actual[record.data.inventory_id]
        identifiers = _matching_members(
            member, record, actual, old=len(identifier) == LEGACY_ID_LENGTH
        )
        matched.update(identifiers)
        matches.extend((entry_id, identifier) for entry_id in identifiers)
    translated = {record.data.template_id for record in inputs.translations()}
    missing = set(definitions) - retired - translated
    return Validated(
        inputs,
        frequencies,
        parents,
        tuple(sorted(set(actual) - matched)),
        tuple(sorted(missing)),
        tuple(
            sorted(
                record.record_key for record in inputs.records if record.low_confidence
            )
        ),
        generated.report,
        generated.entries,
        tuple(sorted(matches)),
    )

"""Validate every source, adopted definition and translation before selecting revisions."""

from collections import defaultdict
from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import TypeAdapter, ValidationError

from sve_carddb.catalog.adoption_loader import ordered
from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.template_parameters.analysis import VERSION_PARAMETERS
from sve_carddb.template_sources.flavor import VERSION as FLAVOR_VERSION
from sve_carddb.template_translations.files import (
    INVENTORY,
    SHARD,
    immutable,
    json_bytes,
    read,
)
from sve_carddb.template_translations.flavor_models import FlavorEntry
from sve_carddb.template_translations.models import (
    DefinitionRecord,
    Inventory,
    Record,
    Shard,
    TranslationRecord,
)
from sve_carddb.template_translations.replay_models import InventoryV2
from sve_carddb.template_translations.review import require_resolved_dispute, verify
from sve_carddb.template_translations.text import parse, verify_flavor
from sve_carddb.translations.loader import Snapshot as Glossary
from sve_carddb.translations.loader import validate_snapshot

if TYPE_CHECKING:
    from sve_carddb.template_translations.files import Files
    from sve_carddb.template_translations.sources import Reconstructed, TemplateSources


LEGACY_WIDTH = 10
INVENTORY_V2_FORMAT = 2
INVENTORY_WIRE: TypeAdapter[Inventory | InventoryV2] = TypeAdapter(
    Inventory | InventoryV2
)


def record_hash(record: Record) -> str:
    """Evidence participates in immutable record membership."""
    return digest(canonical(record.model_dump(mode="json")))


def key(record: Record) -> str:
    """Primary keys do not depend on filing order or translated text."""
    if isinstance(record, DefinitionRecord):
        return canonical([record.kind, record.data.id]).decode()
    return canonical(
        [record.kind, record.data.template_id, record.data.lang, record.data.revision]
    ).decode()


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


@dataclass(frozen=True)
class Snapshot:
    revision: str
    index: bytes
    shards: tuple[tuple[str, bytes, bytes], ...]
    inventories: tuple[tuple[str, bytes, bytes], ...]
    glossary: Glossary
    source_reports: tuple[tuple[bytes, bytes, bytes], ...]
    frequencies: tuple[tuple[str, int], ...]
    unadopted_parents: tuple[tuple[str, str], ...]

    def database_parent(self, template_id: str) -> str | None:
        """Keep legacy provenance without inventing an unadopted parent FK."""
        for record, _ in self.records():
            if isinstance(record, DefinitionRecord) and record.data.id == template_id:
                if any(child == template_id for child, _ in self.unadopted_parents):
                    return None
                return record.data.supersedes_id
        raise ValueError("Unknown adopted template definition")

    def envelopes(self) -> tuple[Shard, ...]:
        """Return detached wire models rather than mutable verified state."""
        return tuple(
            Shard.model_validate_json(content) for _, _, content in self.shards
        )

    def records(self) -> tuple[tuple[Record, str], ...]:
        """Keep every immutable revision and its exact decision."""
        return tuple(
            (r, s.default_decision_id) for s in self.envelopes() for r in s.records
        )

    def effective_translations(self) -> tuple[TranslationRecord, ...]:
        """Project terminal revisions only after the complete history was validated."""
        latest: dict[tuple[str, str], TranslationRecord] = {}
        for record, _ in self.records():
            if isinstance(record, TranslationRecord):
                identity = record.data.template_id, record.data.lang
                if (
                    identity not in latest
                    or latest[identity].data.revision < record.data.revision
                ):
                    latest[identity] = record
        return tuple(latest[k] for k in sorted(latest))

    def pins(self) -> dict[str, object]:
        """Distinguish canonical index hashes from exact Git byte pins."""
        return {
            "authored_revision": self.revision,
            "index_hash": digest(json_bytes(self.index)),
            "index_exact_hash": digest(self.index),
            "files": tuple(
                (p, digest(exact), digest(content))
                for p, exact, content in (*self.shards, *self.inventories)
            ),
            "source_report_hashes": tuple(
                tuple(digest(content) for content in report)
                for report in self.source_reports
            ),
        }


def _shard(raw: bytes) -> Shard:
    try:
        return Shard.model_validate_json(raw)
    except ValidationError:
        raise ValueError("Invalid adopted template shard") from None


def envelope(shard: Shard, filing: str) -> None:
    """Membership, real checked subsets and homogeneous review modes precede loading."""
    decision = shard.decisions[0]
    keys = tuple(r.record_key for r in shard.records)
    if keys != tuple(sorted(set(keys))):
        raise ValueError("Template members must be sorted and unique")
    members = tuple((r.record_key, record_hash(r)) for r in shard.records)
    checksum = digest(canonical([[k, h] for k, h in members]))
    if (
        decision.members != members
        or decision.membership_hash != checksum
        or decision.id != "d:" + checksum[7:]
        or shard.default_decision_id != decision.id
    ):
        raise ValueError("Template decision exact membership mismatch")
    if (
        not decision.sample_ids
        or decision.sample_ids != tuple(sorted(set(decision.sample_ids)))
        or not set(decision.sample_ids) <= set(keys)
    ):
        raise ValueError("Template decision requires actual sampled or checked members")
    if decision.state == "confirmed" and decision.sample_ids != keys:
        raise ValueError("Confirmed template decision must check every member")
    for record in shard.records:
        if (
            record.kind != decision.category
            or record.filing_key != filing
            or record.record_key != key(record)
        ):
            raise ValueError(
                "Template record kind key or filing differs from its batch"
            )
        ordered(record.evidence)
        if isinstance(record, TranslationRecord):
            review = record.data.adoption_review
            if review.mode != "human":
                raise ValueError(
                    "Template policy adoption requires the complete translation-policy loader"
                )
            _model_review(record, shard)


def _model_review(record: TranslationRecord, shard: Shard) -> None:
    decision = shard.decisions[0]
    model = record.data.model_review
    if model is not None:
        verify(record.data.text, model)
        sampled = (
            decision.state == "sampled" and record.record_key in decision.sample_ids
        )
        require_resolved_dispute(model, sampled=sampled)
        if model.result == "disputed":
            assert model.resolution is not None
            if (model.resolution.reviewed_by, model.resolution.reviewed_at) != (
                decision.reviewed_by,
                decision.reviewed_at,
            ):
                raise ValueError(
                    "Template dispute resolution differs from its human sample event"
                )


def _inventory(raw: bytes) -> Inventory | InventoryV2:
    try:
        inventory = INVENTORY_WIRE.validate_json(raw)
    except ValidationError:
        raise ValueError("Invalid formal template inventory") from None
    if not inventory.entries:
        raise ValueError("Formal template inventory shards cannot be empty")
    return inventory


def _inventories(  # ruff: ignore[complex-structure] -- each immutable shard is checked before per-context grouping
    files: Files, sources: TemplateSources
) -> tuple[dict[str, Reconstructed], tuple[tuple[bytes, bytes, bytes], ...]]:
    groups: dict[bytes, list[str]] = defaultdict(list)
    declared = {}
    recipes = {}
    contexts = {}
    frozen_repository = PinnedRepository(sources.repository.root)
    for path, _, raw in files.content:
        if INVENTORY.fullmatch(path) is None:
            continue
        inventory = _inventory(raw)
        if isinstance(inventory, InventoryV2):
            reread = _inventory(
                json_bytes(frozen_repository.read(files.revision, "authored/" + path))
            )
            if reread != inventory:
                raise ValueError(
                    "Template immutable inventory differs from in-memory input"
                )
            identity = inventory.group_key().encode()
            contexts[identity] = inventory.replay_context
        else:
            identity = canonical([r.model_dump(mode="json") for r in inventory.recipes])
        recipes[identity] = inventory.recipes
        for entry in inventory.entries:
            if entry.id in declared:
                raise ValueError("Duplicate immutable template inventory entry")
            declared[entry.id] = entry
            groups[identity].append(entry.id)
    reconstructed = {}
    reports = []
    for identity, ids in sorted(groups.items()):
        replay = (
            sources.reconstruct_v2(recipes[identity], contexts[identity])
            if identity in contexts
            else sources.reconstruct(recipes[identity])
        )
        expected = {m.entry.id: m for m in replay.entries}
        if set(expected) != set(ids):
            raise ValueError(
                "Formal template inventory must cover every frozen batch entry"
            )
        for identifier in ids:
            if declared[identifier] != expected[identifier].entry:
                raise ValueError(
                    "Formal template entry differs from its exact frozen replay"
                )
        reconstructed.update(expected)
        reports.append((replay.recipe, replay.source_coverage, replay.checkpoint))
    return reconstructed, tuple(reports)


def _definitions(
    records: tuple[tuple[Record, str], ...], members: dict[str, Reconstructed]
) -> tuple[
    dict[str, DefinitionRecord],
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
    for record in definitions.values():
        _allocation(record, definitions)
        allowed = {members[i].entry.source_ref for i in matches[record.data.id]}
        if any(e.source_ref not in allowed for e in record.evidence):
            raise ValueError(
                "Template definition evidence must belong to its matched family"
            )
    unadopted = _parent_chains(definitions, members)
    return definitions, _current_frequencies(definitions, matches), unadopted


def _current_frequencies(
    definitions: dict[str, DefinitionRecord], matches: dict[str, tuple[str, ...]]
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


def _allocation(
    record: DefinitionRecord, definitions: dict[str, DefinitionRecord]
) -> None:
    """Only an existing different payload at every shorter prefix permits extension."""
    data = record.data
    if len(data.id) - 1 == LEGACY_WIDTH:
        return
    for width in range(16, len(data.id) - 1, 2):
        previous = definitions.get("T" + data.content_hash[7 : 7 + width])
        if previous is None or previous.data.content_hash == data.content_hash:
            raise ValueError(
                "Extended template ID requires every shorter adopted collision"
            )


def _translations(
    records: tuple[tuple[Record, str], ...], definitions: dict[str, DefinitionRecord]
) -> None:
    chains: dict[tuple[str, str], list[TranslationRecord]] = defaultdict(list)
    for record, _ in records:
        if not isinstance(record, TranslationRecord):
            continue
        definition = definitions.get(record.data.template_id)
        if definition is None:
            raise ValueError("Template translation requires an adopted definition")
        if record.data.lang == definition.data.source_lang:
            raise ValueError(
                "Template translation language must differ from its source"
            )
        parse(record.data.text, definition.data.parameter_schema)
        if definition.data.source_span.role == "flavor":
            verify_flavor(record.data.text, definition.data.parameter_schema)
        chains[record.data.template_id, record.data.lang].append(record)
    for chain in chains.values():
        if sorted(r.data.revision for r in chain) != list(range(1, len(chain) + 1)):
            raise ValueError("Template translation revision chain has a gap or fork")


def load_templates(
    repository: PinnedRepository, authored_revision: str, sources: TemplateSources
) -> Snapshot:
    """Load a pinned complete translation index; no missing area becomes an empty set."""
    files = read(repository, authored_revision)
    immutable(repository, authored_revision)
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
    shards = tuple(
        f
        for f in files.content
        if SHARD.fullmatch(f[0]) and f[0].startswith("translations/templates/")
    )
    records = []
    seen = set()
    for path, _, content in shards:
        shard = _shard(content)
        envelope(shard, path.split("/")[2])
        for record in shard.records:
            if record.record_key in seen:
                raise ValueError("Duplicate immutable adopted template record")
            seen.add(record.record_key)
            records.append((record, shard.default_decision_id))
    members, reports = _inventories(files, sources)
    for record, _ in records:
        for evidence in record.evidence:
            sources.evidence(evidence.source_ref)
    definitions, frequencies, unadopted = _definitions(tuple(records), members)
    _translations(tuple(records), definitions)
    return Snapshot(
        authored_revision,
        files.index,
        shards,
        tuple(f for f in files.content if INVENTORY.fullmatch(f[0])),
        glossary,
        reports,
        frequencies,
        unadopted,
    )


def _definition(
    record: DefinitionRecord, members: dict[str, Reconstructed]
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


def _frequency(
    representative: Reconstructed,
    record: DefinitionRecord,
    members: dict[str, Reconstructed],
    *,
    old: bool,
) -> int:
    return len(_matching_members(representative, record, members, old=old))


def _matching_members(
    representative: Reconstructed,
    record: DefinitionRecord,
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
    definitions: dict[str, DefinitionRecord], members: dict[str, Reconstructed]
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

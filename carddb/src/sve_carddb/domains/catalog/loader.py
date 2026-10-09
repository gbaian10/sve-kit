"""Validate current vocabulary once against this build's frozen source closure."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.build import Json
from sve_carddb.build.rows import insert_exact
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog import adoption_validation as validate
from sve_carddb.domains.catalog.adoption_models import (
    Batch,
    ReviewContext,
    TextEvidence,
)
from sve_carddb.domains.catalog.adoption_sources import AdoptionSources
from sve_carddb.domains.catalog.models import Catalog
from sve_carddb.domains.catalog.projection import CatalogProjection
from sve_carddb.domains.catalog.records import LanguageRecord, Shard, VocabularyRecord
from sve_carddb.domains.text_observations.intern import TextInterner
from sve_carddb.domains.text_observations.vocabulary import Binding, Vocabulary
from sve_carddb.domains.translations.direct import write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database, Value
    from sve_carddb.core.provenance import BuildContext
    from sve_carddb.domains.catalog.adoption_loader import AdoptionSnapshot
    from sve_carddb.domains.registry.snapshot import RegistrySnapshot
    from sve_carddb.ingest.archive.frozen_sources import FrozenBatches


@dataclass(frozen=True)
class Prepared:
    snapshots: tuple[AdoptionSnapshot, ...]
    records: tuple[LanguageRecord | VocabularyRecord, ...]
    sources: AdoptionSources
    projection: CatalogProjection


def prepare(
    snapshots: tuple[AdoptionSnapshot, ...],
    repository: Path,
    build: BuildContext,
    stores: dict[str, Path],
    registry: RegistrySnapshot | None = None,
    *,
    batches: FrozenBatches | None = None,
) -> Prepared:
    """Validate mappings against this build's frozen sources."""
    loaded = tuple(r for snapshot in snapshots for r in snapshot.current_records())
    if any(
        not isinstance(record, (LanguageRecord, VocabularyRecord)) for record in loaded
    ):
        raise ValueError("Offline catalog supports vocabulary and language values")
    records = tuple(
        record
        for record in loaded
        if isinstance(record, (LanguageRecord, VocabularyRecord))
    )
    sources = AdoptionSources(stores, repository, registry, batches=batches)
    source_batches = {
        e.source_ref.batch_id
        for r in records
        for e in r.evidence
        if isinstance(e, TextEvidence)
    }
    review = ReviewContext(
        context=build,
        source_batches=tuple(Batch(batch_id=b) for b in sorted(source_batches)),
    )
    registered = {
        r.data.subject.code
        for r in records
        if isinstance(r, LanguageRecord) and r.data.value is not None
    }
    languages = []
    terms = []
    bindings: list[Binding] = []
    for record in records:
        for evidence in record.evidence:
            if isinstance(evidence, TextEvidence):
                sources.text(evidence.source_ref, review)
            else:
                sources.image(evidence)
        if isinstance(record, LanguageRecord):
            language = validate.language(record, registered)
            if language is not None:
                languages.append(language)
        elif isinstance(record, VocabularyRecord):
            term = validate.term(record, review, sources)
            if term is not None:
                terms.append(term)
                if term.active and record.data.value is not None:
                    bindings.extend(
                        Binding(
                            region=m.region,
                            kind=term.kind,
                            raw=m.raw,
                            code=term.code,
                            special_kinds=m.special_kinds,
                        )
                        for m in record.data.value.raw_mappings
                    )
    vocabulary = Vocabulary(bindings=tuple(bindings), terms=tuple(terms))
    vocabulary.verify()
    catalog = Catalog(
        languages=tuple(languages),
        terms=tuple(terms),
        aliases=(),
        symbols=(),
        names=(),
        normalizer_version="nfkc-casefold-v1",
    )
    return Prepared(snapshots, records, sources, CatalogProjection(catalog, vocabulary))


def populate(  # ruff: ignore[complex-structure,too-many-branches] -- indexed provenance and distinct vocabulary/language keys precede row writes
    db: Database,
    snapshots: tuple[AdoptionSnapshot, ...],
    prepared: Prepared,
    revision: str,
) -> None:
    """Point current values at the authored source read by this build."""
    audit: dict[str, str] = {}
    for snapshot in snapshots:
        for shard in snapshot.shards:
            identifier = (
                "authored:catalog-current:"
                + digest(canonical([revision, shard.path, digest(shard.exact)]))[7:]
            )
            insert_exact(
                db,
                "source_record",
                {
                    "id": identifier,
                    "kind": "authored",
                    "sha256": digest(shard.exact),
                    "authored_path": "authored/" + shard.path,
                    "authored_revision": revision,
                    "parser_version": "catalog-current-v3",
                },
                ("id",),
            )
            for record in Shard.model_validate_json(shard.content).records:
                audit[record.record_key] = identifier
    languages = {item.code: item for item in prepared.projection.catalog.languages}
    terms = {(item.kind, item.code): item for item in prepared.projection.catalog.terms}
    for record in prepared.records:
        metadata: dict[str, Value] = {
            "authored_source_id": audit[record.record_key],
            "record_key": record.record_key,
            "origin": record.origin,
            "low_confidence": record.low_confidence,
        }
        if isinstance(record, LanguageRecord):
            item = languages.get(record.data.subject.code)
            if item is not None:
                values: dict[str, Value] = {
                    "fallback_order": Json(list(item.fallback_order)),
                    "display_name": item.display_name,
                    **metadata,
                }
                if db.select("language", ("code",), where={"code": item.code}):
                    db.update("language", {"code": item.code}, values)
                else:
                    db.insert("language", {"code": item.code, **values})
    texts = TextInterner(db, published=())
    for record in prepared.records:
        metadata = {
            "authored_source_id": audit[record.record_key],
            "record_key": record.record_key,
            "origin": record.origin,
            "low_confidence": record.low_confidence,
        }
        if isinstance(record, VocabularyRecord):
            item_term = terms.get((record.data.subject.kind, record.data.subject.code))
            if item_term is not None:
                values = {
                    "label_unit_id": texts.intern(item_term.label),
                    "active": item_term.active,
                    **metadata,
                }
                subject = {"kind": item_term.kind, "code": item_term.code}
                if db.select("vocabulary", ("kind", "code"), where=subject):
                    db.update("vocabulary", subject, values)
                else:
                    db.insert("vocabulary", {**subject, **values})
                for label in (
                    record.data.value.translations if record.data.value else ()
                ):
                    write(
                        db,
                        {
                            "vocabulary_kind": item_term.kind,
                            "vocabulary_code": item_term.code,
                        },
                        field="label",
                        lang=label.lang,
                        source_unit_id=str(values["label_unit_id"]),
                        text=label.text,
                        origin=label.origin,
                        low_confidence=label.low_confidence,
                    )

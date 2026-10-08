"""Project current glossary data with authored provenance, never fake decisions."""

from typing import TYPE_CHECKING

from sve_carddb.build.rows import insert_exact
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.core.provenance import input_record
from sve_carddb.translations.current_models import ChoiceRecord, Shard, TermRecord
from sve_carddb.translations.importer import validate_choice
from sve_carddb.translations.sources import Sources

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.core.provenance import BuildContext, InputRecord
    from sve_carddb.translations.importer import Inputs
    from sve_carddb.translations.loader import Snapshot


def authored_sources(db: Database, snapshot: Snapshot, revision: str) -> dict[str, str]:
    """Record real indexed bytes; these rows describe inputs, not approval events."""
    result = {}
    for path, exact, content in snapshot.shards:
        identifier = (
            "authored:translations:"
            + digest(canonical([revision, path, digest(exact)]))[7:]
        )
        insert_exact(
            db,
            "source_record",
            {
                "id": identifier,
                "kind": "authored",
                "sha256": digest(exact),
                "authored_path": "authored/" + path,
                "authored_revision": revision,
                "parser_version": "translation-authored-current-v2",
            },
            ("id",),
        )
        for record in Shard.model_validate_json(content).records:
            result[record.record_key] = identifier
    return result


def populate(  # ruff: ignore[complex-structure,too-many-branches] -- source validation finishes before any typed current projection
    db: Database,
    inputs: Inputs,
    snapshot: Snapshot,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    sources: Sources | None = None,
) -> InputRecord:
    """Read sources once for every currently used value before projection."""
    configuration = object_value(parse(build.configuration.encode()))
    if (
        configuration.get("translation_authored")
        != inputs.configuration()["translation_authored"]
    ):
        raise ValueError("Build configuration does not pin translation authored bytes")
    sources = (
        sources.stage(build)
        if sources is not None
        else Sources(stores, inputs.repository, build)
    )
    records = snapshot.current_records()
    originals: dict[str, str] = {}
    values: dict[str, tuple[str, str | None] | None] = {}
    for record in records:
        if not isinstance(record, TermRecord):
            continue
        if record.data.source_ref is None:
            lang, text = "ja", record.data.authored_source_ja
        else:
            lang, text, _ = sources.text(
                record.data.source_ref, record.data.source_span
            )
        if lang != "ja" or not text:
            raise ValueError("Glossary concept requires exact Japanese source")
        originals[record.data.id] = text
    for record in records:
        if isinstance(record, ChoiceRecord):
            values[record.record_key] = validate_choice(
                record,
                original=originals.get(record.data.term_id),
                sources=sources,
                db=db,
            )
        elif record.kind == "card_name_concept":
            lang, _, _ = sources.text(record.data.source_ref)
            if lang != record.data.subject.source_lang:
                raise ValueError("Name concept language differs from physical source")
    # Owner/face applicability is verified against this build, not old adoption bases.
    from sve_carddb.translations.current_names import prepare  # ruff: ignore[import-outside-top-level] -- owner checks share current resolved sources without introducing a module import cycle

    prepare(snapshot, originals, inputs, sources, db)
    insert_raw_sources(db, (use.source for use in sources.uses))
    audit = authored_sources(db, snapshot, inputs.authored_revision)
    emphasis = {
        r.data.term_id: r.data.value
        for r in records
        if r.kind == "glossary_emphasis_choice"
    }
    for record in records:
        source = audit.get(record.record_key)
        if source is None:
            raise ValueError(
                "Current glossary projection requires current-format shards"
            )
        if isinstance(record, TermRecord):
            db.insert(
                "glossary_term",
                {
                    "id": record.data.id,
                    "category": record.data.category,
                    "emphasis": emphasis.get(record.data.id)
                    if record.data.category == "rule_term"
                    else True,
                    "source_ja": originals[record.data.id],
                    "concept_key": record.data.concept_key,
                    "record_key": record.record_key,
                    "origin": record.origin,
                    "authored_source_id": source,
                    "low_confidence": record.low_confidence,
                },
            )
        elif isinstance(record, ChoiceRecord):
            value = values[record.record_key]
            if value is not None:
                db.insert(
                    "glossary_translation",
                    {
                        "term_id": record.data.term_id,
                        "lang": record.data.lang,
                        "text": value[0],
                        "source_id": value[1],
                        "record_key": record.record_key,
                        "origin": record.origin,
                        "authored_source_id": source,
                        "low_confidence": record.low_confidence,
                    },
                )
    return input_record(build, sources.uses)

"""Date provenance cannot widen the generic instant or raw-row boundary."""

import sqlite3

import pytest
from pydantic import ValidationError

from sve_carddb.build import create_database
from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import (
    DateSource,
    DateSourceUse,
    InputRecord,
    Source,
    SourceUse,
    input_record,
)

from ..support.construction_fixtures import evidence
from ..support.registry_preview_fixtures import BUILD


def dated() -> DateSource:
    instant = evidence().source.model_dump(mode="json")
    instant.update(
        fetched_at=None,
        observed_precision="date",
        observed_on="2026-09-25",
        observed_timezone="Asia/Taipei",
        observed_basis="Synthetic approved day without an instant.",
        import_receipt_id="sha256:" + "e" * 64,
    )
    return DateSource.model_validate_json(canonical(instant))


def test_date_evidence_round_trips_without_a_fabricated_instant() -> None:
    source = dated()
    use = DateSourceUse(
        source=source, usage="construction_coverage", locator="synthetic day"
    )
    record = input_record(BUILD, (), date_uses=(use, use))
    assert record.uses == ()
    assert record.date_uses == (use,)
    assert InputRecord.model_validate_json(record.content()) == record
    with create_database(compile_build()) as db, db.transaction():
        insert_raw_sources(db, (source,))
        [row] = db.rows("source_record")
        assert row.values["fetched_at"] is None
        assert row.values["observed_on"] == "2026-09-25"
        assert row.values["parser_version"] is None


def test_generic_sources_and_uses_still_reject_date_metadata() -> None:
    source = dated()
    with pytest.raises(ValidationError):
        Source.model_validate_json(source.model_dump_json())
    with pytest.raises(ValidationError):
        SourceUse.model_validate_json(
            canonical(
                {
                    "source": source.model_dump(mode="json"),
                    "usage": "registry_observation",
                    "locator": "synthetic",
                }
            )
        )
    with pytest.raises(ValidationError):
        DateSourceUse.model_validate_json(
            canonical(
                {
                    "source": source.model_dump(mode="json"),
                    "usage": "registry_observation",
                    "locator": "synthetic",
                }
            )
        )


def test_one_version_cannot_claim_both_precisions() -> None:
    use = DateSourceUse(
        source=dated(), usage="construction_profile", locator="synthetic"
    )
    with pytest.raises(ValueError, match="cannot mix instant and date"):
        input_record(BUILD, (evidence(),), date_uses=(use,))


def test_raw_row_without_time_needs_all_approved_date_metadata() -> None:
    with create_database(compile_build()) as db:
        with pytest.raises(sqlite3.IntegrityError), db.transaction():
            db.insert(
                "source_record",
                {
                    "id": "missing-date",
                    "kind": "official_page",
                    "url": "https://example.invalid/rules",
                    "sha256": "sha256:" + "a" * 64,
                },
            )
        assert not db.rows("source_record")

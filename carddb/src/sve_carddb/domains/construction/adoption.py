"""One verified human entry for construction facts in offline builds."""

from dataclasses import dataclass
from pathlib import Path
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.build.source_rows import insert_raw_sources, source_values
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.core.models import RecordData
from sve_carddb.core.provenance import input_record
from sve_carddb.domains.construction.adoption_loader import Snapshot, load
from sve_carddb.domains.construction.adoption_models import (
    CoverageRecord,
    ProfileRecord,
    Record,
    RestrictionRecord,
)
from sve_carddb.domains.construction.adoption_reports import StorePin, verify_reports
from sve_carddb.domains.construction.adoption_sources import Lists
from sve_carddb.domains.construction.adoption_validation import validate
from sve_carddb.domains.text_observations.intern import TextInterner

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value
    from sve_carddb.core.provenance import BuildContext, InputRecord


class Settings(RecordData):
    source_stores: dict[str, Path]
    review_stores: tuple[StorePin, ...]

    def protected_inputs(self) -> tuple[Path, ...]:
        """Keep every immutable input outside disposable output roots."""
        return (
            *self.source_stores.values(),
            *(
                path
                for store in self.review_stores
                for path in (store.root, store.backup, store.restored)
            ),
        )


@dataclass(frozen=True)
class Prepared:
    snapshot: Snapshot
    lists: Lists
    reports: tuple[dict[str, str], ...]
    settings: Settings

    def configuration(self) -> dict[str, JsonValue]:
        """Pin complete adoption bytes, staging and verified report inventories."""
        values: dict[str, JsonValue] = {
            "construction_staging": [
                record.model_dump(mode="json") for record in self.snapshot.current
            ],
            "construction_review_reports": [
                {"report_hash": report["report_hash"], "store_id": report["store_id"]}
                for report in self.reports
            ],
            "construction_review_inventories": [
                {"store_id": store.store_id, "sha256": store.inventory_sha256}
                for store in sorted(
                    self.settings.review_stores, key=lambda item: item.store_id
                )
            ],
        }

        return self.snapshot.configuration() | values

    def populate(self, db: Database, build: BuildContext) -> Adopted:
        """Replay official evidence and freshness before writing confirmed facts."""
        configuration = object_value(parse(build.configuration.encode()))
        if any(
            configuration.get(key) != value
            for key, value in self.configuration().items()
        ):
            raise ValueError(
                "Build configuration does not pin the complete construction adoption"
            )
        validate(self.snapshot, db, self.lists)
        expected = tuple(
            sorted(
                {
                    item.use
                    for loaded in self.snapshot.shards
                    for record in loaded.shard.records
                    for item in record.evidence
                },
                key=lambda use: canonical(use.model_dump(mode="json")),
            )
        )
        if set(expected) != self.lists.uses:
            raise ValueError(
                "Construction expected source uses differ from actual replay"
            )
        insert_raw_sources(db, (use.source for use in expected))
        _decisions(db, self.snapshot)
        texts = TextInterner(db)
        decisions = {
            record.record_key: loaded.shard.default_decision_id
            for loaded in self.snapshot.shards
            for record in loaded.shard.records
        }
        for record in self.snapshot.current:
            value = record.data.value
            assert value is not None
            primary = next(item.use for item in record.evidence if item.primary)
            if isinstance(record, ProfileRecord):
                assert record.data.value is not None
                db.insert_exact(
                    "rules_profile",
                    {
                        "id": record.data.subject.profile_id,
                        "region": record.data.value.region,
                        "format_code": "standard",
                        "name_unit_id": texts.intern(record.data.value.name),
                    },
                    ("id",),
                )
        for record in self.snapshot.current:
            primary = next(item.use for item in record.evidence if item.primary)
            if isinstance(record, RestrictionRecord):
                assert record.data.value is not None
                values: dict[str, Value] = dict(
                    record.data.value.model_dump(
                        mode="json", exclude={"members"}
                    ).items()
                )
                values |= {
                    "id": record.data.subject.restriction_id,
                    "source_id": primary.source.id,
                    "decision_id": decisions[record.record_key],
                }
                db.insert_exact("restriction", values, ("id",))
                for member in record.data.value.members:
                    db.insert_exact(
                        "restriction_member",
                        member.model_dump(mode="json")
                        | {"restriction_id": record.data.subject.restriction_id},
                        ("restriction_id", "rules_name_id", "deck_scope"),
                    )
            elif isinstance(record, CoverageRecord):
                assert record.data.value is not None
                db.insert_exact(
                    "restriction_coverage",
                    {
                        "profile_id": record.data.subject.profile_id,
                        "from_date": record.data.subject.from_date,
                        "until_date": record.data.value.until_date,
                        "state": record.data.value.state,
                        "source_id": primary.source.id,
                    },
                    ("profile_id", "from_date"),
                )
        raw = {
            row.values["id"]: dict(row.values)
            for row in db.rows("source_record")
            if row.values["observed_on"] is not None
        }
        if raw != {use.source.id: source_values(use.source) for use in expected}:
            raise ValueError(
                "Construction F1 date evidence differs from the DB raw source set"
            )
        db.verify()
        return Adopted(
            self.snapshot.current, input_record(build, (), date_uses=expected)
        )


def prepare(
    root: Path, repository: Path, revision: str, settings: Settings
) -> Prepared:
    """Read the whole immutable entry and require both report backups."""
    snapshot = load(root, repository, revision)
    reports = tuple(verify_reports(snapshot, settings.review_stores))
    batches = tuple(
        sorted(
            {
                pin.batch_id
                for loaded in snapshot.shards
                for pin in loaded.shard.review_context.source_batches
            }
        )
    )
    lists = Lists(settings.source_stores, batches)
    return Prepared(snapshot, lists, reports, settings)


def _decisions(db: Database, snapshot: Snapshot) -> None:
    for loaded in snapshot.shards:
        checksum = digest(loaded.exact)
        source_id = (
            "authored:v1:"
            + digest(
                canonical(
                    {
                        "path": loaded.name,
                        "hash": checksum,
                        "revision": snapshot.revision,
                    }
                )
            )[7:]
        )
        db.insert_exact(
            "source_record",
            {
                "id": source_id,
                "kind": "authored",
                "sha256": checksum,
                "authored_path": "authored/" + loaded.name,
                "authored_revision": snapshot.revision,
                "parser_version": "construction-adoption-v1",
            },
            ("id",),
        )
        decision = loaded.shard.decisions[0]
        values: dict[str, Value] = dict(
            decision.model_dump(mode="json", exclude={"members", "sample_ids"}).items()
        )
        values["sample_ids"] = Json(list(decision.sample_ids))
        db.insert_exact("decision", values, ("id",))
        db.insert_exact(
            "decision_source",
            {
                "decision_id": decision.id,
                "source_id": source_id,
                "role": "adoption_envelope",
                "locator": None,
                "quote": None,
            },
            ("decision_id", "source_id", "role"),
        )
        grouped: dict[str, set[str]] = {}
        for record in loaded.shard.records:
            for evidence in record.evidence:
                grouped.setdefault(evidence.use.source.id, set()).add(
                    canonical(evidence.use.model_dump(mode="json")).decode()
                )
        for identifier, uses in sorted(grouped.items()):
            db.insert_exact(
                "decision_source",
                {
                    "decision_id": decision.id,
                    "source_id": identifier,
                    "role": "official_evidence",
                    "locator": canonical(
                        [parse(value.encode()) for value in sorted(uses)]
                    ).decode(),
                    "quote": None,
                },
                ("decision_id", "source_id", "role"),
            )


@dataclass(frozen=True)
class Adopted:
    records: tuple[Record, ...]
    record: InputRecord

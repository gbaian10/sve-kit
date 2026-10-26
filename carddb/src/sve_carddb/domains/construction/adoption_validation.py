"""Reconcile full current lists and the exact live identity dependencies."""

from datetime import date, timedelta
from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.build import Json
from sve_carddb.core.json import array, canonical, digest, object_value, parse
from sve_carddb.core.provenance import DateSource
from sve_carddb.domains.construction.adoption_models import (
    CoverageRecord,
    Dependency,
    ProfileRecord,
    Record,
    RestrictionRecord,
)
from sve_carddb.domains.construction.adoption_sources import observed_day

if TYPE_CHECKING:
    from sve_carddb.build import Database
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.construction.adoption_loader import Snapshot
    from sve_carddb.domains.construction.adoption_sources import Lists


class Freshness:
    def __init__(self, db: Database, records: tuple[Record, ...]) -> None:
        self.tables: dict[str, list[dict[str, JsonValue]]] = {
            name: [
                {
                    key: value.value if isinstance(value, Json) else value
                    for key, value in row.values.items()
                }
                for row in db.rows(name)
            ]
            for name in (
                "card",
                "face",
                "face_revision",
                "rules_name",
                "face_rules_name",
                "printing",
                "printing_face",
            )
        }
        self.profiles = {
            record.data.subject.profile_id: record.data.value
            for record in records
            if isinstance(record, ProfileRecord)
        }

    def dependency(  # ruff: ignore[complex-structure] -- Three supported dependency projections have distinct closure checks.
        self,
        kind: Literal["profile", "rules_name", "card"],
        identifier: str,
        region: Region,
    ) -> Dependency:
        """Hash the complete supported regional dependency projection."""
        if kind == "profile":
            profile = self.profiles.get(identifier)
            if profile is None or profile.region != region:
                raise ValueError(
                    "Construction profile dependency is absent or cross-region"
                )
            value: JsonValue = {
                "id": identifier,
                "region": region,
                "format_code": profile.format_code,
            }
        elif kind == "rules_name":
            names = [
                row
                for row in self.tables["rules_name"]
                if row["id"] == identifier and row["region"] == region
            ]
            if len(names) != 1:
                raise ValueError(
                    "Construction rules-name dependency is absent or cross-region"
                )
            faces: list[JsonValue] = sorted(
                (
                    {"face_id": row["face_id"], "role": row["role"]}
                    for row in self.tables["face_rules_name"]
                    if row["rules_name_id"] == identifier and row["region"] == region
                ),
                key=canonical,
            )
            if not faces:
                raise ValueError("Construction rules-name group has no physical faces")
            value = {
                "id": identifier,
                "region": region,
                "official_name": names[0]["official_name"],
                "faces": faces,
            }
        elif kind == "card":
            cards = [row for row in self.tables["card"] if row["id"] == identifier]
            if len(cards) != 1 or not any(
                row["card_id"] == identifier and row["region"] == region
                for row in self.tables["printing"]
            ):
                raise ValueError(
                    "Construction card dependency is absent from its region"
                )
            projected: list[JsonValue] = []
            for face in self.tables["face"]:
                if face["card_id"] != identifier:
                    continue
                revisions = [
                    row
                    for row in self.tables["face_revision"]
                    if row["face_id"] == face["id"] and row["region"] == region
                ]
                if not revisions:
                    raise ValueError(
                        "Construction card dependency lacks a regional face type"
                    )
                latest = max(revisions, key=lambda row: int(str(row["revision"])))
                projected.append(
                    {
                        "id": face["id"],
                        "ordinal": face["ordinal"],
                        "side": face["side"],
                        "type_code": latest["type_code"],
                    }
                )
            if not projected:
                raise ValueError("Construction card dependency lacks physical faces")
            value = {
                "id": identifier,
                "region": region,
                "layout": cards[0]["layout"],
                "faces": sorted(projected, key=canonical),
            }
        else:
            raise ValueError("Unsupported current-list dependency kind")
        return Dependency(
            kind=kind, id=identifier, region=region, basis_hash=digest(canonical(value))
        )

    def restriction_dependencies(
        self, record: RestrictionRecord, region: Region
    ) -> tuple[Dependency, ...]:
        """Include every named group and its physical cards in freshness."""
        value = record.data.value
        assert value is not None
        dependencies = {self.dependency("profile", value.profile_id, region)}
        for member in value.members:
            dependencies.add(
                self.dependency("rules_name", member.rules_name_id, region)
            )
            faces = {
                row["face_id"]
                for row in self.tables["face_rules_name"]
                if row["rules_name_id"] == member.rules_name_id
                and row["region"] == region
            }
            dependencies.update(
                self.dependency("card", card, region)
                for card in {
                    str(row["card_id"])
                    for row in self.tables["face"]
                    if row["id"] in faces
                }
            )
        return tuple(
            sorted(
                dependencies, key=lambda item: canonical(item.model_dump(mode="json"))
            )
        )

    def verify_restriction(
        self, record: RestrictionRecord, observation: JsonValue, region: Region
    ) -> None:
        """Match the linked card, final fields and full regional name group."""
        value = record.data.value
        assert value is not None
        entry = object_value(observation)
        if len(value.members) != 1:
            raise ValueError(
                "Current-list entry must name one exact regional name group"
            )
        member = value.members[0]
        names = [
            row
            for row in self.tables["rules_name"]
            if row["id"] == member.rules_name_id
            and row["region"] == region
            and row["official_name"] == entry["name"]
        ]
        if (
            len(names) != 1
            or value.announced_on is not None
            or any(
                getattr(value, field) != entry[field]
                for field in ("effective_from", "effective_until", "max_copies")
            )
        ):
            raise ValueError("Adopted restriction differs from the replayed list entry")
        printings = [
            row
            for row in self.tables["printing"]
            if row["region"] == region and row["card_no"] == entry["card_no"]
        ]
        named_faces = {
            row["face_id"]
            for row in self.tables["face_rules_name"]
            if row["region"] == region and row["rules_name_id"] == member.rules_name_id
        }
        if not printings or not any(
            face["printing_id"] in {printing["id"] for printing in printings}
            and face["face_id"] in named_faces
            for face in self.tables["printing_face"]
        ):
            raise ValueError(
                "Linked card number does not have the adopted regional name group"
            )
        if record.data.dependencies != self.restriction_dependencies(record, region):
            raise ValueError(
                "Construction restriction dependencies are missing, extra or stale"
            )


def _restriction_observation(
    observation: JsonValue, additional: list[JsonValue], region: Region
) -> JsonValue:
    if not additional:
        return observation
    entry = object_value(observation)
    if len(additional) != 1:
        raise ValueError("A closed restriction needs one explicit lift notice")
    lift = object_value(additional[0])
    if (
        region != "jp"  # ruff: ignore[too-many-boolean-expressions] -- The lift must match the original entry on every identity field.
        or set(lift) != {"name", "card_no", "card_link", "effective_until", "locator"}
        or any(
            lift.get(field) != entry.get(field)
            for field in ("name", "card_no", "card_link")
        )
        or entry.get("effective_until") is not None
        or not isinstance(lift.get("effective_until"), str)
        or str(lift["effective_until"]) <= str(entry.get("effective_from"))
    ):
        raise ValueError("Lift notice must match the original linked restriction")
    return entry | {"effective_until": lift["effective_until"]}


def _context_records(snapshot: Snapshot, context_hash: str) -> tuple[Record, ...]:
    latest: dict[bytes, Record] = {}
    for candidate in sorted(
        (
            record
            for shard in snapshot.shards
            for record in shard.shard.records
            if record.data.review_context_hash == context_hash
        ),
        key=lambda item: item.data.adoption_no,
    ):
        latest[
            canonical([candidate.kind, candidate.data.subject.model_dump(mode="json")])
        ] = candidate
    return tuple(latest.values())


def validate(  # ruff: ignore[complex-structure] -- Validate all evidence and effective records together.
    snapshot: Snapshot, db: Database, lists: Lists
) -> None:
    """Reconcile every active list entry with complete current adoption data."""
    freshness = Freshness(db, snapshot.current)
    primary_values: dict[str, JsonValue] = {}
    reconciled: dict[str, list[JsonValue]] = {}
    for loaded in snapshot.shards:
        for record in loaded.shard.records:
            region: Region = "jp" if record.filing_key == "jp-standard" else "en"
            for evidence in record.evidence:
                observed = lists.verify(evidence.use, region, record.record_key)
                if evidence.primary:
                    primary_values[record.record_key] = observed
                else:
                    reconciled.setdefault(record.record_key, []).append(observed)
    for record in snapshot.current:
        region = "jp" if record.filing_key == "jp-standard" else "en"
        if isinstance(record, ProfileRecord):
            if (
                record.data.dependencies
                or object_value(primary_values[record.record_key])["region"] != region
            ):
                raise ValueError("Current profile evidence or dependency set mismatch")
        elif isinstance(record, RestrictionRecord):
            freshness.verify_restriction(
                record,
                _restriction_observation(
                    primary_values[record.record_key],
                    reconciled.get(record.record_key, []),
                    region,
                ),
                region,
            )
        else:
            _coverage(
                record,
                _context_records(snapshot, record.data.review_context_hash),
                primary_values[record.record_key],
                freshness,
                lists,
                region,
            )
    profiles = {
        record.data.subject.profile_id
        for record in snapshot.current
        if isinstance(record, ProfileRecord)
    }
    covered = [
        record.data.subject.profile_id
        for record in snapshot.current
        if isinstance(record, CoverageRecord)
    ]
    if profiles != set(covered):
        raise ValueError("Current-list profiles require exact day coverage each")
    if any(
        isinstance(record, RestrictionRecord)
        and record.data.value is not None
        and record.data.value.profile_id not in profiles
        for record in snapshot.current
    ):
        raise ValueError("Restriction has no current adopted profile")


def _coverage(  # ruff: ignore[complex-structure] -- Reconcile the declared day and its complete list denominator together.
    record: CoverageRecord,
    records: tuple[Record, ...],
    observation: JsonValue,
    freshness: Freshness,
    lists: Lists,
    region: Region,
) -> None:
    value = record.data.value
    assert value is not None
    evidence = next(item.use for item in record.evidence if item.primary)
    day = record.data.subject.from_date
    reconciliation = value.reconciliation
    legacy = reconciliation.mode == "current-list-day-v1"
    if (
        value.until_date != (date.fromisoformat(day) + timedelta(days=1)).isoformat()  # ruff: ignore[too-many-boolean-expressions] -- Day, baseline and provenance must all describe the same observation.
        or value.as_of != day
        or reconciliation.observed_on != day
        or observed_day(evidence.source, reconciliation.observed_timezone) != day
        or (legacy and not isinstance(evidence.source, DateSource))
        or value.source_set.baseline != (evidence.source.id,)
        or (
            legacy
            and lists.regional_versions(region, evidence.source.archive.batch_id)
            != {evidence.source.id}
        )
        or reconciliation.projection_hash != digest(canonical(observation))
    ):
        raise ValueError(
            "Current-list coverage is restricted to its exact observed day and full source set"
        )
    expected_dependencies = (
        freshness.dependency("profile", record.data.subject.profile_id, region),
    )
    if record.data.dependencies != expected_dependencies:
        raise ValueError("Construction coverage profile dependency is stale")
    entries = array(object_value(observation)["entries"])
    required: set[str] = set()
    excluded: list[str] = []
    for index, raw in enumerate(entries):
        entry = object_value(raw)
        pointer = "/entries/" + str(index)
        if (not legacy or str(entry["effective_from"]) <= day) and (
            entry["effective_until"] is None or day < str(entry["effective_until"])
        ):
            required.add(pointer)
        else:
            excluded.append(pointer)
    if reconciliation.excluded_entry_pointers != tuple(sorted(excluded)):
        raise ValueError(
            "Inactive current-list entries must remain explicitly classified"
        )
    adopted: list[str] = []
    for restriction in records:
        if (
            not isinstance(restriction, RestrictionRecord)
            or restriction.data.value is None
            or restriction.data.value.profile_id != record.data.subject.profile_id
        ):
            continue
        use = next(item.use for item in restriction.evidence if item.primary)
        restriction_value = restriction.data.value
        if (
            not legacy
            and restriction_value.effective_until is not None
            and restriction_value.effective_until <= day
        ):
            continue
        if use.source != evidence.source:
            raise ValueError(
                "Current-list restriction differs from the coverage baseline"
            )
        location = object_value(object_value(parse(use.locator.encode()))["location"])
        adopted.append(str(location["pointer"]))
    if len(adopted) != len(set(adopted)) or set(adopted) != required:
        raise ValueError(
            "Current-list coverage must reconcile every active entry exactly once"
        )

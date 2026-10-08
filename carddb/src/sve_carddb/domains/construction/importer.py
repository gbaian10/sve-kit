"""Transaction-local projection of pinned construction staging evidence."""

from typing import TYPE_CHECKING

from sve_carddb.build.source_rows import insert_raw_sources
from sve_carddb.core.json import parse
from sve_carddb.core.provenance import input_record
from sve_carddb.domains.construction.models import Construction
from sve_carddb.domains.text_observations.intern import TextInterner

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value
    from sve_carddb.core.models import RecordData
    from sve_carddb.core.provenance import BuildContext, InputRecord


def _values(record: RecordData, *exclude: str) -> dict[str, Value]:
    result: dict[str, Value] = {}
    for key, value in record.model_dump(mode="json", exclude=set(exclude)).items():
        if value is not None and not isinstance(value, (str, bool, int)):
            raise TypeError("Construction row must contain scalar values")
        result[key] = value
    return result


def _insert(
    db: Database, table: str, values: dict[str, Value], keys: tuple[str, ...]
) -> None:
    previous = [
        row.values
        for row in db.rows(table)
        if all(row.values[key] == values[key] for key in keys)
    ]
    if previous:
        if previous != [values]:
            raise ValueError("Conflicting construction row")
    else:
        db.insert(table, values)


def _adoption(db: Database, decision_id: str) -> None:
    decisions = {row.values["id"]: row.values for row in db.rows("decision")}
    if decision_id not in decisions or decisions[decision_id]["state"] != "confirmed":
        raise ValueError("Construction adoption requires a confirmed decision")
    # A confirmed identity/catalog decision cannot authorize a different rule payload.
    raise ValueError(
        "Construction adoption cannot verify category, exact members or freshness "
        "until the authored adoption contract and loader are finalized"
    )


def _coverage(db: Database) -> None:
    rows = [row.values for row in db.rows("restriction_coverage")]
    for index, left in enumerate(rows):
        for right in rows[index + 1 :]:
            if left["profile_id"] != right["profile_id"]:
                continue
            if (
                left["until_date"] is None
                or str(right["from_date"]) < str(left["until_date"])
            ) and (
                right["until_date"] is None
                or str(left["from_date"]) < str(right["until_date"])
            ):
                raise ValueError(
                    "Overlapping coverage must be reconciled before import"
                )


def populate_construction(
    db: Database, staging: Construction, *, build: BuildContext
) -> InputRecord:
    """Compose inside the caller's transaction; this does not adopt authored rules."""
    staging = Construction.model_validate_json(staging.model_dump_json())
    configuration = parse(build.configuration.encode())
    if (
        not isinstance(configuration, dict)
        or configuration.get("construction") != staging.configuration()["construction"]
    ):
        raise ValueError("Build configuration does not pin construction staging")
    for override in staging.overrides:
        _adoption(db, override.decision_id)
    for restriction in staging.restrictions:
        if restriction.decision_id is not None:
            _adoption(db, restriction.decision_id)
    expected = staging.source_uses()
    insert_raw_sources(db, (use.source for use in expected))
    texts = TextInterner(db)
    for profile in staging.profiles:
        values = _values(profile, "evidence", "name")
        values["name_unit_id"] = texts.intern(profile.name)
        _insert(db, "rules_profile", values, ("id",))
    _cr(db, staging, texts)
    for revision in staging.revisions:
        values = _values(revision, "evidence")
        values["source_id"] = revision.evidence.source.id
        _insert(db, "rules_profile_revision", values, ("id",))
    _restrictions(db, staging)
    for coverage in staging.coverage:
        values = _values(coverage, "evidence")
        values["source_id"] = coverage.evidence.source.id
        _insert(db, "restriction_coverage", values, ("profile_id", "from_date"))
    _coverage(db)
    _clauses(db)
    db.verify()
    return input_record(build, expected)


def _cr(db: Database, staging: Construction, texts: TextInterner) -> None:
    for version in staging.cr_versions:
        values = _values(version, "evidence", "clauses")
        values.update(
            source_id=version.evidence.source.id, source_url=version.evidence.source.url
        )
        _insert(db, "cr_version", values, ("id",))
        for clause in version.clauses:
            _insert(
                db,
                "cr_clause",
                {
                    "id": clause.id,
                    "cr_version_id": version.id,
                    "number": clause.number,
                    "text_unit_id": texts.intern(clause.text),
                },
                ("id",),
            )


def _restrictions(db: Database, staging: Construction) -> None:
    existing_ids = {row.values["id"] for row in db.rows("restriction")}
    for restriction in staging.restrictions:
        members = tuple(
            _values(member) | {"restriction_id": restriction.id}
            for member in restriction.members
        )
        if restriction.id in existing_ids:
            previous = [
                dict(row.values)
                for row in db.rows("restriction_member")
                if row.values["restriction_id"] == restriction.id
            ]
            if len(previous) != len(members) or any(
                member not in previous for member in members
            ):
                raise ValueError("Conflicting construction restriction membership")
        values = _values(restriction, "evidence", "members")
        values["source_id"] = restriction.evidence.source.id
        _insert(db, "restriction", values, ("id",))
        for values in members:
            _insert(
                db,
                "restriction_member",
                values,
                ("restriction_id", "rules_name_id", "deck_scope"),
            )
        existing_ids.add(restriction.id)


def _clauses(db: Database) -> None:
    if not any(
        row.values["cr_version_id"] is not None
        for row in db.rows("rules_profile_revision")
    ):
        return
    versions = {row.values["cr_version_id"] for row in db.rows("cr_clause")}
    if any(
        row.values["cr_version_id"] is not None
        and row.values["cr_version_id"] not in versions
        for row in db.rows("rules_profile_revision")
    ):
        raise ValueError("Referenced CR version requires concrete clauses")

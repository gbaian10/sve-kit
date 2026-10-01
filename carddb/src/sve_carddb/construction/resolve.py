"""Resolve dated construction inputs conservatively, without evaluating a deck."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from pydantic import ConfigDict, TypeAdapter

from sve_carddb.registry.records import Date

_DATE = TypeAdapter(Date, config=ConfigDict(regex_engine="python-re", strict=True))

if TYPE_CHECKING:
    from sve_carddb.build_db import Database


@dataclass(frozen=True)
class ConstructionContext:
    inputs_state: Literal["ready", "unknown"]
    reasons: tuple[str, ...]
    revision_id: str | None
    restriction_ids: tuple[str, ...]
    legality: Literal["unknown"] = "unknown"


def _contains(start: str, end: str | None, day: str) -> bool:
    return start <= day and (end is None or day < end)


def resolve_construction(
    db: Database,
    profile_id: str,
    *,
    on_date: str,
    as_of: str,
    supported_refs: frozenset[str],
) -> ConstructionContext:
    """Require coverage and an explicitly supported pinned algorithm reference."""
    _DATE.validate_python(on_date)
    _DATE.validate_python(as_of)
    db.verify()
    profiles = {row.values["id"] for row in db.rows("rules_profile")}
    reasons = []
    if profile_id not in profiles:
        reasons.append("profile_missing")
    if on_date > as_of:
        reasons.append("after_as_of")
    coverage = [
        row.values
        for row in db.rows("restriction_coverage")
        if row.values["profile_id"] == profile_id
        and _contains(
            str(row.values["from_date"]),
            None if row.values["until_date"] is None else str(row.values["until_date"]),
            on_date,
        )
    ]
    if len(coverage) != 1 or coverage[0]["state"] != "complete":
        reasons.append("coverage_unknown")
    revisions = [
        row.values
        for row in db.rows("rules_profile_revision")
        if row.values["profile_id"] == profile_id
        and _contains(
            str(row.values["effective_from"]),
            None
            if row.values["effective_until"] is None
            else str(row.values["effective_until"]),
            on_date,
        )
    ]
    revision = revisions[0] if len(revisions) == 1 else None
    if revision is None:
        reasons.append("revision_unknown")
    else:
        reference = revision["construction_rules_ref"]
        if reference is None or reference not in supported_refs:
            reasons.append("construction_rules_unknown")
        if revision["default_copy_limit"] is None:
            reasons.append("copy_limit_unknown")
        if revision["cr_version_id"] is not None and not any(
            row.values["cr_version_id"] == revision["cr_version_id"]
            for row in db.rows("cr_clause")
        ):
            reasons.append("cr_clauses_missing")
    restrictions = [
        row.values
        for row in db.rows("restriction")
        if row.values["profile_id"] == profile_id
        and _contains(
            str(row.values["effective_from"]),
            None
            if row.values["effective_until"] is None
            else str(row.values["effective_until"]),
            on_date,
        )
    ]
    if any(row["state"] == "announced" for row in restrictions):
        reasons.append("restriction_unconfirmed")
    confirmed = tuple(
        sorted(str(row["id"]) for row in restrictions if row["state"] == "confirmed")
    )
    members = {row.values["restriction_id"] for row in db.rows("restriction_member")}
    if any(identifier not in members for identifier in confirmed):
        reasons.append("restriction_members_missing")
    return ConstructionContext(
        inputs_state="unknown" if reasons else "ready",
        reasons=tuple(reasons),
        revision_id=None if revision is None else str(revision["id"]),
        restriction_ids=confirmed,
    )

"""Consume exact reviewed correction checks without adopting or inventing card text."""

from typing import TYPE_CHECKING

from sve_carddb.build_db import Json
from sve_carddb.card_extras.reskin_rules import actual_rules
from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_db import Database, Value
    from sve_carddb.card_extras.importer import CardExtrasRestriction


class ErrataConfirmation(RecordData):
    issue_id: Text
    decision_id: Text
    context_hash: Hash


def _plain(values: dict[str, Value]) -> dict[str, JsonValue]:
    return {
        key: value.value if isinstance(value, Json) else value
        for key, value in values.items()
    }


def _string(value: Value) -> str:
    if not isinstance(value, str):
        raise TypeError("Expected errata review identity text")
    return value


def review_context(db: Database, restriction: CardExtrasRestriction) -> str | None:
    """Hash all observed/current faces, source metadata and exact announcement evidence."""
    context = _context(db, restriction)
    return None if context is None else digest(context[0])


def confirmed(
    db: Database,
    restriction: CardExtrasRestriction,
    checks: tuple[ErrataConfirmation, ...],
) -> bool:
    """A stale check or absent review can never clear a pending capability block."""
    check = next(
        (item for item in checks if item.issue_id == restriction.issue_id), None
    )
    if check is None or (context := _context(db, restriction)) is None:
        return False
    raw, sources, announcements = context
    if check.context_hash != digest(raw):
        return False
    if not any(
        row.values["id"] == check.decision_id and row.values["state"] == "confirmed"
        for row in db.rows("decision")
    ):
        return False
    links = [
        row.values
        for row in db.rows("decision_source")
        if row.values["decision_id"] == check.decision_id
    ]
    pinned = {
        _string(row["source_id"])
        for row in links
        if row["role"] == "errata_current_evidence"
    }
    receipt = canonical(
        {"issue_id": check.issue_id, "context_hash": check.context_hash}
    ).decode()
    reviewed = {
        _string(row["source_id"])
        for row in links
        if row["role"] == "errata_current_checked" and row["locator"] == receipt
    }
    return sources <= pinned and announcements <= reviewed


def _context(  # ruff: ignore[too-many-locals] -- bind all faces, physical observations and sources in the reviewed graph
    db: Database, restriction: CardExtrasRestriction
) -> tuple[bytes, set[str], set[str]] | None:
    if restriction.reason != "errata_current_pending" or restriction.card_id is None:
        return None
    issue = next(
        row.values
        for row in db.rows("build_issue")
        if row.values["id"] == restriction.issue_id
    )
    details = object_value(parse(_string(issue["message"]).encode()))
    notices = {
        row.values["id"]
        for row in db.rows("errata")
        if row.values["region"] == restriction.region
        and row.values["official_url"] == details["target"]
    }
    versions = [
        dict(row.values)
        for row in db.rows("errata_version")
        if row.values["errata_id"] in notices
    ]
    printings = [
        dict(row.values)
        for row in db.rows("printing")
        if row.values["card_id"] == restriction.card_id
        and row.values["region"] == restriction.region
    ]
    printing_ids = {row["id"] for row in printings}
    faces = [
        dict(row.values)
        for row in db.rows("printing_face")
        if row.values["printing_id"] in printing_ids
    ]
    face_ids = {row["face_id"] for row in faces}
    version_ids = {row["id"] for row in versions}
    linked = any(
        row.values["errata_version_id"] in version_ids
        and row.values["printing_id"] in printing_ids
        for row in db.rows("errata_printing")
    ) or any(
        row.values["errata_version_id"] in version_ids
        and row.values["face_id"] in face_ids
        for row in db.rows("errata_change")
    )
    if not linked:
        return None
    currents = [
        dict(row.values)
        for row in db.rows("face_current")
        if row.values["face_id"] in face_ids
        and row.values["region"] == restriction.region
    ]
    if not face_ids or {row["face_id"] for row in currents} != face_ids:
        return None
    observations = [
        dict(row.values)
        for row in db.rows("printing_face_observation")
        if row.values["printing_id"] in printing_ids
    ]
    if {(row["printing_id"], row["face_id"]) for row in observations} != {
        (row["printing_id"], row["face_id"]) for row in faces
    }:
        return None
    revision_ids = {_string(row["revision_id"]) for row in (*currents, *observations)}
    rules = actual_rules(db)
    if not revision_ids <= rules.keys():
        return None
    announcements = {_string(row["source_id"]) for row in versions}
    sources = (
        announcements
        | {_string(row["source_id"]) for row in (*printings, *observations)}
        | {_string(issue["source_id"])}
    )
    revisions = [
        dict(row.values)
        for row in db.rows("face_revision")
        if row.values["id"] in revision_ids
    ]
    sources.update(_string(row["source_id"]) for row in revisions)
    graph: dict[str, list[dict[str, JsonValue]]] = {
        table: [
            _plain(dict(row.values))
            for row in db.rows(table)
            if row.values[owner]
            in (
                revision_ids
                if owner == "revision_id"
                else {row["id"] for row in versions}
            )
        ]
        for table, owner in (
            ("face_text_section", "revision_id"),
            ("errata_change", "errata_version_id"),
            ("errata_printing", "errata_version_id"),
        )
    }
    graph.update(
        {
            "issue": [_plain(dict(issue))],
            "printings": [_plain(row) for row in printings],
            "faces": [_plain(row) for row in faces],
            "currents": [_plain(row) for row in currents],
            "observations": [_plain(row) for row in observations],
            "revisions": [_plain(row) for row in revisions],
            "versions": [_plain(row) for row in versions],
            "sources": [
                _plain(dict(row.values))
                for row in db.rows("source_record")
                if row.values["id"] in sources
            ],
        }
    )
    text_ids = {
        value
        for rows in graph.values()
        for row in rows
        for name, value in row.items()
        if name.endswith("unit_id") and isinstance(value, str)
    }
    graph["texts"] = [
        _plain(dict(row.values))
        for row in db.rows("text_unit")
        if row.values["id"] in text_ids
    ]
    # Rules signatures cover traits/titles/special kinds even when a revision ID is reused.
    value: dict[str, JsonValue] = {
        "graph": {table: sorted(rows, key=canonical) for table, rows in graph.items()},
        "rules": {
            identifier: digest(rules[identifier]) for identifier in sorted(revision_ids)
        },
    }
    return canonical(value), sources, announcements

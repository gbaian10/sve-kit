"""Identity-only locations do not imply dependencies at a particular use."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import canonical, object_value, string

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.export.read_api import Snapshot


def rule_identities(snapshot: Snapshot, region: str) -> dict[str, JsonValue]:
    """Retain exact published locations without inferring a particular use's dependencies."""
    versions = {string(row["id"]): row for row in snapshot.tables["cr_version"]}
    answers = {string(row["id"]): row for row in snapshot.tables["qa"]}
    entries: list[JsonValue] = []
    for row in snapshot.tables["cr_clause"]:
        parent = versions[string(row["cr_version_id"])]
        if parent["region"] == region:
            entries.append(
                {"kind": "cr", "id": row["number"], "revision": parent["id"]}
            )
    for row in snapshot.tables["qa_version"]:
        parent = answers[string(row["qa_id"])]
        if parent["region"] == region:
            entries.append({"kind": "qa", "id": parent["id"], "revision": row["id"]})
    for row in snapshot.tables["ruling_revision"]:
        if row["strength"] not in {"official", "generalized", "inferred", "undecided"}:
            raise ValueError("Invalid ruling strength")
        entries.append(
            {
                "kind": "ruling",
                "id": row["ruling_id"],
                "revision": row["id"],
                "strength": row["strength"],
            }
        )
    keys: set[tuple[str, str, str]] = set()
    for value in entries:
        entry = object_value(value)
        key = (string(entry["kind"]), string(entry["id"]), string(entry["revision"]))
        if any(not part for part in key) or key in keys:
            raise ValueError("Ambiguous rule identity")
        keys.add(key)
    return {
        "manifest_sha256": snapshot.manifest_sha256,
        "entries": sorted(entries, key=canonical),
    }

"""Identity-only locations do not imply dependencies at a particular use."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import array, canonical, integer, object_value, string

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


def resolve_engine_rules(
    rules: dict[str, JsonValue], faces: dict[tuple[str, int], dict[str, JsonValue]]
) -> dict[str, JsonValue]:
    """Resolve rule anchors in place and return their matching engine identity binding."""
    source = object_value(rules["input"])
    region = string(object_value(rules["scope"])["region"])
    selected: dict[str, dict[str, JsonValue]] = {}

    def anchor(raw: JsonValue) -> dict[str, JsonValue]:
        ref = object_value(raw)
        if ref["kind"] != "legacy-face" or ref["region"] != region:
            raise ValueError("Unusable published battle rule anchor")
        face = faces[string(ref["card_no"]), integer(ref["face_ordinal"])]
        face_id = string(face["face_id"])
        if face["rules_name_id"] is None or selected.setdefault(face_id, face) != face:
            raise ValueError("Ambiguous published battle rule anchor")
        return {"kind": "face-id", "region": region, "face_id": face_id}

    for raw in array(rules["titles"]):
        title = object_value(raw)
        title["anchor"] = anchor(title["anchor"])
    for raw in array(rules["resources"]):
        resource = object_value(raw)
        if "template" in resource:
            resource["template"] = anchor(resource["template"])
        for raw_name in array(resource["names"]):
            name = object_value(raw_name)
            name["face"] = anchor(name["face"])
    source["kind"] = "resolved"
    projected: list[JsonValue] = list(selected.values())
    return {
        "Resolved": {
            "region": region,
            "snapshot_sha256": source["snapshot_sha256"],
            "faces": projected,
        }
    }

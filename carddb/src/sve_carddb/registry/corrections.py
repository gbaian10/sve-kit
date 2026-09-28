"""Project confirmed corrections without mutating source observations."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.registry.build import string
from sve_carddb.registry.inputs import digest

if TYPE_CHECKING:
    from sve_carddb.registry.review import Inputs
    from sve_carddb.registry.storage import Entry


def project_corrections(
    inputs: Inputs, entries: list[Entry]
) -> list[dict[str, JsonValue]]:
    """Build field-local values and snapshot Correction markers from pinned sources.

    This is a build-stage projection, not a complete published card snapshot.
    Conflicts remain unapplied and must block the caller's publication step.
    """
    printings = {
        string(entry.data, "id"): entry.data
        for entry in entries
        if entry.kind == "printing"
    }
    result: list[dict[str, JsonValue]] = []
    for entry in entries:
        if entry.kind != "source_correction" or entry.data["state"] != "active":
            continue
        data = entry.data
        printing = printings[string(data, "printing_id")]
        region = string(printing, "region")
        card = (inputs.jp if region == "jp" else inputs.en)[string(printing, "card_no")]
        maps = printing["source_face_map"]
        if not isinstance(maps, list):
            raise TypeError("Invalid source face map")
        index = next(
            index
            for index, mapping in enumerate(maps)
            if isinstance(mapping, dict) and mapping["face_id"] == data["face_id"]
        )
        face = card.faces[index]
        field = string(data, "field")
        if field not in {"effect", "card_type"}:
            raise ValueError("Unsupported correction field")
        raw = (
            face.text
            if field == "effect"
            else (face.card_type if region == "jp" else face.info["Card Type"])
        )
        source_hash = digest(card.model_dump(mode="json"))
        matches = (
            raw == data["expected_raw_value"]
            and source_hash == data["expected_source_hash"]
        )
        status = (
            "already_fixed"
            if raw == data["corrected_value"]
            else "applied"
            if matches
            else "conflict"
        )
        marker: list[JsonValue] = []
        if status == "applied":
            marker.append(
                {
                    "field": field,
                    "corrected_from": raw,
                    "is_corrected": True,
                    "reason": data["reason"],
                }
            )
        result.append(
            {
                "correction_id": data["id"],
                "printing_id": data["printing_id"],
                "face_id": data["face_id"],
                "region": region,
                "card_no": card.number,
                "field": field,
                "source_observation_hash": source_hash,
                "status": status,
                "value": data["corrected_value"] if status == "applied" else raw,
                "corrections": marker,
            }
        )
    return result

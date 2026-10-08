"""Project confirmed corrections without mutating source observations."""

from typing import TYPE_CHECKING, Literal

from pydantic import JsonValue

from sve_carddb.domains.registry.build import string
from sve_carddb.domains.registry.inputs import digest

if TYPE_CHECKING:
    from sve_carddb.domains.registry.review import Inputs
    from sve_carddb.domains.registry.storage import Entry

Status = Literal["applied", "already_fixed", "conflict"]


def correction_status(
    raw: str | None,
    observation_hash: str,
    *,
    expected: str,
    corrected: str,
    expected_hash: str,
) -> Status:
    """Check upstream fixes before exact old bytes and the complete observation pin."""
    if raw == corrected:
        return "already_fixed"
    if raw == expected and observation_hash == expected_hash:
        return "applied"
    return "conflict"


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
        status = correction_status(
            raw,
            source_hash,
            expected=string(data, "expected_raw_value"),
            corrected=string(data, "corrected_value"),
            expected_hash=string(data, "expected_source_hash"),
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

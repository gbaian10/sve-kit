"""Identify Vanguard printing faces through their adopted regional revisions."""

from pydantic import JsonValue

from sve_carddb.core.json import array, object_value, string


def vanguard_printing_faces(
    view: dict[str, list[dict[str, JsonValue]]],
) -> dict[tuple[str, str], tuple[JsonValue, JsonValue]]:
    """Resolve each eligible permanent printing face to its region and card number."""
    revisions = {
        (row["id"], row["region"])
        for row in view["face_revision"]
        if "cardfight_vanguard" in array(row["titles"])
    }
    faces = {
        (face["id"], current["region"])
        for face in view["face"]
        for current in map(object_value, array(face["current"]))
        if (current["revision_id"], current["region"]) in revisions
    }
    return {
        (string(printing["id"]), string(face["face_id"])): (
            printing["region"],
            printing["card_no"],
        )
        for printing in view["printing"]
        if printing["card_no_state"] == "official"
        for face in map(object_value, array(printing["faces"]))
        if (face["face_id"], printing["region"]) in faces
    }


def validate_trigger_icons(
    view: dict[str, list[dict[str, JsonValue]]], icons: JsonValue
) -> None:
    """Bind the schema-validated attachment to exact Vanguard printing faces."""
    by_printing = object_value(icons)
    if not by_printing:
        return
    scope = vanguard_printing_faces(view)
    if any(
        (printing_id, face_id) not in scope
        for printing_id, faces in by_printing.items()
        for face_id in object_value(faces)
    ):
        raise ValueError("Trigger icon is outside its Vanguard printing face")

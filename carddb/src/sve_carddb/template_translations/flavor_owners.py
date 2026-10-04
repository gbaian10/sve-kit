"""Flavor owners reuse immutable identity replay without borrowing current effects or names."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.values import digest

if TYPE_CHECKING:
    from sve_carddb.build_db import Database
    from sve_carddb.template_translations.flavor_models import FlavorOwner


def verify_owner(
    db: Database, owner: FlavorOwner, *, source_hash: str, context_source_unit_id: str
) -> None:
    """A downstream context must use this printing's own flavor unit, even with pending effects."""
    printings = db.select(
        "printing", db.columns("printing"), where={"id": owner.printing_id}
    )
    faces = db.select("face", db.columns("face"), where={"id": owner.face_id})
    rows = db.select(
        "printing_face",
        db.columns("printing_face"),
        where={"printing_id": owner.printing_id, "face_id": owner.face_id},
    )
    if len(printings) != 1 or len(faces) != 1 or len(rows) != 1:
        raise ValueError("Flavor printing face owner is absent")
    printed, face, printing = rows[0].values, faces[0].values, printings[0].values
    if (
        face["card_id"] != printing["card_id"]
        or printed["card_id"] != printing["card_id"]
    ):
        raise ValueError("Flavor printing face belongs to another card")
    cards = db.select("card", db.columns("card"), where={"id": printing["card_id"]})
    if len(cards) != 1 or cards[0].values["identity_state"] != "confirmed":
        raise ValueError("Flavor owner requires confirmed card identity")
    if (
        printed["flavor_unit_id"] != owner.flavor_unit_id
        or context_source_unit_id != owner.flavor_unit_id
    ):
        raise ValueError("Flavor context must use its physical owner's flavor unit")
    units = db.select(
        "text_unit", db.columns("text_unit"), where={"id": owner.flavor_unit_id}
    )
    if len(units) != 1:
        raise ValueError("Flavor physical text unit is absent")
    unit = units[0].values
    if (
        printing["region"] != "jp"
        or unit["lang"] != "ja"
        or not isinstance(unit["text"], str)
        or digest(unit["text"].encode()) != source_hash
        or unit["content_hash"] != source_hash
    ):
        raise ValueError("Flavor physical text unit exact hash or language mismatch")

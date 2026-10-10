"""Current JP effect selections supply whole-field views on confirmed paired EN faces."""

from typing import TYPE_CHECKING

from sve_carddb.core.json import string
from sve_carddb.domains.translations.four_layer_storage import payload
from sve_carddb.domains.translations.names.bindings import DisplayBinding

if TYPE_CHECKING:
    from sve_carddb.build import Database


def effect_bindings(db: Database) -> tuple[DisplayBinding, ...]:
    """Region alignment governs rule eligibility independently of JP text display."""
    current = {
        (string(row["face_id"]), string(row["region"])): string(row["revision_id"])
        for stored in db.rows("face_current")
        if (row := payload(stored))
    }
    revisions = {
        string(row["id"]): row
        for stored in db.rows("face_revision")
        if (row := payload(stored))
    }
    faces = {
        string(row["id"]): string(row["card_id"])
        for stored in db.rows("face")
        if (row := payload(stored))
    }
    cards = {
        string(row["id"]): row for stored in db.rows("card") if (row := payload(stored))
    }
    selected = {
        string(row["context_id"])
        for stored in db.rows("translation_selection")
        if (row := payload(stored)) and row["target_lang"] == "zh-Hant"
    }
    result = []
    for stored in db.rows("translation_use"):
        use = payload(stored)
        if (
            use["field"] != "effect"
            or use["ordinal"] is not None
            or use["face_revision_id"] is None
            or use["context_id"] not in selected
        ):
            continue
        identifier = string(use["face_revision_id"])
        source = revisions[identifier]
        face = string(source["face_id"])
        receiver_id = current.get((face, "en"))
        if (
            source["region"] != "jp"
            or current.get((face, "jp")) != identifier
            or receiver_id is None
            or cards[faces[face]]["identity_state"] != "confirmed"
        ):
            continue
        receiver = revisions[receiver_id]
        if source["effect_unit_id"] is None or receiver["effect_unit_id"] is None:
            continue
        result.append(
            DisplayBinding(
                string(use["id"]),
                ("face_revision", receiver_id),
                "zh-Hant",
                "jp_source",
            )
        )
    return tuple(
        sorted(result, key=lambda binding: (binding.source_use_id, binding.destination))
    )

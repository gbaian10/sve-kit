"""Current JP names and effects supply whole-field views on confirmed paired EN faces."""

from collections import defaultdict
from typing import TYPE_CHECKING

from sve_carddb.core.json import string
from sve_carddb.domains.translations.four_layer_storage import payload
from sve_carddb.domains.translations.names.bindings import DisplayBinding

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database


def display_bindings(
    db: Database,
    *,
    names: tuple[DisplayBinding, ...] = (),
    paired: Mapping[str, str] | None = None,
) -> tuple[DisplayBinding, ...]:
    """Region alignment governs rule eligibility independently of JP text display.

    `paired` maps an EN printing to its reviewed JP printing; an EN printing that
    prints its own name (a merged collab printing) borrows that JP printing's name.
    """
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
    name_translations = {
        binding.source_use_id: binding.translation_id
        for binding in names
        if binding.basis == "own_source"
        and binding.target_lang == "zh-Hant"
        and binding.translation_id is not None
    }
    result = []
    for stored in db.rows("translation_use"):
        use = payload(stored)
        if (
            use["field"] not in {"name", "effect"}
            or use["ordinal"] is not None
            or use["face_revision_id"] is None
        ):
            continue
        translation = name_translations.get(string(use["id"]))
        if (use["field"] == "name" and translation is None) or (
            use["field"] == "effect" and use["context_id"] not in selected
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
        field = string(use["field"]) + "_unit_id"
        if source[field] is None or receiver[field] is None:
            continue
        result.append(
            DisplayBinding(
                string(use["id"]),
                ("face_revision", receiver_id),
                "zh-Hant",
                "jp_source",
                translation if use["field"] == "name" else None,
            )
        )
    result.extend(_printed_names(db, name_translations, current, paired or {}))
    return tuple(
        sorted(result, key=lambda binding: (binding.source_use_id, binding.destination))
    )


def _printed_names(
    db: Database,
    translations: Mapping[str, str],
    current: Mapping[tuple[str, str], str],
    paired: Mapping[str, str],
) -> list[DisplayBinding]:
    observed: dict[tuple[str, str], set[str]] = defaultdict(set)
    for stored in db.rows("printing_face_observation"):
        row = payload(stored)
        if row["revision_id"] is not None:
            observed[string(row["printing_id"]), string(row["face_id"])].add(
                string(row["revision_id"])
            )
    uses = {
        string(row["face_revision_id"]): string(row["id"])
        for stored in db.rows("translation_use")
        if (row := payload(stored))["field"] == "name"
        and row["ordinal"] is None
        and row["face_revision_id"] is not None
    }
    result = []
    for (printing, face), revisions in sorted(observed.items()):
        source = observed.get((paired.get(printing, ""), face), set())
        if len(revisions) != 1 or len(source) != 1:
            continue
        (receiver,), (donor,) = revisions, source
        source_use = uses.get(donor)
        translation = None if source_use is None else translations.get(source_use)
        # The current EN revision already borrows the current JP name above.
        if (
            source_use is None
            or translation is None
            or receiver == current.get((face, "en"))
        ):
            continue
        result.append(
            DisplayBinding(
                source_use,
                ("face_revision", receiver),
                "zh-Hant",
                "jp_source",
                translation,
            )
        )
    return result

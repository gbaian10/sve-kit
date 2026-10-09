"""Prove card source ownership before normalizing or sharing a typed binding."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.contracts.four_layer import FaceRevisionOwner, PrintingFaceOwner
from sve_carddb.core.json import digest
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.translations.sources import pointer

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
    from sve_carddb.contracts.source_binding import SourceDescriptor
    from sve_carddb.domains.translations.sources import Sources


@dataclass(frozen=True)
class CardSource:
    descriptor: SourceDescriptor
    text: str
    card_id: str
    face_id: str
    source_id: str


def _string(value: Value) -> str:
    if not isinstance(value, str) or not value:
        raise ValueError("Card source requires a nonempty typed identifier")
    return value


def _row(db: Database, table: str, key: Mapping[str, Value]) -> Mapping[str, Value]:
    rows = db.select(table, db.columns(table), where=key)
    if len(rows) != 1:
        raise ValueError("Card source exact owner or field is missing")
    return rows[0].values


def card_source(  # ruff: ignore[complex-structure, too-many-branches, too-many-statements, too-many-locals] -- prove the joined owner, field, face, raw version and bytes together
    db: Database, sources: Sources, descriptor: SourceDescriptor
) -> CardSource:
    """Equal bytes on another card or face cannot authorize a source descriptor."""
    owner = descriptor.owner
    if isinstance(owner, FaceRevisionOwner):
        row = _row(db, "face_revision", {"id": owner.revision_id})
        face_id = _string(row["face_id"])
        face = _row(db, "face", {"id": face_id})
        card_id = _string(face["card_id"])
        region = _string(row["region"])
        fields = {"name": "name_unit_id", "effect": "effect_unit_id"}
        section_table, section_key = (
            "face_text_section",
            {"revision_id": owner.revision_id},
        )
    elif isinstance(owner, PrintingFaceOwner):
        row = _row(
            db,
            "printing_face",
            {"printing_id": owner.printing_id, "face_id": owner.face_id},
        )
        printing = _row(db, "printing", {"id": owner.printing_id})
        face_id = owner.face_id
        face = _row(db, "face", {"id": face_id})
        card_id = _string(printing["card_id"])
        if row["card_id"] != card_id or face["card_id"] != card_id:
            raise ValueError("Card source printed face belongs to another card")
        region = _string(printing["region"])
        fields = {
            "name": "printed_name_unit_id",
            "effect": "printed_effect_unit_id",
            "flavor": "flavor_unit_id",
        }
        section_table, section_key = (
            "printing_text_section",
            {"printing_id": owner.printing_id, "face_id": face_id},
        )
    else:
        raise TypeError("Card source requires a card owner")
    if region != "jp":
        raise ValueError(
            "Four-layer normalization requires the owner's Japanese source"
        )
    if _row(db, "card", {"id": card_id})["identity_state"] != "confirmed":
        raise ValueError("Card source identity is not confirmed")
    face_ordinal = face["ordinal"]
    if type(face_ordinal) is not int or face_ordinal < 0:
        raise TypeError("Card source face has an invalid permanent ordinal")
    if descriptor.field == "section":
        section = _row(db, section_table, section_key | {"ordinal": descriptor.ordinal})
        unit_id = _string(section["text_unit_id"])
        locator = f"/faces/{face_ordinal}/sections/{descriptor.ordinal}"
    else:
        unit_id = _string(row[fields[descriptor.field]])
        source_field = "text" if descriptor.field == "effect" else descriptor.field
        locator = f"/faces/{face_ordinal}/{source_field}"
    source_id = _string(row["source_id"])
    unit = _row(db, "text_unit", {"id": unit_id})
    raw = unit["text"]
    if (
        not isinstance(raw, str)
        or unit["lang"] != "ja"
        or unit["content_hash"] != digest(raw.encode())
    ):
        raise ValueError("Card source original text identity is invalid")
    if (
        descriptor.source_unit_id != unit_id
        or descriptor.source_hash != digest(raw.encode())[7:]
    ):
        raise ValueError("Card source descriptor differs from the exact owner field")
    if descriptor.source_ref.locator != locator:
        raise ValueError("Card source locator does not select its exact face field")
    ref = SourceRef(
        batch_id=descriptor.source_ref.batch_id,
        source_version_id=descriptor.source_ref.source_version_id,
        parser=descriptor.source_ref.parser,
        locator=descriptor.source_ref.locator,
        text_hash="sha256:" + descriptor.source_ref.text_hash,
    )
    lang, document, source = sources.document(ref)
    if source.id != source_id or lang != "ja":
        raise ValueError("Card source frozen version belongs to another owner source")
    if pointer(document, locator) != raw:
        raise ValueError("Card source frozen field differs from the exact owner bytes")
    return CardSource(descriptor, raw, card_id, face_id, source_id)

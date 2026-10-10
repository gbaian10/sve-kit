"""Prove card source ownership before normalizing or sharing a typed binding."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.contracts.four_layer import (
    FaceRevisionOwner,
    OwnerField,
    PrintingFaceOwner,
)
from sve_carddb.contracts.source_binding import SourceDescriptor
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.catalog.adoption_models import SourceRef
from sve_carddb.domains.translations.corrected_sources import PARSER as CORRECTED_PARSER
from sve_carddb.domains.translations.four_layer_semantics import CardContext
from sve_carddb.domains.translations.sources import pointer

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.build import Database, Value
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


@dataclass(frozen=True)
class _Field:
    unit_id: str
    text: str
    source_id: str
    locator: str
    card_id: str
    face_id: str
    face_ordinal: int


def _field(  # ruff: ignore[too-many-locals] -- resolve the joined exact owner and field once for both descriptor registration and verification
    db: Database, field: OwnerField, lang: Literal["ja", "en"] = "ja"
) -> _Field:
    owner = field.owner
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
    if region != ("jp" if lang == "ja" else "en"):
        raise ValueError("Card source requires the owner's selected regional source")
    if _row(db, "card", {"id": card_id})["identity_state"] != "confirmed":
        raise ValueError("Card source identity is not confirmed")
    face_ordinal = face["ordinal"]
    if type(face_ordinal) is not int or face_ordinal < 0:
        raise TypeError("Card source face has an invalid permanent ordinal")
    if field.field == "section":
        section = _row(db, section_table, section_key | {"ordinal": field.ordinal})
        unit_id = _string(section["text_unit_id"])
        locator = f"/faces/{face_ordinal}/sections/{field.ordinal}"
    else:
        unit_id = _string(row[fields[field.field]])
        source_field = "text" if field.field == "effect" else field.field
        locator = f"/faces/{face_ordinal}/{source_field}"
    source_id = _string(row["source_id"])
    unit = _row(db, "text_unit", {"id": unit_id})
    raw = unit["text"]
    if (
        not isinstance(raw, str)
        or unit["lang"] != lang
        or unit["content_hash"] != digest(raw.encode())
    ):
        raise ValueError("Card source original text identity is invalid")
    return _Field(unit_id, raw, source_id, locator, card_id, face_id, face_ordinal)


def descriptor(
    db: Database,
    sources: Sources,
    field: OwnerField,
    batch_id: str,
    *,
    lang: Literal["ja", "en"] = "ja",
) -> SourceDescriptor:
    """Register the actual owner field rather than searching other cards by equal text."""
    selected = _field(db, field, lang)
    parser = "translation-jp-v1" if lang == "ja" else "translation-en-v1"
    if (
        lang == "ja"
        and isinstance(field.owner, FaceRevisionOwner)
        and field.field == "effect"
    ):
        applications = db.select(
            "correction_application",
            db.columns("correction_application"),
            where={
                "face_revision_id": field.owner.revision_id,
                "source_id": selected.source_id,
                "status": "applied",
                "result_unit_id": selected.unit_id,
            },
        )
        if applications:
            parser = CORRECTED_PARSER
    checksum = digest(selected.text.encode())[7:]
    result = SourceDescriptor.model_validate_json(
        canonical(
            field.model_dump(mode="json")
            | {
                "source_unit_id": selected.unit_id,
                "source_hash": checksum,
                "source_ref": {
                    "batch_id": batch_id,
                    "source_version_id": selected.source_id,
                    "parser": parser,
                    "locator": selected.locator,
                    "text_hash": checksum,
                },
            }
        )
    )
    card_source(db, sources, result, lang=lang)
    return result


def empty_field(db: Database, field: OwnerField) -> bool:
    """An absent effect is interned as empty text and has no source span to authorize."""
    return not _field(db, field).text


def card_source(
    db: Database,
    sources: Sources,
    descriptor: SourceDescriptor,
    *,
    lang: Literal["ja", "en"] = "ja",
) -> CardSource:
    """Equal bytes on another card or face cannot authorize a source descriptor."""
    selected = _field(db, descriptor, lang)
    unit_id, raw, source_id, locator = (
        selected.unit_id,
        selected.text,
        selected.source_id,
        selected.locator,
    )
    owner = descriptor.owner
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
    source_lang, document, source = sources.document(ref)
    if source.id != source_id or source_lang != lang:
        raise ValueError("Card source frozen version belongs to another owner source")
    if pointer(document, locator) != raw:
        raise ValueError(
            "Card source frozen field differs from the exact owner bytes: "
            f"{descriptor.owner.model_dump(mode='json')} {descriptor.field} "
            f"{descriptor.ordinal} {source_id} {locator}"
        )
    if ref.parser == CORRECTED_PARSER:
        if not isinstance(owner, FaceRevisionOwner) or descriptor.field != "effect":
            raise ValueError("Corrected source recipe requires a revision effect field")
        if sources.corrections is None:
            raise ValueError("Corrected source recipe lacks its pinned correction plan")
        applications = db.select(
            "correction_application",
            db.columns("correction_application"),
            where={
                "face_revision_id": owner.revision_id,
                "source_id": source_id,
                "status": "applied",
                "result_unit_id": unit_id,
            },
        )
        if len(applications) != 1:
            raise ValueError("Corrected source requires its exact applied revision")
        sources.corrections.verify_scope(
            ref.batch_id,
            ref.source_version_id,
            selected.face_ordinal,
            selected.card_id,
            selected.face_id,
        )
    return CardSource(descriptor, raw, selected.card_id, selected.face_id, source_id)


def semantic_context(db: Database, source: CardSource) -> CardContext | None:
    """Use the exact source revision, including historical printed observations."""
    owner = source.descriptor.owner
    key: dict[str, Value] = {
        "face_id": source.face_id,
        "region": "jp",
        "source_id": source.source_id,
    }
    if isinstance(owner, FaceRevisionOwner):
        key["id"] = owner.revision_id
    revisions = db.select(
        "face_revision",
        db.columns("face_revision"),
        where=key,
    )
    if len(revisions) != 1:
        return None
    revision = revisions[0].values
    special = {
        _string(row.values["special_kind_code"])
        for row in db.select(
            "face_special_kind",
            db.columns("face_special_kind"),
            where={"revision_id": _string(revision["id"])},
        )
    }
    return CardContext(
        source.descriptor,
        source.card_id,
        source.face_id,
        "evolved"
        if "evolve" in special
        else "advance"
        if "advance" in special
        else "normal",
        _string(revision["type_code"]),
    )

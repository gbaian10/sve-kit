"""Owner-local source lookup shared by current names, links and templates."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.core.json import digest

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.build_db import Database, Value


@dataclass(frozen=True)
class NameOwner:
    kind: Literal["face_revision", "printing_face"]
    identifier: str
    face_id: str | None = None

    def __post_init__(self) -> None:
        """Keep printing pairs complete before reading any database rows."""
        if (
            self.kind not in {"face_revision", "printing_face"}
            or not self.identifier
            or (self.kind == "printing_face") != (self.face_id is not None)
            or (self.face_id is not None and not self.face_id)
        ):
            raise ValueError("Invalid name build owner")

    def payload(self) -> dict[str, JsonValue]:
        """Use the translation-contract §6.3 owner shape in generated use IDs."""
        if self.kind == "face_revision":
            return {"kind": self.kind, "revision_id": self.identifier}
        return {
            "kind": self.kind,
            "printing_id": self.identifier,
            "face_id": self.face_id,
        }


@dataclass(frozen=True)
class NameSource:
    owner: NameOwner
    card_id: str
    face_id: str
    unit_id: str
    lang: str
    text: str
    source_hash: str


def _row(db: Database, table: str, identifier: str) -> dict[str, Value]:
    found = [
        dict(row.values)
        for row in db.select(table, db.columns(table), where={"id": identifier})
    ]
    if len(found) != 1:
        raise ValueError("Name build owner or source row is absent")
    return found[0]


def name_source(db: Database, owner: NameOwner) -> NameSource | None:
    """Read each owner's own original; pending effects do not discard known names."""
    if owner.kind == "face_revision":
        revision = _row(db, "face_revision", owner.identifier)
        face_id, region, unit_id = (
            revision["face_id"],
            revision["region"],
            revision["name_unit_id"],
        )
        face = _row(db, "face", str(face_id))
        card_id = face["card_id"]
    else:
        printing = _row(db, "printing", owner.identifier)
        rows = [
            dict(row.values)
            for row in db.select(
                "printing_face",
                db.columns("printing_face"),
                where={"printing_id": owner.identifier, "face_id": owner.face_id},
            )
        ]
        if len(rows) != 1:
            raise ValueError("Name build printing face is absent")
        printed = rows[0]
        face_id, region, card_id = (
            printed["face_id"],
            printing["region"],
            printing["card_id"],
        )
        face = _row(db, "face", str(face_id))
        if printed["card_id"] != card_id or face["card_id"] != card_id:
            raise ValueError("Name build printing face belongs to another card")
        if printed["printed_text_state"] in {"unknown", "omitted"}:
            return None
        if printed["printed_name_unit_id"] is None:
            raise ValueError("Known printed name is missing its own source")
        unit_id = printed["printed_name_unit_id"]
    if _row(db, "card", str(card_id))["identity_state"] != "confirmed":
        return None
    unit = _row(db, "text_unit", str(unit_id))
    text = unit["text"]
    if (
        region not in {"jp", "en"}
        or unit["lang"] != {"jp": "ja", "en": "en"}[str(region)]
    ):
        raise ValueError("Name build source language differs from owner region")
    if not isinstance(text, str) or unit["content_hash"] != digest(text.encode()):
        raise ValueError("Name build source exact hash mismatch")
    return NameSource(
        owner,
        str(card_id),
        str(face_id),
        str(unit_id),
        str(unit["lang"]),
        text,
        str(unit["content_hash"]),
    )

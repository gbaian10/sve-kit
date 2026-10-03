"""Name-only uses over verified publication rows; no automatic policy selection."""

from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal

from sve_carddb.build_db.rows import insert_exact
from sve_carddb.build_db.t2_translation import OWNERS
from sve_carddb.snapshot.values import canonical, digest

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
    _row(db, "card", str(card_id))
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


def default_name_context(db: Database, owner: NameOwner) -> str | None:
    """Default semantics require no new adoption or per-card variant."""
    source = name_source(db, owner)
    if source is None:
        return None
    payload: dict[str, JsonValue] = {
        "recipe": "context-v1",
        "source_unit_id": source.unit_id,
        "semantic_variant": "default",
    }
    identifier = "ctx:" + digest(canonical(payload))[7:]
    insert_exact(
        db,
        "translation_context",
        {
            "id": identifier,
            "source_unit_id": source.unit_id,
            "semantic_variant": "default",
            "decision_id": None,
        },
        ("id",),
    )
    return identifier


def bind_name_use(db: Database, owner: NameOwner, context_id: str) -> str:
    """Revalidate the real owner even when another owner already uses the context."""
    source = name_source(db, owner)
    if source is None:
        raise ValueError("Unknown printed name cannot acquire a translation use")
    context = _row(db, "translation_context", context_id)
    if context["source_unit_id"] != source.unit_id:
        raise ValueError("Name use context does not match its owner source")
    if context["semantic_variant"] != "default":
        raise ValueError("Name variants require complete assignment replay support")
    identifier = (
        "use:"
        + digest(
            canonical(
                {
                    "recipe": "use-v1",
                    "owner": owner.payload(),
                    "context_id": context_id,
                    "field": "name",
                    "ordinal": None,
                }
            )
        )[7:]
    )
    values: dict[str, Value] = {name: None for group in OWNERS for name in group}
    values.update(id=identifier, context_id=context_id, field="name", ordinal=None)
    if owner.kind == "face_revision":
        values["face_revision_id"] = owner.identifier
    else:
        values.update(printing_id=owner.identifier, face_id=owner.face_id)
    insert_exact(db, "translation_use", values, ("id",))
    return identifier


def select_unofficial_name(
    db: Database, context_id: str, lang: str, translation_id: str
) -> None:
    """Official names need later per-owner evidence rather than a shared selection."""
    translation = _row(db, "translation", translation_id)
    if translation["authority"] != "unofficial":
        raise ValueError(
            "Shared official name selection requires owner eligibility checks"
        )
    if (
        translation["context_id"] != context_id
        or translation["target_lang"] != lang
        or translation["status"] != "reviewed"
    ):
        raise ValueError(
            "Name selection must match an exact reviewed context and language"
        )
    insert_exact(
        db,
        "translation_selection",
        {
            "context_id": context_id,
            "target_lang": lang,
            "translation_id": translation_id,
        },
        ("context_id", "target_lang"),
    )

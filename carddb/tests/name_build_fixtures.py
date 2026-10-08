"""A verified publication graph with different current, printed and back names."""

from typing import TYPE_CHECKING

from sve_carddb.build_db import Json, create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.core.json import digest

from .build_db_fixtures import rows
from .database_fixtures import DatabaseTemplate

if TYPE_CHECKING:
    from sve_carddb.build_db import Database, Value


def populate(db: Database) -> None:
    """Seed genuine owner relationships without a current-only name assumption."""
    baseline = rows()
    baseline["text_unit"]["content_hash"] = digest(b"Synthetic text")
    baseline["printing_face"].update(
        printed_text_state="verified", printed_name_unit_id="printed-name"
    )
    for table, row in reversed(baseline.items()):
        db.insert(table, row)
    db.insert(
        "card", baseline["card"] | {"id": "old_card", "identity_state": "retired"}
    )
    for kind in ("trait", "title", "special_kind", "class", "rarity", "frame"):
        db.insert(
            "vocabulary", baseline["vocabulary"] | {"kind": kind, "code": "synthetic"}
        )
    db.insert(
        "language",
        {"code": "en", "display_name": "English", "fallback_order": Json([])},
    )
    db.insert(
        "language",
        {
            "code": "zh-Hant",
            "display_name": "Synthetic target",
            "fallback_order": Json([]),
        },
    )
    for identifier, lang, text in (
        ("printed-name", "ja", "Synthetic old name"),
        ("back-name", "ja", "Synthetic back name"),
        ("other-name", "ja", "Synthetic other name"),
        ("english-name", "en", "Synthetic English name"),
    ):
        db.insert(
            "text_unit",
            {
                "id": identifier,
                "lang": lang,
                "text": text,
                "content_hash": digest(text.encode()),
            },
        )
    db.update("card", {"id": "card"}, {"layout": "double_faced"})
    db.insert("face", {"id": "back", "card_id": "card", "ordinal": 1, "side": "back"})
    for identifier, face, unit, region in (
        ("back-revision", "back", "back-name", "jp"),
        ("other-revision", "face", "other-name", "jp"),
        ("english-revision", "face", "english-name", "en"),
    ):
        revision: dict[str, Value] = dict(baseline["face_revision"])
        revision.update(id=identifier, face_id=face, name_unit_id=unit, region=region)
        if identifier == "other-revision":
            revision["revision"] = 1
        # English effect text must have its own language even in a name-only test.
        if region == "en":
            revision["effect_unit_id"] = unit
        db.insert("face_revision", revision)


def template() -> DatabaseTemplate:
    """Share expensive DDL compilation; every test gets an isolated connection."""
    schema = compile_build(("t0", "translation_names"))
    with create_database(schema) as db:
        with db.transaction():
            populate(db)
        return DatabaseTemplate(schema, db._connection.serialize())

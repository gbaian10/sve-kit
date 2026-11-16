"""Ephemeral synthetic producer bytes for Rust's native format compatibility test."""

from dataclasses import replace
from pathlib import Path
from tempfile import TemporaryDirectory
from typing import TYPE_CHECKING

from sve_carddb.build import create_database
from sve_carddb.core.json import digest
from sve_carddb.export.media import prepare_media
from sve_carddb.export.preview import Roots, write_preview
from sve_carddb.export.project import project
from sve_carddb.export.read_api import load_battle_inputs
from sve_carddb.export.transport import Batch, Ownership, Snapshot, export_snapshot
from tests.support.build_db_fixtures import rows as core_rows
from tests.support.snapshot_project_fixtures import (
    SETTINGS,
    TEXT,
    decisions,
    populate,
    schema,
)

if TYPE_CHECKING:
    from sve_carddb.build import Database


def _amulet(db: Database) -> None:
    replacements = {
        "card": "card-amulet",
        "face": "face-amulet",
        "printing": "printing-amulet",
        "revision": "revision-amulet",
        "text": TEXT,
    }
    rows = core_rows()
    for table in [
        "card",
        "face",
        "printing",
        "face_revision",
        "printing_face",
        "face_current",
        "printing_face_observation",
    ]:
        row = {
            key: replacements.get(value, value) if isinstance(value, str) else value
            for key, value in rows[table].items()
        }
        if table == "printing":
            row["card_no"] = "TEST-002"
        elif table == "face_revision":
            row.update(
                {
                    "type_code": "amulet",
                    "class_code": "synthetic",
                    "cost": 1,
                    "attack": None,
                    "defense": None,
                }
            )
        db.insert(table, row)
    db.insert("card_int_id", {"int_id": 20002, "printing_id": "printing-amulet"})
    db.insert(
        "vocabulary",
        {"kind": "type", "code": "amulet", "label_unit_id": TEXT, "active": True},
    )


def snapshot() -> Snapshot:
    """One complete synthetic export with native-representable characteristics."""
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
            _amulet(db)
            db.delete(
                "face_special_kind",
                {"revision_id": "revision", "special_kind_code": "synthetic"},
            )
            db.update(
                "face_revision",
                {"id": "revision"},
                {"class_code": "synthetic", "cost": 1, "attack": 2, "defense": 3},
            )
            for text, kind, code in [
                ("ニュートラル", "class", "synthetic"),
                ("フォロワー", "type", "follower"),
                ("アミュレット", "type", "amulet"),
            ]:
                identifier = "t:ja:" + digest(text.encode())[7:23]
                db.insert(
                    "text_unit",
                    {
                        "id": identifier,
                        "lang": "ja",
                        "text": text,
                        "content_hash": digest(text.encode()),
                    },
                )
                db.update(
                    "vocabulary",
                    {"kind": kind, "code": code},
                    {"label_unit_id": identifier},
                )
        projection = project(
            db, regions=("jp",), settings=SETTINGS, decisions=decisions()
        )
        projection = replace(
            projection,
            tables=projection.tables
            | {
                "printing_image": [
                    row | {"availability": "unfetched", "publication_state": "pending"}
                    for row in projection.tables["printing_image"]
                ],
                "image_variant": [],
            },
        )
        projection = prepare_media(projection, None, revision=1).projection
        return export_snapshot(
            projection,
            Ownership.from_database(db, projection),
            Batch(
                "preview-20261011T010203Z-0001",
                "2026-10-11T01:02:03Z",
                "2026-10-11",
                ("jp",),
            ),
        )


def generate() -> bytes:
    """Exercise the complete verified reader without claiming real host admission."""
    with TemporaryDirectory(prefix="battle-projection-") as temporary:
        roots = Roots(Path(temporary) / "public", Path(temporary) / "private")
        write_preview(snapshot(), roots, {})
        return load_battle_inputs(roots.preview, "jp").snapshot

"""Synthetic image/CR graph; reusable for fresh builds without nested transactions."""

from typing import TYPE_CHECKING

from sve_carddb.build.t1_cr import TABLES as CR_TABLES
from sve_carddb.build.t1_images import TABLES as IMAGE_TABLES

from .build_db_fixtures import DATE, HASH
from .build_db_fixtures import rows as t0_rows

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value


TABLES = (*CR_TABLES, *IMAGE_TABLES)


def rows() -> dict[str, dict[str, Value]]:
    values: dict[str, dict[str, Value]] = {
        "image_asset": {
            "id": "image",
            "publication_state": "approved",
            "source_id": "source",
            "source_url": "https://example.invalid/source.png",
            "source_src_raw": "../source.png",
            "content_hash": HASH,
            "mime": "image/png",
            "width": 459,
            "height": 641,
            "bytes": 123,
            "availability": "available",
        },
        "printing_image": {
            "printing_id": "printing",
            "face_id": "face",
            "image_id": "image",
        },
        "image_size": {
            "key": "card_s",
            "purpose": "card",
            "max_width": 128,
            "max_height": 179,
            "is_original": False,
        },
        "image_variant": {
            "image_id": "image",
            "size_key": "card_s",
            "format": "webp",
            "path": "images/sha256/aa/" + "a" * 64 + ".webp",
            "width": 128,
            "height": 179,
            "bytes": 99,
            "sha256": HASH,
            "recipe_version": "synthetic-v1",
        },
        "cr_version": {
            "id": "cr",
            "region": "jp",
            "version": "synthetic-1",
            "published_on": DATE,
            "source_id": "source",
            "source_url": "https://example.invalid/rules.pdf",
        },
        "cr_clause": {
            "id": "clause",
            "cr_version_id": "cr",
            "number": "1.10.2",
            "text_unit_id": "text",
        },
    }
    for table in TABLES:
        for column in table.columns:
            if column.nullable:
                values[table.name].setdefault(column.name, None)
    return values


def populate(db: Database) -> None:
    values = t0_rows()
    for name, row in reversed((values | rows()).items()):
        db.insert(name, row)
    db.insert("card", values["card"] | {"id": "old_card", "identity_state": "retired"})
    for kind in ("trait", "title", "special_kind", "class", "rarity", "frame"):
        db.insert(
            "vocabulary", values["vocabulary"] | {"kind": kind, "code": "synthetic"}
        )
    db.update(
        "rules_profile_revision", {"id": "profile_revision"}, {"cr_version_id": "cr"}
    )

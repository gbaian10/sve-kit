"""Reviewed image marks retain identity, absence and unknown states."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.domains.registry.marks import (
    MarkShard,
    PrintingMark,
    Stamp,
    load_marks,
    populate_marks,
)
from sve_carddb.export.project.source import Source
from tests.support.schema_helpers import compile_t0

from ...support.build_db_fixtures import seed

if TYPE_CHECKING:
    from pathlib import Path


def write(authored: Path, record: PrintingMark) -> Path:
    path = authored / "catalog/printing-marks/TEST/001.yaml"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        MarkShard(
            format=1, kind="printing_marks_shard", records=(record,)
        ).model_dump_json()
    )
    return path


def record() -> PrintingMark:
    return PrintingMark(
        printing_id="printing",
        region="jp",
        card_no="TEST-001Ⓢa",
        stamp=Stamp(kind="anniversary", year=None),
        serial=True,
    )


def test_reviewed_stamp_and_serial_reach_public_projection(tmp_path: Path) -> None:
    write(tmp_path, record())
    reviewed = load_marks(tmp_path)
    with create_database(compile_t0()) as db:
        seed(db)
        assert db.rows("printing")[0].values["serial"] is None
        with db.transaction():
            populate_marks(db, reviewed)
        rows = Source(db).rows("printing", "id,stamp,serial")
        assert rows == [
            {
                "id": "printing",
                "stamp": {"kind": "anniversary", "year": None},
                "serial": True,
            }
        ]
        assert db.rows("printing")[0].values["stamp"] == Json(
            {"kind": "anniversary", "year": None}
        )
        with db.transaction():
            populate_marks(
                db, (record().model_copy(update={"stamp": None, "serial": False}),)
            )
        assert Source(db).rows("printing", "stamp,serial") == [
            {"stamp": None, "serial": False}
        ]


@pytest.mark.parametrize(
    "changes", [{"printing_id": "absent"}, {"region": "en"}, {"card_no": "TEST-002"}]
)
def test_unknown_or_mismatched_identity_is_rejected(changes: dict[str, str]) -> None:
    with create_database(compile_t0()) as db:
        seed(db)
        with pytest.raises(
            ValueError, match=r"Unknown printing mark|identity mismatch"
        ):
            populate_marks(db, (record().model_copy(update=changes),))


def test_shards_reject_duplicates_and_malformed_input(tmp_path: Path) -> None:
    path = write(tmp_path, record())
    other = path.with_name("002.yaml")
    other.write_bytes(path.read_bytes())
    with pytest.raises(ValueError, match="Duplicate"):
        load_marks(tmp_path)
    other.unlink()
    path.write_text(path.read_text().replace('"serial":true', '"serial":"yes"'))
    with pytest.raises(ValueError, match="Invalid printing marks"):
        load_marks(tmp_path)


def test_unreviewed_catalog_has_no_implied_marks(tmp_path: Path) -> None:
    assert load_marks(tmp_path) == ()

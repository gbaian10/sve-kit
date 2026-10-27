"""Confirmed leader frames are validated independently of official rarity."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.domains.registry.leader_frames import (
    FILE,
    load_leader_frames,
    populate_leader_frames,
)
from tests.support.schema_helpers import compile_t0

from ...support.build_db_fixtures import seed

if TYPE_CHECKING:
    from pathlib import Path


def authored(tmp_path: Path, records: str) -> Path:
    path = tmp_path / FILE
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        "format: 1\nkind: printing_leader_name_frames\nrecords:\n" + records
    )
    return tmp_path


@pytest.mark.parametrize("frame", ["gold", "white"])
def test_only_existing_leaders_accept_frames(tmp_path: Path, frame: str) -> None:
    row = f"- printing_id: printing\n  region: jp\n  card_no: TEST-001Ⓢa\n  leader_name_frame: {frame}\n"
    records = load_leader_frames(authored(tmp_path, row))
    with create_database(compile_t0()) as db:
        seed(db)
        assert db.rows("printing")[0].values["leader_name_frame"] is None
        with db.transaction():
            with pytest.raises(ValueError, match="requires a leader"):
                populate_leader_frames(db, records)
            db.insert(
                "vocabulary",
                {
                    "kind": "type",
                    "code": "leader",
                    "active": True,
                    "label_unit_id": "text",
                },
            )
            db.update("face_revision", {"id": "revision"}, {"type_code": "leader"})
            populate_leader_frames(db, records)
        printing = db.rows("printing")[0].values
        assert printing["leader_name_frame"] == frame
        assert not printing["rarity_raw"]
    with pytest.raises(ValueError, match="Duplicate"):
        load_leader_frames(authored(tmp_path, row + row))


@pytest.mark.parametrize("frame", ["silver", "null"])
def test_rejects_invalid_frame(tmp_path: Path, frame: str) -> None:
    with pytest.raises(ValueError, match="Invalid leader frame"):
        load_leader_frames(
            authored(
                tmp_path,
                f"- printing_id: missing\n  region: jp\n  card_no: TEST\n  leader_name_frame: {frame}\n",
            )
        )


def test_rejects_unknown_printing(tmp_path: Path) -> None:
    records = load_leader_frames(
        authored(
            tmp_path,
            "- printing_id: missing\n  region: jp\n  card_no: TEST\n  leader_name_frame: gold\n",
        )
    )
    with (
        create_database(compile_t0()) as db,
        pytest.raises(ValueError, match="Unknown leader frame"),
    ):
        populate_leader_frames(db, records)

"""Badge adoption preserves official rarity and distinguishes missing from absent."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t0 import compile_t0
from sve_carddb.domains.registry.badges import FILE, load_badges, populate_badges
from sve_carddb.domains.registry.rarity import populate

from ...support.build_db_fixtures import seed

if TYPE_CHECKING:
    from pathlib import Path


def authored(tmp_path: Path, records: str) -> Path:
    path = tmp_path / FILE
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("format: 1\nkind: printing_rarity_badges\nrecords:\n" + records)
    return tmp_path


def test_appearance_does_not_change_official_rarity(tmp_path: Path) -> None:
    records = load_badges(
        authored(
            tmp_path,
            "- printing_id: printing\n  region: jp\n  card_no: TEST-001\n  style: bronze\n  text: BR\n",
        )
    )
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction():
            db.update("printing", {"id": "printing"}, {"rarity_raw": "PR"})
            populate(db)
            assert populate_badges(db, ()) == [
                {"printing_id": "printing", "region": "jp", "card_no": "TEST-001Ⓢa"}
            ]
            assert populate_badges(db, records) == []
        printing = db.rows("printing")[0].values
        assert (
            printing["rarity_code"],
            printing["rarity_raw"],
            printing["premium"],
        ) == ("pr", "PR", False)
        assert printing["rarity_badge_text"] == "BR"


@pytest.mark.parametrize(
    "style", ["bronze", "silver", "gold", "red", "pastel", "rainbow", "none"]
)
def test_styles_and_absent_text(tmp_path: Path, style: str) -> None:
    text = "" if style == "none" else "  text: PR\n"
    records = load_badges(
        authored(
            tmp_path,
            f"- printing_id: printing\n  region: jp\n  card_no: TEST-001\n  style: {style}\n"
            + text,
        )
    )
    assert records[0].style == style


@pytest.mark.parametrize(
    "fields",
    [
        "style: unknown",
        "style: gold",
        "style: none\n  text: PR",
        "style: none\n  extra: true",
        "style: none\n  style: gold",
    ],
)
def test_rejects_invalid_records(tmp_path: Path, fields: str) -> None:
    with pytest.raises(
        ValueError, match=r"Invalid badge|Invalid authored|Duplicate|duplicate"
    ):
        load_badges(
            authored(
                tmp_path,
                "- printing_id: printing\n  region: jp\n  card_no: TEST-001\n  "
                + fields
                + "\n",
            )
        )


def test_duplicate_and_unknown_printing(tmp_path: Path) -> None:
    row = "- printing_id: unknown\n  region: jp\n  card_no: TEST-001\n  style: none\n"
    with pytest.raises(ValueError, match="Duplicate"):
        load_badges(authored(tmp_path, row + row))
    records = load_badges(authored(tmp_path, row))
    with (
        create_database(compile_t0()) as db,
        db.transaction(),
        pytest.raises(ValueError, match="Unknown badge"),
    ):
        populate_badges(db, records)

"""Normalization preserves raw evidence, including labels without a known base tier."""

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t0 import compile_t0
from sve_carddb.domains.registry.rarity import normalize, populate

from ...support.build_db_fixtures import seed


@pytest.mark.parametrize(
    ("jp", "en", "code"),
    [
        ("BR", "Bronze", "br"),
        ("SR", "Silver", "sr"),
        ("GR", "Gold", "gr"),
        ("LG", "Legendary", "lg"),
        ("SL", "Super Legendary", "sl"),
        ("UR", "Ultimate", "ur"),
        ("SP", "Special", "sp"),
        ("SSP", "Super Special", "ssp"),
        ("PR", "Promo", "pr"),
        ("-", "-", "unknown"),
        ("プレミアム", "Premium", "premium"),
    ],
)
def test_region_labels_share_a_code(jp: str, en: str, code: str) -> None:
    assert normalize("jp", jp) == normalize("en", en) == (code, code == "premium")


@pytest.mark.parametrize(
    ("jp", "en", "code"),
    [
        ("BR・プレミアム", "Bronze / Premium", "br"),
        ("SR・プレミアム", "Silver / Premium", "sr"),
        ("GR・プレミアム", "Gold / Premium", "gr"),
        ("LG・プレミアム", "Legendary / Premium", "lg"),
    ],
)
def test_premium_keeps_its_base_tier(jp: str, en: str, code: str) -> None:
    assert normalize("jp", jp) == normalize("en", en) == (code, True)


def test_build_keeps_raw_and_reports_unrecognized_values() -> None:
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction():
            db.update("printing", {"id": "printing"}, {"rarity_raw": "GR・プレミアム"})
            assert populate(db) == []
        printing = db.rows("printing")[0].values
        assert (
            printing["rarity_raw"],
            printing["rarity_code"],
            printing["premium"],
        ) == ("GR・プレミアム", "gr", True)
        with db.transaction():
            db.update("printing", {"id": "printing"}, {"rarity_raw": "Future rarity"})
            assert populate(db) == [
                {
                    "printing_id": "printing",
                    "region": "jp",
                    "rarity_raw": "Future rarity",
                    "reason": "unrecognized_rarity",
                }
            ]
        assert db.rows("printing")[0].values["rarity_code"] is None


def test_unknown_region_fails() -> None:
    with pytest.raises(ValueError, match="Unsupported rarity region"):
        normalize("other", "br")

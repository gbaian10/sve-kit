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
    ],
)
def test_region_labels_share_a_code(jp: str, en: str, code: str) -> None:
    assert normalize("jp", jp) == normalize("en", en) == (code, False)


@pytest.mark.parametrize(
    ("jp", "en", "premium"), [("-", "-", None), ("プレミアム", "Premium", True)]
)
def test_labels_without_a_base_tier_adopt_no_code(
    jp: str, en: str, premium: bool | None
) -> None:
    assert normalize("jp", jp) == normalize("en", en) == (None, premium)


@pytest.mark.parametrize(
    ("region", "raw"),
    [
        ("jp", "-・プレミアム"),
        ("jp", "プレミアム・プレミアム"),
        ("en", "- / Premium"),
        ("en", "BR"),
        ("jp", "Bronze"),
    ],
)
def test_unrecognized_combinations_are_not_guessed(region: str, raw: str) -> None:
    assert normalize(region, raw) is None


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
        assert (
            db.rows("printing")[0].values["rarity_code"],
            db.rows("printing")[0].values["premium"],
        ) == (None, None)
        with db.transaction():
            db.update("printing", {"id": "printing"}, {"rarity_raw": "プレミアム"})
            assert populate(db) == []
        assert (
            db.rows("printing")[0].values["rarity_code"],
            db.rows("printing")[0].values["premium"],
        ) == (None, True)


def test_label_is_the_jp_card_face_label_when_en_is_staged_first() -> None:
    with create_database(compile_t0()) as db:
        values = seed(db)
        with db.transaction():
            # Primary-key order stages this EN printing before the seeded JP one.
            db.insert(
                "printing",
                values["printing"]
                | {
                    "id": "en-printing",
                    "region": "en",
                    "card_no": "TEST-001EN",
                    "rarity_raw": "Super Legendary",
                },
            )
            db.update("printing", {"id": "printing"}, {"rarity_raw": "SL"})
            assert populate(db) == []
        texts = {row.values["id"]: row.values for row in db.rows("text_unit")}
        label = next(
            texts[row.values["label_unit_id"]]
            for row in db.rows("vocabulary")
            if (row.values["kind"], row.values["code"]) == ("rarity", "sl")
        )
        assert (label["lang"], label["text"]) == ("ja", "SL")


def test_unknown_region_fails() -> None:
    with pytest.raises(ValueError, match="Unsupported rarity region"):
        normalize("other", "br")

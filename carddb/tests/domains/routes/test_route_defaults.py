"""Default-printing policy through the public selector with synthetic build rows."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t0 import compile_t0
from sve_carddb.domains.registry.rarity import populate
from sve_carddb.domains.routes import build_index, populate_routes
from sve_carddb.domains.routes.defaults import select_defaults

from ...support.build_db_fixtures import rows
from ...support.routes_fixtures import base, printing

if TYPE_CHECKING:
    from sve_carddb.build import Database


def home(db: Database, family: str, region: str, day: str | None) -> None:
    values = rows()
    if family != "family":
        db.insert(
            "product_family",
            values["product_family"]
            | {"id": family, "code": family, "public_code": family},
        )
    pid = family + "-" + region
    db.insert(
        "product",
        values["product"]
        | {
            "id": pid,
            "region": region,
            "released_on": day,
            "date_precision": "day" if day else "unknown",
        },
    )
    db.insert("product_family_member", {"product_id": pid, "family_id": family})


def test_lowest_normalized_rarity_precedes_number_dates_and_processing() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "sl", "BP01-SL01", changes={"rarity_raw": "SL"})
            printing(db, "lg", "BP01-001", changes={"rarity_raw": "LG"})
            printing(
                db, "premium", "BP01-002", changes={"rarity_raw": "BR・プレミアム"}
            )
            populate(db)
        result = select_defaults(db)[0]
        assert (result.printing_id, result.method) == ("premium", "lowest_rarity")
        with db.transaction():
            db.update(
                "printing", {"id": "premium"}, {"rarity_code": "lg", "rarity_raw": "BR"}
            )
        assert select_defaults(db)[0].printing_id == "lg"


@pytest.mark.parametrize(
    ("region", "labels", "expected"),
    [
        ("jp", ("UR", "SL", "SP"), "SL"),
        ("en", ("Ultimate", "Super Legendary", "Special"), "Super Legendary"),
    ],
)
def test_only_high_rarity_printings_still_choose_lowest(
    region: str, labels: tuple[str, ...], expected: str
) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            for label in labels:
                printing(db, label, label, region=region, changes={"rarity_raw": label})
            populate(db)
        result = select_defaults(db)[0]
        assert (result.printing_id, result.method) == (expected, "lowest_rarity")


def test_same_rarity_uses_permanent_home_release_then_card_number_then_id() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            home(db, "early", "jp", "2020-01-01")
            home(db, "late", "jp", "2022-01-01")
            for pid, number, family in (
                ("z-id", "A", "early"),
                ("a-id", "Z", "early"),
                ("late", "0", "late"),
            ):
                printing(
                    db, pid, number, changes={"rarity_raw": "LG", "home_set_id": family}
                )
            # An earlier reprint inclusion does not change the permanent home release order.
            db.update(
                "product",
                {"id": "product"},
                {"released_on": "2010-01-01", "date_precision": "day"},
            )
            db.insert(
                "printing_product", values["printing_product"] | {"printing_id": "late"}
            )
            populate(db)
        assert select_defaults(db)[0].printing_id == "z-id"
        with db.transaction():
            db.update(
                "printing", {"id": "a-id"}, {"card_no": "A", "variant_key": "alternate"}
            )
        assert select_defaults(db)[0].printing_id == "a-id"
        assert db.rows("card")[0].values["home_set_id"] == "family"


def test_home_release_uses_only_same_region_and_known_days() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            home(db, "family", "jp", "2021-01-01")
            home(db, "other", "en", "2010-01-01")
            home(db, "other-jp", "jp", "2022-01-01")
            db.insert(
                "product_family_member",
                {"family_id": "other", "product_id": "other-jp-jp"},
            )
            for pid, family in (("z", "family"), ("a", "other")):
                printing(
                    db, pid, pid, changes={"rarity_raw": "LG", "home_set_id": family}
                )
            populate(db)
        assert select_defaults(db)[0].printing_id == "z"


@pytest.mark.parametrize(
    ("precision", "raw"), [("month", "2010-01"), ("year", "2010"), ("unknown", None)]
)
def test_partial_home_dates_sort_after_known_without_inventing_a_day(
    precision: str, raw: str | None
) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            home(db, "family", "jp", None)
            home(db, "known", "jp", "2020-01-01")
            db.update(
                "product",
                {"id": "family-jp"},
                {"date_precision": precision, "date_raw": raw},
            )
            printing(db, "a", "A", changes={"rarity_raw": "LG"})
            printing(db, "z", "Z", changes={"rarity_raw": "LG", "home_set_id": "known"})
            populate(db)
        assert select_defaults(db)[0].printing_id == "z"


def test_unknown_rarity_keeps_uncertainty_and_never_uses_raw_number_guess() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "unknown", "BP01-001", changes={"rarity_raw": "Unknown"})
            printing(db, "sl", "BP01-SL01", changes={"rarity_raw": "SL"})
            populate(db)
        result = select_defaults(db)[0]
        assert (result.printing_id, result.method) == ("sl", "candidate_lowest_rarity")
        with db.transaction():
            db.update("printing", {"id": "sl"}, {"rarity_code": None})
        result = select_defaults(db)[0]
        assert (result.printing_id, result.method) == ("unknown", "fallback")


def test_groups_are_scoped_to_card_and_region() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "jp", "JP", changes={"rarity_raw": "SL"})
            printing(db, "en", "EN", region="en", changes={"rarity_raw": "Bronze"})
            db.insert("card", values["card"] | {"id": "other"})
            db.insert(
                "printing",
                values["printing"]
                | {
                    "id": "other",
                    "card_id": "other",
                    "card_no": "OTHER",
                    "rarity_raw": "BR",
                },
            )
            populate(db)
        assert [
            (item.card_id, item.region, item.printing_id)
            for item in select_defaults(db)
        ] == [("card", "en", "en"), ("card", "jp", "jp"), ("other", "jp", "other")]


def test_confirmed_override_precedes_rarity_and_keeps_url_identity() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "lg", "LG", changes={"rarity_raw": "LG"})
            printing(db, "sl", "SL", changes={"rarity_raw": "SL"})
            populate(db)
            populate_routes(db)
        before = build_index(db).resolve("/cards/LG")
        assert select_defaults(db)[0].printing_id == "lg"
        with db.transaction():
            db.insert(
                "default_printing_override",
                values["default_printing_override"] | {"printing_id": "sl"},
            )
        result = select_defaults(db)[0]
        assert (result.printing_id, result.method) == ("sl", "override")
        assert build_index(db).resolve("/cards/LG") == before
        with db.transaction():
            db.update("decision", {"id": "decision"}, {"state": "proposed"})
        with pytest.raises(ValueError, match="not confirmed"):
            select_defaults(db)


def test_override_cannot_cross_regions() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        assert select_defaults(db) == ()
        with db.transaction():
            printing(db, "jp", "JP")
            printing(db, "en", "EN", region="en")
        with (
            pytest.raises(sqlite3.IntegrityError, match="Foreign key"),
            db.transaction(),
        ):
            db.insert(
                "default_printing_override",
                values["default_printing_override"] | {"printing_id": "en"},
            )
        assert select_defaults(db)[1].printing_id == "jp"

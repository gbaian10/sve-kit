"""Product family mapping uses confirmed catalog membership, not card origin."""

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t0 import compile_t0
from sve_carddb.domains.products.families import populate
from sve_carddb.export.project.records import initial
from sve_carddb.export.project.source import Source

from ...support.build_db_fixtures import seed


@pytest.mark.parametrize(
    ("code", "expected"),
    [
        ("BP01", ["BP01"]),
        ("bp01-bp02", ["BP01", "BP02"]),
        ("CSD02A", ["CSD02a"]),
        ("DSD01", ["DSD01a", "DSD01b"]),
        (None, ["family"]),
        ("unmatched", ["family"]),
    ],
)
def test_codes_precede_printing_membership(
    code: str | None, expected: list[str]
) -> None:
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction():
            db.delete(
                "product_family_member",
                {"product_id": "product", "family_id": "family"},
            )
            family = dict(db.rows("product_family")[0].values)
            for identifier in ("BP01", "BP02", "CSD02a", "DSD01a", "DSD01b"):
                db.insert(
                    "product_family",
                    family
                    | {
                        "id": identifier,
                        "code": identifier.lower(),
                        "public_code": identifier,
                    },
                )
            db.update("card", {"id": "card"}, {"home_set_id": "BP02"})
            db.update("product", {"id": "product"}, {"product_code": code})
            assert populate(db) == []
        assert initial(Source(db))["product"][0]["family_ids"] == expected
        # The card's original set does not drive the fallback.
        assert db.rows("card")[0].values["home_set_id"] == "BP02"


def test_missing_product_family_is_explicit() -> None:
    with create_database(compile_t0()) as db:
        seed(db)
        with db.transaction():
            db.delete(
                "product_family_member",
                {"product_id": "product", "family_id": "family"},
            )
            db.delete(
                "printing_product", {"printing_id": "printing", "product_id": "product"}
            )
            assert populate(db) == [
                {
                    "product_id": "product",
                    "region": "jp",
                    "product_code": None,
                    "reason": "no_matching_code_or_printing_family",
                }
            ]
        assert initial(Source(db))["product"][0]["family_ids"] == []

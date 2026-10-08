"""Independent default-printing ordering and uncertainty counterexamples."""

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- failed transaction setup is part of rollback verification
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t0 import compile_t0
from sve_carddb.routes import build_index, populate_routes
from sve_carddb.routes.defaults import GeneralEvidence, select_defaults

from .build_db_fixtures import rows
from .routes_fixtures import base, printing

if TYPE_CHECKING:
    from sve_carddb.build import Database, Value

ORDINARY = GeneralEvidence(True, True, False)


def ordinary(
    db: Database, pid: str, number: str, day: str | None, *, region: str = "jp"
) -> None:
    values = rows()
    printing(db, pid, number, region=region, changes={"premium": False})
    db.update(
        "printing_face",
        {"printing_id": pid, "face_id": "face"},
        {
            "signed": False,
            "frame_code": "synthetic",
            "embellishment_state": "confirmed",
        },
    )
    product_id = "product-" + pid
    db.insert("product", values["product"] | {"id": product_id, "region": region})
    db.insert(
        "printing_product",
        values["printing_product"]
        | {
            "printing_id": pid,
            "product_id": product_id,
            "first_available_on": day,
            "first_available_precision": "day" if day else "unknown",
        },
    )


def test_default_is_scoped_to_card_and_region_and_ties_use_permanent_id() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "z", "Z", "2020-01-01")
            ordinary(db, "a", "A", "2020-01-01")
            ordinary(db, "en", "EN", "2010-01-01", region="en")
        result = select_defaults(
            db, general_evidence=dict.fromkeys(("a", "z", "en"), ORDINARY)
        )
        assert [(item.region, item.printing_id, item.method) for item in result] == [
            ("en", "en", "earliest_general"),
            ("jp", "a", "earliest_general"),
        ]


def test_earliest_is_by_inclusion_date_and_not_current_product_release_or_id() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            ordinary(db, "a", "A", "2021-01-01")
            ordinary(db, "z", "Z", "2022-01-01")
            db.insert(
                "printing_product",
                values["printing_product"]
                | {
                    "printing_id": "z",
                    "product_id": "product",
                    "first_available_on": "2019-01-01",
                    "first_available_precision": "day",
                },
            )
        assert (
            select_defaults(db, general_evidence={"a": ORDINARY, "z": ORDINARY})[
                0
            ].printing_id
            == "z"
        )


def test_confirmed_override_precedes_general_dates_and_keeps_url_identity() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            ordinary(db, "a", "A", "2020-01-01")
            ordinary(db, "b", "B", "2021-01-01")
            populate_routes(db)
        before = build_index(db).resolve("/cards/A")
        assert select_defaults(db)[0].printing_id == "a"
        with db.transaction():
            db.insert(
                "default_printing_override",
                values["default_printing_override"] | {"printing_id": "b"},
            )
        assert select_defaults(db)[0].printing_id == "b"
        assert select_defaults(db)[0].method == "override"
        assert build_index(db).resolve("/cards/A") == before
        with pytest.raises(ValueError, match="not confirmed"), db.transaction():
            db.update(
                "decision",
                {"id": "decision"},
                {"state": "proposed"},
            )
            select_defaults(db)


def test_home_general_precedes_earlier_reprint_and_preserves_owner() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            db.insert(
                "product_family",
                values["product_family"]
                | {"id": "reprint", "code": "reprint", "public_code": "REPRINT"},
            )
            ordinary(db, "home", "HOME", "2022-01-01")
            ordinary(db, "reprint", "REPRINT", "2010-01-01")
            db.update("printing", {"id": "reprint"}, {"home_set_id": "reprint"})
        result = select_defaults(
            db, general_evidence={"home": ORDINARY, "reprint": ORDINARY}
        )[0]
        assert (result.printing_id, result.method) == ("home", "earliest_general")
        assert db.rows("card")[0].values["home_set_id"] == "family"


@pytest.mark.parametrize(
    ("printing_changes", "face_changes", "facts"),
    [
        ({"premium": True}, {}, ORDINARY),
        ({}, {"signed": True}, ORDINARY),
        ({}, {}, GeneralEvidence(True, True, True)),
        ({}, {}, GeneralEvidence(True, False, False)),
        ({}, {}, GeneralEvidence(False, True, False)),
    ],
)
def test_known_special_processing_and_non_general_rarity_are_excluded(
    printing_changes: dict[str, Value],
    face_changes: dict[str, Value],
    facts: GeneralEvidence,
) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "special", "SPECIAL", "2010-01-01")
            ordinary(db, "ordinary", "ORDINARY", "2020-01-01")
            if printing_changes:
                db.update("printing", {"id": "special"}, printing_changes)
            if face_changes:
                db.update(
                    "printing_face",
                    {"printing_id": "special", "face_id": "face"},
                    face_changes,
                )
        result = select_defaults(
            db, general_evidence={"special": facts, "ordinary": ORDINARY}
        )[0]
        assert (result.printing_id, result.method) == ("ordinary", "earliest_general")


@pytest.mark.parametrize(
    ("printing_changes", "face_changes", "facts"),
    [
        ({"premium": None}, {}, ORDINARY),
        ({}, {"signed": None}, ORDINARY),
        ({}, {"embellishment_state": "unreviewed"}, ORDINARY),
        ({}, {}, GeneralEvidence(True, None, False)),
        ({}, {}, GeneralEvidence(True, True, None)),
    ],
)
def test_unknown_processing_is_candidate_never_proven_ordinary(
    printing_changes: dict[str, Value],
    face_changes: dict[str, Value],
    facts: GeneralEvidence,
) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a", "A", "2020-01-01")
            if printing_changes:
                db.update("printing", {"id": "a"}, printing_changes)
            if face_changes:
                db.update(
                    "printing_face",
                    {"printing_id": "a", "face_id": "face"},
                    face_changes,
                )
        assert (
            select_defaults(db, general_evidence={"a": facts})[0].method
            == "candidate_general"
        )


def test_missing_classifier_stays_fallback_and_unknown_competitor_prevents_earliest() -> (
    None
):
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a", "A", "2020-01-01")
            ordinary(db, "b", "B", "2010-01-01")
        assert select_defaults(db)[0].method == "fallback"
        assert select_defaults(db)[0].printing_id == "b"
        assert (
            select_defaults(db, general_evidence={"a": ORDINARY})[0].method
            == "candidate_general"
        )


def test_missing_date_competitor_prevents_claiming_earliest() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a", "A", "2020-01-01")
            ordinary(db, "b", "B", None)
        result = select_defaults(db, general_evidence={"a": ORDINARY, "b": ORDINARY})[0]
        assert (result.printing_id, result.method) == ("a", "candidate_general")


def test_partial_dates_do_not_become_first_of_month_or_product_release() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a", "A", None)
            ordinary(db, "z", "Z", "2020-01-01")
            db.update(
                "printing_product",
                {"printing_id": "a", "product_id": "product-a"},
                {
                    "first_available_precision": "month",
                    "first_available_raw": "2010-01",
                },
            )
            db.update(
                "product",
                {"id": "product-a"},
                {"released_on": "2009-01-01", "date_precision": "day"},
            )
        assert select_defaults(db)[0].printing_id == "z"
        assert (
            select_defaults(db, general_evidence={"a": ORDINARY, "z": ORDINARY})[
                0
            ].method
            == "candidate_general"
        )


def test_without_dates_fallback_ties_use_id_and_never_infer_from_number() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "z", "A")
            printing(db, "a", "Z")
        result = select_defaults(db)[0]
        assert (result.printing_id, result.method) == ("a", "fallback")


def test_inclusion_region_is_checked_independently() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "en", "EN", region="en")
            db.insert(
                "printing_product", values["printing_product"] | {"printing_id": "en"}
            )
        with pytest.raises(ValueError, match="printing region"):
            select_defaults(db)


def test_unknown_evidence_targets_and_truthy_non_facts_are_refused() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        assert select_defaults(db) == ()
        with pytest.raises(ValueError, match="absent printing"):
            select_defaults(db, general_evidence={"absent": ORDINARY})
    with pytest.raises(TypeError, match="bool or unknown"):
        GeneralEvidence(general_rarity=1)  # type: ignore[arg-type]  # runtime boundary counterexample


def test_incomplete_double_face_processing_cannot_prove_ordinary() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            db.update("card", {"id": "card"}, {"layout": "double_faced"})
            db.insert(
                "face", values["face"] | {"id": "back", "ordinal": 1, "side": "back"}
            )
            ordinary(db, "a", "A", "2020-01-01")
        assert (
            select_defaults(db, general_evidence={"a": ORDINARY})[0].method
            == "candidate_general"
        )
        with db.transaction():
            db.insert(
                "printing_face",
                values["printing_face"]
                | {
                    "printing_id": "a",
                    "face_id": "back",
                    "signed": False,
                    "embellishment_state": "confirmed",
                },
            )
        assert (
            select_defaults(db, general_evidence={"a": ORDINARY})[0].method
            == "earliest_general"
        )
        with db.transaction():
            db.update(
                "printing_face",
                {"printing_id": "a", "face_id": "back"},
                {"signed": True},
            )
        assert (
            select_defaults(db, general_evidence={"a": ORDINARY})[0].method
            == "fallback"
        )


def test_card_groups_do_not_borrow_another_cards_printings() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            db.insert("card", values["card"] | {"id": "other"})
            db.insert("face", values["face"] | {"id": "other-face", "card_id": "other"})
            printing(db, "a", "A")
            db.insert(
                "printing",
                values["printing"]
                | {"id": "other", "card_id": "other", "card_no": "OTHER"},
            )
        assert [(item.card_id, item.printing_id) for item in select_defaults(db)] == [
            ("card", "a"),
            ("other", "other"),
        ]


def product_date(db: Database, pid: str, day: str, *, precision: str = "day") -> None:
    db.update(
        "product",
        {"id": "product-" + pid},
        {
            "released_on": day if precision == "day" else None,
            "date_precision": precision,
            "date_raw": day if precision in {"month", "year"} else None,
        },
    )


def test_no_inclusion_override_inherits_product_date_2019_before_2022() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a-late", "LATE", None)
            ordinary(db, "z-early", "EARLY", None)
            for pid, day in (("a-late", "2022-01-01"), ("z-early", "2019-01-01")):
                product_date(db, pid, day)
                db.update(
                    "printing_product",
                    {"printing_id": pid, "product_id": "product-" + pid},
                    {"first_available_precision": None},
                )
        result = select_defaults(
            db, general_evidence=dict.fromkeys(("a-late", "z-early"), ORDINARY)
        )[0]
        assert (result.printing_id, result.method) == ("z-early", "earliest_general")
        assert select_defaults(db)[0].printing_id == "z-early"
        assert select_defaults(db)[0].method == "fallback"


@pytest.mark.parametrize(
    ("precision", "raw"), [("month", "2010-01"), ("year", "2010"), ("unknown", None)]
)
def test_explicit_non_day_override_blocks_dated_product(
    precision: str, raw: str | None
) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a-override", "A", None)
            ordinary(db, "z-known", "Z", "2020-01-01")
            product_date(db, "a-override", "2010-01-01")
            db.update(
                "printing_product",
                {"printing_id": "a-override", "product_id": "product-a-override"},
                {"first_available_precision": precision, "first_available_raw": raw},
            )
        result = select_defaults(
            db, general_evidence=dict.fromkeys(("a-override", "z-known"), ORDINARY)
        )[0]
        assert (result.printing_id, result.method) == ("z-known", "candidate_general")
        assert select_defaults(db)[0].printing_id == "z-known"


@pytest.mark.parametrize(
    ("precision", "raw"), [("month", "2010-01"), ("year", "2010"), ("unknown", "")]
)
def test_no_override_does_not_promote_partial_product_dates(
    precision: str, raw: str
) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a-partial", "A", None)
            ordinary(db, "z-known", "Z", "2020-01-01")
            product_date(db, "a-partial", raw, precision=precision)
            db.update(
                "printing_product",
                {"printing_id": "a-partial", "product_id": "product-a-partial"},
                {"first_available_precision": None},
            )
        result = select_defaults(
            db, general_evidence=dict.fromkeys(("a-partial", "z-known"), ORDINARY)
        )[0]
        assert (result.printing_id, result.method) == ("z-known", "candidate_general")


def test_full_day_override_precedes_different_product_date() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            ordinary(db, "a", "A", "2023-01-01")
            ordinary(db, "z", "Z", "2022-01-01")
            product_date(db, "a", "2010-01-01")
        assert (
            select_defaults(db, general_evidence=dict.fromkeys(("a", "z"), ORDINARY))[
                0
            ].printing_id
            == "z"
        )


def test_product_date_requires_an_actual_inclusion() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "a", "A")
            ordinary(db, "z", "Z", "2020-01-01")
            db.update(
                "product",
                {"id": "product"},
                {"released_on": "2010-01-01", "date_precision": "day"},
            )
        result = select_defaults(
            db, general_evidence=dict.fromkeys(("a", "z"), ORDINARY)
        )[0]
        assert (result.printing_id, result.method) == ("z", "candidate_general")

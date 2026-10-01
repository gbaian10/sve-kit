# ruff: file-ignore[pytest-raises-with-multiple-statements] -- transaction rollback assertions require complete failed imports
"""Independent exact/folded/alias/variant counterexamples for card-route-v1."""

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.routes import (
    build_index,
    decode_segment,
    encode_segment,
    populate_routes,
)
from sve_carddb.routes.codec import card_path

from .routes_fixtures import base, printing


@pytest.mark.parametrize(
    ("raw", "encoded"),
    [
        ("BP01-001Ⓢa", "BP01-001%E2%93%88a"),
        (" A ", "%20A%20"),
        ("é", "%C3%A9"),
        ("e\u0301", "e%CC%81"),
        ("%2F", "%252F"),
        ("x+y", "x%2By"),
        ("Ａ", "%EF%BC%A1"),
        ("-._~", "-._~"),
    ],
)
def test_exact_codec_golden(raw: str, encoded: str) -> None:
    assert encode_segment(raw) == encoded
    assert decode_segment(encoded) == raw


@pytest.mark.parametrize(
    "segment", ["", "%", "%0", "%GG", "%FF", "%C0%AF", "%ED%A0%80", "%2F", "%00", "a/b"]
)
def test_decode_rejects_invalid_segments(segment: str) -> None:
    with pytest.raises(ValueError, match=r"Invalid|Malformed|codec|Reserved|Expected"):
        decode_segment(segment)


@pytest.mark.parametrize(
    "raw", ["", "a/b", "a\x00b", pytest.param("\ud800", id="surrogate")]
)
def test_encoder_rejects_invalid_keys(raw: str) -> None:
    with pytest.raises(ValueError, match=r"Invalid|Malformed|codec|Reserved|Expected"):
        encode_segment(raw)


def test_exact_is_prioritized_and_folded_collision_only_disables_folded() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "lower", "TEST-a")
            printing(db, "upper", "TEST-A")
            printing(db, "signed", "TEST-Ⓢa")
            populate_routes(db)
        index = build_index(db)
        assert index.collisions == ("test-a",)
        assert db.rows("build_issue")[0].values["severity"] == "warning"
        assert db.rows("build_issue")[0].values["category"] == "route_folded_collision"
        assert index.resolve("/cards/TEST-a/ignored-slug").printing_id == "lower"
        assert index.resolve("/cards/TEST-A").printing_id == "upper"
        assert index.resolve("/cards/test-a").status == "ambiguous"
        found = index.resolve("/cards/test-sA")
        assert (found.status, found.printing_id, found.canonical_path) == (
            "redirect",
            "signed",
            "/cards/TEST-%E2%93%88a",
        )
        assert index.resolve("/cards/TEST-%E2%93%88a").status == "exact"
        assert index.resolve("/cards/no-such-number").status == "missing"


def test_provisional_never_claims_its_display_number() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "guess", "SAME", state="provisional")
            printing(db, "official", "SAME")
            populate_routes(db)
        index = build_index(db)
        assert index.resolve("/cards/SAME").printing_id == "official"
        assert index.resolve("/cards/_provisional/20002/slug").printing_id == "guess"
        assert index.resolve("/cards/_provisional/20004").status == "missing"
        assert index.resolve("/cards/unimplemented").status == "reserved"
        assert index.resolve("/cards/_provisional").status == "reserved"
        for path in ("/cards/_provisional/020002", "/decks/SAME", "/cards"):
            with pytest.raises(
                ValueError, match=r"Invalid|Malformed|codec|Reserved|Expected"
            ):
                index.resolve(path)


@pytest.mark.parametrize("number", ["unimplemented", "_provisional", "x/y", "x\x00y"])
def test_invalid_official_keys_roll_back_routes(number: str) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with (
            pytest.raises(
                ValueError, match=r"Invalid|Malformed|codec|Reserved|Expected"
            ),
            db.transaction(),
        ):
            printing(db, "bad", number)
            populate_routes(db)
        assert not db.rows("printing")
        assert not db.rows("card_route")


def test_true_cross_region_exact_collision_is_rejected_even_with_override() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with pytest.raises(ValueError, match="Cross-region exact"), db.transaction():
            printing(db, "jp", "SAME")
            printing(db, "en", "SAME", region="en")
            db.insert(
                "route_override",
                {"route_key": "SAME", "printing_id": "jp", "decision_id": "decision"},
            )
            populate_routes(db)
        assert not db.rows("printing")


def test_same_number_variants_require_exact_confirmed_override() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "a", "SAME")
            printing(db, "b", "SAME", changes={"variant_key": "b"})
            with pytest.raises(ValueError, match="variants require"):
                populate_routes(db)
            db.insert(
                "route_override",
                {"route_key": "SAME", "printing_id": "b", "decision_id": "decision"},
            )
            populate_routes(db)
        assert build_index(db).resolve("/cards/SAME").printing_id == "b"
        with pytest.raises(ValueError, match="not confirmed"), db.transaction():
            db.update(
                "decision",
                {"id": "decision"},
                {"state": "proposed", "reviewed_at": None, "reviewed_by": None},
            )
            populate_routes(db)


@pytest.mark.parametrize("key", ["ABSENT", "OTHER"])
def test_override_cannot_select_unrelated_number(key: str) -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with (
            pytest.raises(ValueError, match=r"no official number|exact-number variant"),
            db.transaction(),
        ):
            printing(db, "a", "SAME")
            printing(db, "b", "OTHER")
            db.insert(
                "route_override",
                {"route_key": key, "printing_id": "a", "decision_id": "decision"},
            )
            populate_routes(db)


def test_alias_precedes_folded_and_folded_aliases_share_one_canonical_target() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "a", "NEW")
            printing(db, "b", "old")
            populate_routes(db)
            for key in ("OLD", "Old"):
                db.insert(
                    "card_route_alias",
                    values["card_route_alias"]
                    | {"namespace": "official", "old_key": key, "target_key": "NEW"},
                )
        index = build_index(db)
        assert index.resolve("/cards/OLD").printing_id == "a"
        assert index.resolve("/cards/OLD").status == "redirect"
        assert index.resolve("/cards/old").printing_id == "b"
        assert index.resolve("/cards/oLD").status == "ambiguous"
        with db.transaction():
            db.delete("card_route_alias", {"namespace": "official", "old_key": "Old"})
            db.delete("card_route_alias", {"namespace": "official", "old_key": "OLD"})
            for key in ("OLDER", "Older"):
                db.insert(
                    "card_route_alias",
                    values["card_route_alias"]
                    | {"namespace": "official", "old_key": key, "target_key": "NEW"},
                )
        assert build_index(db).resolve("/cards/older").printing_id == "a"


def test_active_alias_hijack_and_missing_canonical_rows_are_refused() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "a", "A")
            printing(db, "b", "B")
            populate_routes(db)
            db.insert(
                "card_route_alias",
                values["card_route_alias"]
                | {"namespace": "official", "old_key": "A", "target_key": "B"},
            )
        with pytest.raises(ValueError, match="hijack"):
            build_index(db)
        with db.transaction():
            db.delete("card_route_alias", {"namespace": "official", "old_key": "A"})
            db.delete("card_route", {"namespace": "official", "route_key": "A"})
        with pytest.raises(ValueError, match="differ"):
            build_index(db)


def test_populate_is_idempotent_and_refuses_renumbering_without_repair() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "a", "OLD")
            first = populate_routes(db)
            assert populate_routes(db) == first
        with pytest.raises(ValueError, match="identity-repair"), db.transaction():
            db.update("printing", {"id": "a"}, {"card_no": "NEW"})
            populate_routes(db)
        assert build_index(db).resolve("/cards/OLD").printing_id == "a"


def test_missing_permanent_allocation_is_not_replaced_with_row_order() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with pytest.raises(ValueError, match="permanent UInt32"), db.transaction():
            printing(db, "a", "A")
            db.delete("card_int_id", {"int_id": 20002})
            populate_routes(db)


def test_namespace_validation() -> None:
    with pytest.raises(ValueError, match="provisional"):
        card_path("unrecognized", "1")


def test_alias_requires_confirmation_and_direct_canonical_target() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "a", "NEW")
            populate_routes(db)
            db.insert(
                "card_route_alias",
                values["card_route_alias"]
                | {"namespace": "official", "old_key": "OLD", "target_key": "NEW"},
            )
        with pytest.raises(ValueError, match="not confirmed"), db.transaction():
            db.update(
                "decision",
                {"id": "decision"},
                {"state": "proposed", "reviewed_by": None, "reviewed_at": None},
            )
            build_index(db)
        with pytest.raises(ValueError, match="directly target"), db.transaction():
            db.insert(
                "card_route_alias",
                values["card_route_alias"]
                | {"namespace": "official", "old_key": "OLDER", "target_key": "OLD"},
            )
            build_index(db)


def test_provisional_alias_preserves_permanent_entry_without_folding() -> None:
    with create_database(compile_t0()) as db:
        values = base(db)
        with db.transaction():
            printing(db, "a", "OFFICIAL")
            populate_routes(db)
            db.insert(
                "card_route_alias",
                values["card_route_alias"]
                | {"old_key": "20002", "target_key": "OFFICIAL"},
            )
        result = build_index(db).resolve("/cards/_provisional/20002")
        assert (result.status, result.printing_id, result.canonical_path) == (
            "redirect",
            "a",
            "/cards/OFFICIAL",
        )
        assert build_index(db).resolve("/cards/20002").status == "missing"


def test_exact_unicode_and_percent_literals_are_distinct_from_folded_redirects() -> (
    None
):
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "literal", "%2F")
            printing(db, "sharp", "ß")
            printing(db, "composed", "é")
            printing(db, "decomposed", "e\u0301")
            populate_routes(db)
        index = build_index(db)
        assert index.resolve("/cards/%252F").printing_id == "literal"
        assert index.resolve("/cards/SS").printing_id == "sharp"
        assert index.resolve("/cards/%C3%A9").printing_id == "composed"
        assert index.resolve("/cards/e%CC%81").printing_id == "decomposed"
        assert index.resolve("/cards/%C3%89").status == "ambiguous"


def test_folded_diagnostic_is_idempotent_and_conflicts_fail() -> None:
    with create_database(compile_t0()) as db:
        base(db)
        with db.transaction():
            printing(db, "a", "A")
            printing(db, "lower", "a")
            populate_routes(db)
            populate_routes(db)
        assert len(db.rows("build_issue")) == 1
        issue_id = db.rows("build_issue")[0].values["id"]
        with pytest.raises(ValueError, match="Conflicting folded"), db.transaction():
            db.update("build_issue", {"id": issue_id}, {"severity": "error"})
            populate_routes(db)

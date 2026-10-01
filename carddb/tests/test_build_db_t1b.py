"""Independent counterexamples for minimum T1 and optional cross-region DDL."""

# ruff: file-ignore[pytest-raises-with-multiple-statements] -- related row changes are checked together at commit
import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Database, Json, create_database
from sve_carddb.build_db.t0 import TABLES as T0_TABLES
from sve_carddb.build_db.t1 import (
    MINIMUM_CAPABILITIES,
    REGISTRY,
    compile_build,
    compile_minimum,
)

from .build_db_t1b_fixtures import populate, rows

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.build_db import Value

    from .database_fixtures import DatabaseTemplate

MINIMUM_TABLES = {
    "image_asset",
    "printing_image",
    "image_variant",
    "image_size",
    "cr_version",
    "cr_clause",
    "errata",
    "errata_version",
    "errata_change",
    "errata_printing",
    "source_correction",
    "correction_evidence",
    "correction_application",
    "qa",
    "qa_version",
    "qa_card",
    "card_related",
}
EN_TABLES = {"art", "region_mapping_review", "region_text_review", "region_divergence"}


@pytest.fixture
def db(t1b_database_template: DatabaseTemplate) -> Iterator[Database]:
    with t1b_database_template.copy() as database:
        yield database


def key(name: str) -> dict[str, Value]:
    table = next(table for table in REGISTRY.tables if table.name == name)
    return {column: rows()[name][column] for column in table.primary_key}


@pytest.mark.parametrize("include_en", [False, True])
def test_exact_minimum_inventory_and_real_rows(include_en: bool) -> None:
    schema = compile_minimum(include_en=include_en)
    expected = (
        {table.name for table in T0_TABLES}
        | MINIMUM_TABLES
        | (EN_TABLES if include_en else set())
    )
    assert {table.name for table in schema.tables} == expected
    assert len(schema.tables) == (61 if include_en else 57)
    with create_database(schema) as database:
        with database.transaction():
            populate(database, include_en=include_en)
        assert all(database.rows(table.name) for table in schema.tables)
        assert database._read(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table'"
        ) == ((len(expected),),)
        database.verify()
    with pytest.raises(ValueError, match="Importer/validator unavailable"):
        REGISTRY.require_usable(MINIMUM_CAPABILITIES + (("en",) if include_en else ()))


@pytest.mark.parametrize(
    ("group", "extra"),
    [
        ("errata", {"errata", "errata_version", "errata_change", "errata_printing"}),
        (
            "correction",
            {"source_correction", "correction_evidence", "correction_application"},
        ),
        ("qa", {"qa", "qa_version", "qa_card"}),
        ("related", {"card_related"}),
        ("art", {"art"}),
        ("en", EN_TABLES),
    ],
)
def test_group_closure_is_explicit(group: str, extra: set[str]) -> None:
    assert {table.name for table in compile_build((group,)).tables} == {
        table.name for table in T0_TABLES
    } | extra
    schema = compile_build((group,))
    with create_database(schema) as database:
        database.verify()


@pytest.mark.parametrize("side", ["before", "after"])
@pytest.mark.parametrize("revision", ["revision2", "revision_en"])
def test_errata_revision_must_match_face_and_region(
    db: Database, side: str, revision: str
) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match=f"errata_{side}_scope"),
        db.transaction(),
    ):
        db.update(
            "errata_change", key("errata_change"), {side + "_revision_id": revision}
        )
    assert db.rows("errata_change")[0].values[side + "_revision_id"] == "revision"


def test_errata_printing_region_is_independent(db: Database) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match="errata_printing_region"),
        db.transaction(),
    ):
        db.update(
            "errata_printing", key("errata_printing"), {"printing_id": "printing_en"}
        )


@pytest.mark.parametrize("side", ["before", "after"])
def test_errata_parent_region_updates_revalidate_children(
    db: Database, side: str
) -> None:
    with db.transaction():
        db.delete("errata_printing", key("errata_printing"))
        db.update(
            "errata_change",
            key("errata_change"),
            {("after" if side == "before" else "before") + "_revision_id": None},
        )
    with (
        pytest.raises(sqlite3.IntegrityError, match=f"errata_{side}_scope"),
        db.transaction(),
    ):
        db.update("errata", key("errata"), {"region": "en"})
    assert db.rows("errata")[0].values["region"] == "jp"


@pytest.mark.parametrize(
    ("owner", "version", "parent_column", "owner_patch", "query"),
    [
        (
            "errata",
            "errata_version",
            "errata_id",
            {"official_url": "https://example.invalid/other"},
            "errata_supersedes_owner",
        ),
        (
            "qa",
            "qa_version",
            "qa_id",
            {"official_number": "Q2", "stable_source_key": "synthetic:other"},
            "qa_supersedes_owner",
        ),
    ],
)
def test_supersedes_stays_in_its_owner(
    db: Database,
    owner: str,
    version: str,
    parent_column: str,
    owner_patch: dict[str, Value],
    query: str,
) -> None:
    with db.transaction():
        db.insert(owner, rows()[owner] | owner_patch | {"id": "other_owner"})
        db.insert(
            version,
            rows()[version] | {"id": "other_version", parent_column: "other_owner"},
        )
    with pytest.raises(sqlite3.IntegrityError, match=query), db.transaction():
        db.update(version, key(version), {"supersedes_id": "other_version"})
    with db.transaction():
        db.insert(
            version,
            rows()[version]
            | {
                "id": "next_version",
                "revision": 1,
                "supersedes_id": rows()[version]["id"],
            },
        )


@pytest.mark.parametrize(
    ("table", "field", "value"),
    [
        ("errata_printing", "decision_id", None),
        ("region_text_review", "decision_id", None),
        ("region_divergence", "effect", "override_dsl"),
        ("card_related", "decision_id", None),
        ("card_related", "source_kind", "official"),
        ("card_related", "suggested_count", 1),
        ("card_related", "to_card_id", "card2"),
    ],
)
def test_local_adoption_and_relation_checks(
    db: Database, table: str, field: str, value: Value
) -> None:
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update(table, key(table), {field: value})


@pytest.mark.parametrize(
    ("decision", "query"),
    [
        ("errata_decision", "errata_printing_confirmed"),
        ("related_decision", "reskin_confirmed"),
        ("mapping_decision", "mapping_confirmed_none"),
        ("divergence_decision", "divergence_confirmed"),
    ],
)
@pytest.mark.parametrize(
    "state", ["sampled", "proposed", "model_reviewed", "rejected", "disputed"]
)
def test_confirmed_decisions_recheck_parent_updates(
    db: Database, decision: str, query: str, state: str
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match=query), db.transaction():
        db.update("decision", {"id": decision}, {"state": state})
    assert (
        next(row for row in db.rows("decision") if row.values["id"] == decision).values[
            "state"
        ]
        == "confirmed"
    )


@pytest.mark.parametrize(
    "state", ["proposed", "model_reviewed", "rejected", "disputed"]
)
def test_aligned_requires_adopted_review(db: Database, state: str) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match="text_review_adopted"),
        db.transaction(),
    ):
        db.update("decision", {"id": "text_decision"}, {"state": state})
    with db.transaction():
        db.update("decision", {"id": "text_decision"}, {"state": "sampled"})


@pytest.mark.parametrize("state", ["active", "upstream_fixed"])
def test_correction_adoption_without_application(db: Database, state: str) -> None:
    with db.transaction():
        db.delete("correction_application", key("correction_application"))
        db.update("source_correction", key("source_correction"), {"state": state})
    with (
        pytest.raises(sqlite3.IntegrityError, match="correction_adoption"),
        db.transaction(),
    ):
        db.update("decision", {"id": "correction_decision"}, {"state": "sampled"})


@pytest.mark.parametrize("status", ["applied", "already_fixed"])
def test_application_has_its_own_confirmed_gate(db: Database, status: str) -> None:
    with db.transaction():
        db.update(
            "source_correction", key("source_correction"), {"state": "needs_review"}
        )
        db.update(
            "correction_application", key("correction_application"), {"status": status}
        )
    with (
        pytest.raises(sqlite3.IntegrityError, match="application_adoption"),
        db.transaction(),
    ):
        db.update("decision", {"id": "correction_decision"}, {"state": "proposed"})


@pytest.mark.parametrize("with_decision", [False, True])
def test_pending_records_and_conflicts_do_not_require_adoption(
    db: Database, with_decision: bool
) -> None:
    with db.transaction():
        db.update("decision", {"id": "text_decision"}, {"state": "proposed"})
        db.update("decision", {"id": "errata_decision"}, {"state": "proposed"})
        db.update(
            "errata_printing",
            key("errata_printing"),
            {
                "scope": "listed",
                "decision_id": "errata_decision" if with_decision else None,
            },
        )
        db.update(
            "region_mapping_review", key("region_mapping_review"), {"state": "pending"}
        )
        db.update("decision", {"id": "mapping_decision"}, {"state": "proposed"})
        db.update(
            "region_text_review",
            key("region_text_review"),
            {
                "state": "pending",
                "decision_id": "text_decision" if with_decision else None,
            },
        )
        db.update(
            "source_correction", key("source_correction"), {"state": "needs_review"}
        )
        db.update("decision", {"id": "correction_decision"}, {"state": "proposed"})
        db.update(
            "correction_application",
            key("correction_application"),
            {"status": "conflict", "result_unit_id": None, "face_revision_id": None},
        )


@pytest.mark.parametrize("field", ["effect", "name", "flavor", "other"])
@pytest.mark.parametrize("status", ["applied", "already_fixed"])
def test_text_application_requires_result_unit(
    db: Database, field: str, status: str
) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match="application_text_result"),
        db.transaction(),
    ):
        db.update("source_correction", key("source_correction"), {"field": field})
        db.update(
            "correction_application",
            key("correction_application"),
            {"status": status, "result_unit_id": None},
        )


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("card_type", Json("Synthetic")),
        ("cost", Json(1)),
        ("attack", Json(1)),
        ("defense", Json(1)),
        ("traits", Json([])),
        ("titles", Json([])),
        ("special_kinds", Json([])),
    ],
)
@pytest.mark.parametrize("status", ["applied", "already_fixed"])
def test_rule_application_requires_revision(
    db: Database, field: str, value: Json, status: str
) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match="application_rule_result"),
        db.transaction(),
    ):
        db.update(
            "source_correction",
            key("source_correction"),
            {"field": field, "expected_raw_value": value, "corrected_value": value},
        )
        db.update(
            "correction_application",
            key("correction_application"),
            {"status": status, "face_revision_id": None},
        )


@pytest.mark.parametrize("revision", ["revision2", "revision_en"])
def test_application_revision_scope(db: Database, revision: str) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match="application_revision_scope"),
        db.transaction(),
    ):
        db.update(
            "correction_application",
            key("correction_application"),
            {"face_revision_id": revision},
        )


def test_correction_face_must_belong_to_printing(db: Database) -> None:
    with db.transaction():
        db.delete("correction_application", key("correction_application"))
    with pytest.raises(sqlite3.IntegrityError, match="Foreign key"), db.transaction():
        db.update("source_correction", key("source_correction"), {"face_id": "face2"})


def test_related_printing_belongs_to_target_card(db: Database) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="Foreign key"), db.transaction():
        db.update(
            "card_related", key("card_related"), {"target_printing_id": "printing2"}
        )
    with db.transaction():
        db.update(
            "card_related", key("card_related"), {"target_printing_id": "printing"}
        )


def test_suggested_count_positive_independent_of_reskin(db: Database) -> None:
    with db.transaction():
        db.update("card_related", key("card_related"), {"relation": "mentions"})
    with pytest.raises(sqlite3.IntegrityError), db.transaction():
        db.update("card_related", key("card_related"), {"suggested_count": 0})
    with db.transaction():
        db.update("card_related", key("card_related"), {"suggested_count": 1})


@pytest.mark.parametrize("target", ["card", "old_card"])
def test_reskin_single_target_and_duplicate_pair(db: Database, target: str) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"), db.transaction():
        db.insert(
            "card_related",
            rows()["card_related"] | {"id": "related2", "to_card_id": target},
        )


def test_reskin_no_reverse_row(db: Database) -> None:
    with (
        pytest.raises(sqlite3.IntegrityError, match="reskin_reverse"),
        db.transaction(),
    ):
        db.insert(
            "card_related",
            rows()["card_related"]
            | {"id": "reverse", "from_card_id": "card", "to_card_id": "card2"},
        )
    with db.transaction():
        db.insert(
            "card_related",
            rows()["card_related"]
            | {
                "id": "reverse",
                "from_card_id": "card",
                "to_card_id": "card2",
                "relation": "mentions",
            },
        )


def test_art_card_face_ownership(db: Database) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="Foreign key"), db.transaction():
        db.insert("art", rows()["art"] | {"id": "bad_art", "card_id": "card2"})


def test_printing_art_must_use_same_face(db: Database) -> None:
    with db.transaction():
        db.insert(
            "art",
            rows()["art"] | {"id": "art2", "card_id": "card2", "face_id": "face2"},
        )
    with pytest.raises(sqlite3.IntegrityError, match="Foreign key"), db.transaction():
        db.update(
            "printing_face",
            {"printing_id": "printing", "face_id": "face"},
            {"art_id": "art2"},
        )


@pytest.mark.parametrize(
    ("table", "patch"),
    [
        ("errata", {"id": "duplicate"}),
        ("errata_version", {"id": "duplicate"}),
        ("qa", {"id": "duplicate", "official_number": "Q2"}),
        ("qa", {"id": "duplicate", "stable_source_key": "other"}),
        ("qa_version", {"id": "duplicate"}),
    ],
)
def test_non_primary_unique_keys(
    db: Database, table: str, patch: dict[str, Value]
) -> None:
    with pytest.raises(sqlite3.IntegrityError, match="UNIQUE"), db.transaction():
        db.insert(table, rows()[table] | patch)


def test_unnumbered_qa_and_same_day_history(db: Database) -> None:
    with db.transaction():
        for number in (2, 3):
            db.insert(
                "qa",
                rows()["qa"]
                | {
                    "id": f"qa{number}",
                    "official_number": None,
                    "stable_source_key": f"key{number}",
                },
            )
        db.insert(
            "qa_version",
            rows()["qa_version"]
            | {"id": "qa_next", "revision": 1, "supersedes_id": "qa_v"},
        )
    assert len(db.rows("qa")) == 3
    assert len(db.rows("qa_version")) == 2


def test_other_relations_can_share_a_reskin_source_card(db: Database) -> None:
    with db.transaction():
        db.insert(
            "card_related",
            rows()["card_related"]
            | {
                "id": "mention",
                "relation": "mentions",
                "to_card_id": "old_card",
                "decision_id": None,
            },
        )


def test_non_reskin_relation_cannot_reference_itself(db: Database) -> None:
    with db.transaction():
        db.update("card_related", key("card_related"), {"relation": "mentions"})
    with (
        pytest.raises(sqlite3.IntegrityError, match="CHECK constraint"),
        db.transaction(),
    ):
        db.update("card_related", key("card_related"), {"to_card_id": "card2"})

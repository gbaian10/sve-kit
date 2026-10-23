"""Mechanism storage rejects invalid evidence and sparse public sets at the DB boundary."""

import sqlite3
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database
from sve_carddb.build.t1 import compile_build

from ..support.build_db_fixtures import HASH, seed
from ..support.database_template import DatabaseTemplate

if TYPE_CHECKING:
    from sve_carddb.build import Value


@pytest.fixture(scope="module")
def template() -> DatabaseTemplate:
    schema = compile_build(("keyword",))
    with create_database(schema) as db:
        seed(db)
        with db.transaction():
            for identifier, category in (
                ("a", "keyword"),
                ("b", "keyword"),
                ("rule", "rule_term"),
            ):
                db.insert(
                    "glossary_term",
                    {
                        "id": "term:" + identifier,
                        "category": category,
                        "source_ja": "Synthetic " + identifier,
                        "concept_key": identifier,
                        "emphasis": True,
                        "authored_source_id": "source",
                        "record_key": identifier,
                        "origin": "project",
                        "low_confidence": False,
                    },
                )
            db.insert(
                "keyword",
                {
                    "id": "kw:a",
                    "code": "a",
                    "term_id": "term:a",
                    "kind": "printed_keyword",
                    "decision_id": "decision",
                },
            )
            db.insert(
                "mechanic_action",
                {
                    "keyword_id": "kw:a",
                    "action": "strike",
                    "label_unit_id": "text",
                    "decision_id": "decision",
                },
            )
            db.insert("card_mechanic", evidence() | {"id": "tag:seed"})
        return DatabaseTemplate(schema, db._connection.serialize())


def evidence() -> dict[str, Value]:
    return {
        "id": "tag:new",
        "card_id": "card",
        "keyword_id": "kw:a",
        "relation": "has",
        "action": None,
        "scope": "shared",
        "source_kind": "manual",
        "source_text_hash": HASH,
        "producer_version": "synthetic-v1",
        "dsl_id": None,
        "source_id": "source",
        "decision_id": "decision",
    }


def row(table: str) -> dict[str, Value]:
    rows: dict[str, dict[str, Value]] = {
        "keyword": {
            "id": "kw:b",
            "code": "b",
            "term_id": "term:b",
            "kind": "printed_keyword",
            "definition_unit_id": None,
            "decision_id": "decision",
        },
        "mechanic_action": {
            "keyword_id": "kw:a",
            "action": "other",
            "label_unit_id": "text",
            "decision_id": "decision",
        },
        "card_mechanic": evidence(),
        "mechanic_coverage": {
            "card_id": "card",
            "keyword_id": "kw:a",
            "scope": "shared",
            "state": "unreviewed",
            "checked_ranges": Json([]),
            "source_kind": "manual",
            "source_text_hash": HASH,
            "producer_version": "synthetic-v1",
            "decision_id": "decision",
            "dsl_id": None,
        },
        "mechanic_override": {
            "id": "override:new",
            "target_tag_id": "tag:seed",
            "operation": "suppress",
            "source_text_hash": HASH,
            "producer_version": "synthetic-v1",
            "reason": "Synthetic correction.",
            "decision_id": "decision",
        },
        "mechanic_projection": {
            "card_id": "card",
            "keyword_id": "kw:a",
            "scope": "shared",
            "relations": Json(["has"]),
            "actions": Json([]),
        },
        "card_mechanic_coverage": {
            "card_id": "card",
            "scope": "shared",
            "complete_all": False,
            "complete_mode": "include",
            "complete_keyword_ids": Json([]),
            "partial_mode": "include",
            "partial_keyword_ids": Json(["kw:a"]),
        },
    }
    return rows[table]


@pytest.mark.parametrize(
    "table",
    [
        "keyword",
        "mechanic_action",
        "card_mechanic",
        "mechanic_coverage",
        "mechanic_override",
        "mechanic_projection",
        "card_mechanic_coverage",
    ],
)
def test_connected_manual_rows_are_valid(
    template: DatabaseTemplate, table: str
) -> None:
    with template.copy() as db, db.transaction():
        db.insert(table, row(table))


@pytest.mark.parametrize(
    ("table", "changes", "check"),
    [
        ("keyword", {"term_id": "term:rule"}, "printed_keyword_concept"),
        ("keyword", {"code": "a"}, "UNIQUE"),
        ("keyword", {"term_id": "term:a"}, "UNIQUE"),
        ("card_mechanic", {"source_kind": "dsl"}, "CHECK"),
        ("mechanic_coverage", {"source_kind": "dsl"}, "CHECK"),
        ("mechanic_projection", {"relations": Json([])}, "CHECK"),
        (
            "mechanic_projection",
            {"actions": Json(["missing"])},
            "mechanic_projection_actions",
        ),
        (
            "card_mechanic_coverage",
            {"complete_keyword_ids": Json(["missing"])},
            "complete_keyword_ids_references",
        ),
        (
            "card_mechanic_coverage",
            {"partial_keyword_ids": Json(["missing"])},
            "partial_keyword_ids_references",
        ),
        ("card_mechanic_coverage", {"complete_all": True}, "CHECK"),
        (
            "card_mechanic_coverage",
            {
                "complete_all": True,
                "partial_keyword_ids": Json([]),
                "complete_mode": "exclude",
            },
            "CHECK",
        ),
        (
            "card_mechanic_coverage",
            {
                "complete_all": True,
                "partial_keyword_ids": Json([]),
                "partial_mode": "exclude",
            },
            "CHECK",
        ),
        (
            "card_mechanic_coverage",
            {
                "complete_all": True,
                "partial_keyword_ids": Json([]),
                "complete_keyword_ids": Json(["kw:a"]),
            },
            "CHECK",
        ),
    ],
)
def test_invalid_mechanism_rows_fail_the_declared_constraint(
    template: DatabaseTemplate, table: str, changes: dict[str, Value], check: str
) -> None:
    with (
        template.copy() as db,
        pytest.raises(sqlite3.IntegrityError, match=check),
        db.transaction(),
    ):
        db.insert(table, row(table) | changes)


@pytest.mark.parametrize(
    ("table", "column"),
    [
        ("keyword", "term_id"),
        ("keyword", "definition_unit_id"),
        ("keyword", "decision_id"),
        ("mechanic_action", "keyword_id"),
        ("mechanic_action", "label_unit_id"),
        ("mechanic_action", "decision_id"),
        ("card_mechanic", "card_id"),
        ("card_mechanic", "keyword_id"),
        ("card_mechanic", "action"),
        ("card_mechanic", "source_id"),
        ("card_mechanic", "decision_id"),
        ("mechanic_coverage", "card_id"),
        ("mechanic_coverage", "keyword_id"),
        ("mechanic_coverage", "decision_id"),
        ("mechanic_override", "target_tag_id"),
        ("mechanic_override", "decision_id"),
        ("mechanic_projection", "card_id"),
        ("mechanic_projection", "keyword_id"),
        ("card_mechanic_coverage", "card_id"),
    ],
)
def test_mechanism_references_must_exist(
    template: DatabaseTemplate, table: str, column: str
) -> None:
    with (
        template.copy() as db,
        pytest.raises(sqlite3.IntegrityError, match=r"(?i)foreign key"),
        db.transaction(),
    ):
        db.insert(table, row(table) | {column: "missing"})


@pytest.mark.parametrize("table", ["card_mechanic", "mechanic_projection"])
def test_action_from_another_keyword_is_not_a_valid_reference(
    template: DatabaseTemplate, table: str
) -> None:
    with template.copy() as db:
        with db.transaction():
            db.insert("keyword", row("keyword"))
        action = (
            {"action": "strike"}
            if table == "card_mechanic"
            else {"actions": Json(["strike"])}
        )
        with pytest.raises(sqlite3.IntegrityError), db.transaction():
            db.insert(table, row(table) | {"keyword_id": "kw:b"} | action)


def test_complete_all_storage_accepts_empty_included_sets(
    template: DatabaseTemplate,
) -> None:
    with template.copy() as db, db.transaction():
        db.insert(
            "card_mechanic_coverage",
            row("card_mechanic_coverage")
            | {"complete_all": True, "partial_keyword_ids": Json([])},
        )


@pytest.mark.parametrize(
    ("column", "raw"),
    [
        ("relations", '["absent"]'),
        ("relations", '["has","has"]'),
        ("actions", '["bad code"]'),
        ("actions", '["strike","strike"]'),
        ("complete_keyword_ids", '["kw:a","kw:a"]'),
        ("partial_keyword_ids", '["kw:a","kw:a"]'),
        ("partial_keyword_ids", "[null]"),
    ],
)
def test_json_schema_constraints_are_enforced_below_the_typed_writer(
    template: DatabaseTemplate, column: str, raw: str
) -> None:
    with template.copy() as db:

        def invalid_insert() -> None:
            # Bypass Python's JSON validation to exercise the SQLite CHECK itself.
            with db.transaction():
                if column in {"relations", "actions"}:
                    db._connection.execute(
                        "INSERT INTO mechanic_projection (card_id,keyword_id,scope,relations,actions) VALUES (?,?,?,?,?)",
                        (
                            "card",
                            "kw:a",
                            "shared",
                            raw if column == "relations" else '["has"]',
                            raw if column == "actions" else "[]",
                        ),
                    )
                else:
                    db._connection.execute(
                        "INSERT INTO card_mechanic_coverage (card_id,scope,complete_all,complete_mode,complete_keyword_ids,partial_mode,partial_keyword_ids) VALUES (?,?,?,?,?,?,?)",
                        (
                            "card",
                            "shared",
                            0,
                            "include",
                            raw if column == "complete_keyword_ids" else "[]",
                            "include",
                            raw if column == "partial_keyword_ids" else "[]",
                        ),
                    )

        with pytest.raises(sqlite3.IntegrityError, match="CHECK"):
            invalid_insert()

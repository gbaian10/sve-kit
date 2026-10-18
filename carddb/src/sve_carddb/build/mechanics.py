"""Manual mechanism storage uses the existing sparse public projection contract."""

from pydantic import JsonValue

from sve_carddb.build.model import (
    Check,
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)
from sve_carddb.build.scalars import CODE, HASH

SCOPE = Column("scope", Kind.TEXT, choices=("shared", "en_override"))
FRESHNESS = (
    Column("source_kind", Kind.TEXT, choices=("dsl", "manual", "community")),
    Column("source_text_hash", Kind.TEXT, pattern=HASH),
    Column("producer_version", Kind.TEXT),
)


def _fk(column: str, table: str) -> ForeignKey:
    return ForeignKey((column,), table, ("id",))


def _json(column: str, schema: str) -> Column:
    return Column(column, Kind.JSON, json_schema=schema)


def _keyword_references(table: str, column: str) -> QueryCheck:
    return QueryCheck(
        table + "_" + column + "_references",
        f"SELECT 1 FROM {table} AS r, json_each(r.{column}) AS v "  # ruff: ignore[hardcoded-sql-expression] -- both identifiers are fixed code-authored declarations
        "LEFT JOIN keyword AS k ON k.id=v.value WHERE k.id IS NULL LIMIT 1",
        (table, "keyword"),
    )


TABLES = (
    Table(
        "keyword",
        (
            Column("id", Kind.ID),
            Column("code", Kind.ID, pattern=CODE),
            Column("term_id", Kind.ID),
            Column(
                "kind", Kind.TEXT, choices=("printed_keyword", "mechanic", "resource")
            ),
            Column("definition_unit_id", Kind.ID, nullable=True),
            Column("decision_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            _fk("term_id", "glossary_term"),
            _fk("definition_unit_id", "text_unit"),
            _fk("decision_id", "decision"),
        ),
        unique=(Unique(("code",)), Unique(("term_id",))),
        query_checks=(
            QueryCheck(
                "printed_keyword_concept",
                "SELECT 1 FROM keyword AS k JOIN glossary_term AS t ON t.id=k.term_id "
                "WHERE k.kind='printed_keyword' AND t.category!='keyword' LIMIT 1",
                ("keyword", "glossary_term"),
            ),
        ),
    ),
    Table(
        "mechanic_action",
        (
            Column("keyword_id", Kind.ID),
            Column("action", Kind.ID, pattern=CODE),
            Column("label_unit_id", Kind.ID),
            Column("decision_id", Kind.ID),
        ),
        ("keyword_id", "action"),
        foreign_keys=(
            _fk("keyword_id", "keyword"),
            _fk("label_unit_id", "text_unit"),
            _fk("decision_id", "decision"),
        ),
    ),
    Table(
        "card_mechanic",
        (
            Column("id", Kind.ID),
            Column("card_id", Kind.ID),
            Column("keyword_id", Kind.ID),
            Column(
                "relation", Kind.TEXT, choices=("has", "grants", "refers", "counts")
            ),
            Column("action", Kind.ID, nullable=True, pattern=CODE),
            SCOPE,
            *FRESHNESS,
            Column("dsl_id", Kind.ID, nullable=True),
            Column("source_id", Kind.ID, nullable=True),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            _fk("card_id", "card"),
            _fk("keyword_id", "keyword"),
            ForeignKey(
                ("keyword_id", "action"), "mechanic_action", ("keyword_id", "action")
            ),
            _fk("dsl_id", "dsl_document"),
            _fk("source_id", "source_record"),
            _fk("decision_id", "decision"),
        ),
        checks=(Check("source_kind!='dsl' OR dsl_id IS NOT NULL"),),
    ),
    Table(
        "mechanic_coverage",
        (
            Column("card_id", Kind.ID),
            Column("keyword_id", Kind.ID),
            SCOPE,
            Column("state", Kind.TEXT, choices=("unreviewed", "partial", "complete")),
            *FRESHNESS,
            Column("decision_id", Kind.ID, nullable=True),
            Column("dsl_id", Kind.ID, nullable=True),
        ),
        ("card_id", "keyword_id", "scope"),
        foreign_keys=(
            _fk("card_id", "card"),
            _fk("keyword_id", "keyword"),
            _fk("decision_id", "decision"),
            _fk("dsl_id", "dsl_document"),
        ),
        checks=(Check("source_kind!='dsl' OR dsl_id IS NOT NULL"),),
    ),
    Table(
        "mechanic_override",
        (
            Column("id", Kind.ID),
            Column("target_tag_id", Kind.ID),
            Column("operation", Kind.TEXT, choices=("suppress",)),
            Column("source_text_hash", Kind.TEXT, pattern=HASH),
            Column("producer_version", Kind.TEXT),
            Column("reason", Kind.TEXT),
            Column("decision_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            _fk("target_tag_id", "card_mechanic"),
            _fk("decision_id", "decision"),
        ),
    ),
    Table(
        "mechanic_projection",
        (
            Column("card_id", Kind.ID),
            Column("keyword_id", Kind.ID),
            SCOPE,
            _json("relations", "MechanicRelations"),
            _json("actions", "MechanicCodes"),
        ),
        ("card_id", "keyword_id", "scope"),
        foreign_keys=(_fk("card_id", "card"), _fk("keyword_id", "keyword")),
        checks=(Check("json_array_length(relations)+json_array_length(actions)>0"),),
        query_checks=(
            QueryCheck(
                "mechanic_projection_actions",
                "SELECT 1 FROM mechanic_projection AS p, json_each(p.actions) AS v "
                "LEFT JOIN mechanic_action AS a ON a.keyword_id=p.keyword_id AND a.action=v.value "
                "WHERE a.keyword_id IS NULL LIMIT 1",
                ("mechanic_projection", "mechanic_action"),
            ),
        ),
    ),
    Table(
        "card_mechanic_coverage",
        (
            Column("card_id", Kind.ID),
            SCOPE,
            Column("complete_all", Kind.BOOL),
            Column("complete_mode", Kind.TEXT, choices=("include", "exclude")),
            _json("complete_keyword_ids", "MechanicIDs"),
            Column("partial_mode", Kind.TEXT, choices=("include", "exclude")),
            _json("partial_keyword_ids", "MechanicIDs"),
        ),
        ("card_id", "scope"),
        foreign_keys=(_fk("card_id", "card"),),
        checks=(
            Check(
                "NOT complete_all OR (complete_mode='include' AND partial_mode='include' "
                "AND json_array_length(complete_keyword_ids)=0 AND json_array_length(partial_keyword_ids)=0)"
            ),
        ),
        query_checks=tuple(
            _keyword_references("card_mechanic_coverage", column)
            for column in ("complete_keyword_ids", "partial_keyword_ids")
        ),
    ),
)


def schemas() -> dict[str, dict[str, JsonValue]]:
    """Validate every array member before relational reference checks run."""
    items: dict[str, JsonValue] = {
        "MechanicRelations": {
            "type": "string",
            "enum": ["has", "grants", "refers", "counts"],
        },
        "MechanicCodes": {"type": "string", "pattern": "^" + CODE + "$"},
        "MechanicIDs": {"type": "string", "minLength": 1},
    }
    return {
        name: {"type": "array", "items": item, "uniqueItems": True}
        for name, item in items.items()
    }

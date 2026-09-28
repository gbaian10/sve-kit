"""T0 declarations from docs/schema/build-db.md; domain validation is separate."""

from sve_carddb.build_db.domains import LANG
from sve_carddb.build_db.model import Check, Column, ForeignKey, Kind, Table, Unique

TABLES = (
    Table(
        "build_issue",
        (
            Column("id", Kind.ID),
            Column("category", Kind.ID),
            Column("severity", Kind.TEXT, choices=("error", "warning")),
            Column("entity_type", Kind.TEXT),
            Column("entity_id", Kind.TEXT),
            Column("message", Kind.TEXT),
            Column("source_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("source_id",), "source_record", ("id",)),),
    ),
    Table(
        "text_symbol",
        (
            Column("id", Kind.ID),
            Column("code", Kind.ID),
            Column(
                "parameter_schema",
                Kind.JSON,
                json_schema="text_symbol_parameter_schema",
            ),
            Column("keyword_id", Kind.ID, nullable=True),
            Column("spellings", Kind.JSON, json_schema="text_symbol_spellings"),
            Column("localizations", Kind.JSON, json_schema="text_symbol_localizations"),
            Column("decision_id", Kind.ID),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("keyword_id",), "keyword", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        unique=(Unique(("code",)),),
        checks=(Check("sve_symbol_valid(parameter_schema, spellings) = 1"),),
    ),
    Table(
        "card_route",
        (
            Column("namespace", Kind.TEXT, choices=("official", "provisional")),
            Column("route_key", Kind.TEXT),
            Column("printing_id", Kind.ID),
        ),
        ("namespace", "route_key"),
        foreign_keys=(ForeignKey(("printing_id",), "printing", ("id",)),),
        checks=(
            Check(
                "namespace != 'provisional' OR sve_fullmatch('[1-9][0-9]*', route_key) = 1"
            ),
        ),
    ),
    Table(
        "card_route_alias",
        (
            Column("namespace", Kind.TEXT, choices=("official", "provisional")),
            Column("old_key", Kind.TEXT),
            Column("target_namespace", Kind.TEXT, choices=("official", "provisional")),
            Column("target_key", Kind.TEXT),
            Column(
                "reason",
                Kind.TEXT,
                choices=("renumbered", "merged", "provisional_corrected"),
            ),
            Column("source_id", Kind.ID),
            Column("decision_id", Kind.ID),
        ),
        ("namespace", "old_key"),
        foreign_keys=(
            ForeignKey(
                ("target_namespace", "target_key"),
                "card_route",
                ("namespace", "route_key"),
            ),
            ForeignKey(("source_id",), "source_record", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
        checks=(
            Check("namespace != target_namespace OR old_key != target_key"),
            Check(
                "namespace != 'provisional' OR sve_fullmatch('[1-9][0-9]*', old_key) = 1"
            ),
        ),
    ),
    Table(
        "route_override",
        (
            Column("route_key", Kind.TEXT),
            Column("printing_id", Kind.ID),
            Column("decision_id", Kind.ID),
            Column("namespace", Kind.TEXT, fixed="official"),
        ),
        ("route_key",),
        foreign_keys=(
            ForeignKey(
                ("namespace", "route_key"), "card_route", ("namespace", "route_key")
            ),
            ForeignKey(("printing_id",), "printing", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
    ),
    Table(
        "default_printing_override",
        (
            Column("card_id", Kind.ID),
            Column("region", Kind.TEXT, choices=("jp", "en")),
            Column("printing_id", Kind.ID),
            Column("decision_id", Kind.ID),
        ),
        ("card_id", "region"),
        foreign_keys=(
            ForeignKey(
                ("printing_id", "card_id", "region"),
                "printing",
                ("id", "card_id", "region"),
            ),
            ForeignKey(("card_id",), "card", ("id",)),
            ForeignKey(("printing_id",), "printing", ("id",)),
            ForeignKey(("decision_id",), "decision", ("id",)),
        ),
    ),
    Table(
        "search_alias",
        (
            Column("kind", Kind.ID),
            Column("code", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("text", Kind.TEXT),
            Column("normalized", Kind.TEXT),
            Column("normalizer_version", Kind.TEXT),
            Column("decision_id", Kind.ID, nullable=True),
        ),
        ("kind", "code", "lang", "text"),
        foreign_keys=(
            ForeignKey(("decision_id",), "decision", ("id",)),
            ForeignKey(("lang",), "language", ("code",)),
        ),
    ),
)

"""Format-two translation tables during the explicit legacy migration boundary."""

from dataclasses import replace

from sve_carddb.build_db import t1, t1_json
from sve_carddb.build_db.compiler import CompiledSchema, compile_schema
from sve_carddb.build_db.domains import HASH, LANG
from sve_carddb.build_db.model import Check, Column, ForeignKey, Kind, Table, Unique
from sve_carddb.build_db.registry import Registry
from sve_carddb.build_db.t0_json import schemas

QUALITY = (
    Column("authored_source_id", Kind.ID),
    Column("record_key", Kind.TEXT),
    Column("origin", Kind.TEXT, choices=("official", "project", "machine")),
    Column("low_confidence", Kind.BOOL),
)
SOURCE = ForeignKey(("authored_source_id",), "source_record", ("id",))
TABLES = (
    Table(
        "glossary_term",
        (
            Column("id", Kind.ID),
            Column(
                "category",
                Kind.TEXT,
                choices=("keyword", "ability", "trait", "rule_term", "card_name"),
            ),
            Column("source_ja", Kind.TEXT),
            Column("concept_key", Kind.TEXT, pattern=r"[a-z][a-z0-9_.-]*"),
            Column("emphasis", Kind.BOOL, nullable=True),
            *QUALITY,
        ),
        ("id",),
        foreign_keys=(SOURCE,),
        unique=(Unique(("concept_key",)),),
        checks=(Check("id = 'term:' || concept_key"),),
    ),
    Table(
        "glossary_translation",
        (
            Column("term_id", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("text", Kind.TEXT),
            Column("source_id", Kind.ID, nullable=True),
            *QUALITY,
        ),
        ("term_id", "lang"),
        foreign_keys=(
            ForeignKey(("term_id",), "glossary_term", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
            SOURCE,
        ),
    ),
)


def compile_current_build(requested: tuple[str, ...] = ("t0",)) -> CompiledSchema:
    """Use one schema per build; legacy consumers keep their format-one schema."""
    replacements = {table.name: table for table in TABLES}
    replacements["translation_context"] = Table(
        "translation_context",
        (
            Column("id", Kind.ID),
            Column("source_unit_id", Kind.ID),
            Column("semantic_variant", Kind.ID),
        ),
        ("id",),
        foreign_keys=(ForeignKey(("source_unit_id",), "text_unit", ("id",)),),
        unique=(Unique(("source_unit_id", "semantic_variant")),),
    )
    replacements["translation"] = Table(
        "translation",
        (
            Column("id", Kind.ID),
            Column("context_id", Kind.ID),
            Column("target_lang", Kind.ID, pattern=LANG),
            Column("revision", Kind.UINT),
            Column("text", Kind.TEXT),
            Column("tokens", Kind.JSON, nullable=True, json_schema="TranslationTokens"),
            Column("origin", Kind.TEXT, choices=("official", "project", "machine")),
            Column(
                "authority",
                Kind.TEXT,
                choices=("sve_official", "digital_official", "unofficial"),
            ),
            Column("low_confidence", Kind.BOOL),
            Column("source_hash", Kind.TEXT, pattern=HASH),
            Column("source_id", Kind.ID, nullable=True),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("context_id",), "translation_context", ("id",)),
            ForeignKey(("source_id",), "source_record", ("id",)),
        ),
        unique=(Unique(("context_id", "target_lang", "revision")),),
    )
    for table in t1.REGISTRY.tables:
        if table.name == "translation_use":
            replacements[table.name] = replace(
                table,
                query_checks=tuple(
                    check
                    for check in table.query_checks
                    if check.name != "name_use_adopted_variant"
                ),
            )
        elif table.name == "translation_selection":
            replacements[table.name] = replace(
                table,
                query_checks=tuple(
                    replace(
                        check, sql=check.sql.replace(" OR t.status != 'reviewed'", "")
                    )
                    if check.name == "translation_selection_exact"
                    else check
                    for check in table.query_checks
                ),
            )
    for table in t1.REGISTRY.tables:
        if table.name in {"language", "vocabulary"}:
            replacements[table.name] = replace(
                table,
                columns=(
                    *table.columns,
                    *(replace(column, nullable=True) for column in QUALITY),
                ),
                foreign_keys=(*table.foreign_keys, SOURCE),
            )
    registry = Registry(
        tables=tuple(
            replacements.get(table.name, table) for table in t1.REGISTRY.tables
        ),
        capabilities=t1.REGISTRY.capabilities,
    )
    return compile_schema(
        registry,
        requested,
        schemas()
        | t1_json.schemas()
        | {
            "TranslationTokens": {"type": "null"},
            "semantic_sections": {
                "type": "array",
                "items": {"type": "string", "minLength": 1},
            },
        },
        version=t1.SCHEMA_VERSION + 1,
    )

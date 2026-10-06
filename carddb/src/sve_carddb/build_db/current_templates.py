"""Current template projection; semantic identity is independent of approval history."""

from pydantic import JsonValue, TypeAdapter

from sve_carddb.build_db.domains import HASH, LANG
from sve_carddb.build_db.model import (
    Column,
    ForeignKey,
    Kind,
    QueryCheck,
    Table,
    Unique,
)
from sve_carddb.snapshot.values import canonical, parse
from sve_carddb.template_parameters.models import Schema, SourceSpan

QUALITY = (
    Column("authored_source_id", Kind.ID),
    Column("record_key", Kind.TEXT),
    Column("origin", Kind.TEXT, choices=("official", "project", "machine")),
    Column("low_confidence", Kind.BOOL),
)
SOURCE = ForeignKey(("authored_source_id",), "source_record", ("id",))
TABLES = (
    Table(
        "translation_term",
        (Column("translation_id", Kind.ID), Column("term_id", Kind.ID)),
        ("translation_id", "term_id"),
        foreign_keys=(
            ForeignKey(("translation_id",), "translation", ("id",)),
            ForeignKey(("term_id",), "glossary_term", ("id",)),
        ),
    ),
    Table(
        "sentence_template",
        (
            Column("id", Kind.ID),
            Column("level", Kind.TEXT, choices=("sentence", "clause")),
            Column("source_lang", Kind.ID, pattern=LANG),
            Column("normalized_text", Kind.TEXT),
            Column("normalizer_version", Kind.TEXT),
            Column("semantic_variant", Kind.ID),
            Column("parameter_schema", Kind.JSON, json_schema="TemplateParameters"),
            Column("content_hash", Kind.TEXT, pattern=HASH),
            *QUALITY,
        ),
        ("id",),
        foreign_keys=(SOURCE,),
        unique=(Unique(("content_hash",)),),
    ),
    Table(
        "template_translation",
        (
            Column("template_id", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("variant_key", Kind.ID),
            Column("text", Kind.TEXT),
            *QUALITY,
        ),
        ("template_id", "lang", "variant_key"),
        foreign_keys=(
            ForeignKey(("template_id",), "sentence_template", ("id",)),
            SOURCE,
        ),
    ),
    Table(
        "text_template_binding",
        (
            Column("id", Kind.ID),
            Column("context_id", Kind.ID),
            Column("ordinal", Kind.UINT),
            Column("template_id", Kind.ID),
            Column("params", Kind.JSON, json_schema="TemplateValues"),
            Column("source_span", Kind.JSON, json_schema="TemplateSpan"),
        ),
        ("id",),
        foreign_keys=(
            ForeignKey(("context_id",), "translation_context", ("id",)),
            ForeignKey(("template_id",), "sentence_template", ("id",)),
        ),
        unique=(Unique(("context_id", "ordinal")),),
    ),
    Table(
        "translation_binding",
        (
            Column("translation_id", Kind.ID),
            Column("binding_id", Kind.ID),
            Column("template_id", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("variant_key", Kind.ID),
        ),
        ("translation_id", "binding_id"),
        foreign_keys=(
            ForeignKey(("translation_id",), "translation", ("id",)),
            ForeignKey(("binding_id",), "text_template_binding", ("id",)),
            ForeignKey(
                ("template_id", "lang", "variant_key"),
                "template_translation",
                ("template_id", "lang", "variant_key"),
            ),
        ),
        query_checks=(
            QueryCheck(
                "current_template_binding_exact",
                "SELECT 1 FROM translation_binding AS x "
                "JOIN translation AS t ON t.id=x.translation_id "
                "JOIN text_template_binding AS b ON b.id=x.binding_id "
                "WHERE x.template_id!=b.template_id OR x.lang!=t.target_lang "
                "OR b.context_id!=t.context_id LIMIT 1",
                ("translation_binding", "translation", "text_template_binding"),
            ),
        ),
    ),
)


def schemas() -> dict[str, JsonValue]:
    """The source and finite parameter shapes use the same reader models."""
    return {
        "TemplateParameters": parse(canonical(Schema.model_json_schema())),
        "TemplateValues": {"type": "object"},
        "TemplateSpan": parse(canonical(TypeAdapter(SourceSpan).json_schema())),
    }

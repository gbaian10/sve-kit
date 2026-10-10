"""Four-layer relational declarations and named JSON contracts for build boundaries."""

from typing import Annotated

from pydantic import ConfigDict, Field, JsonValue, TypeAdapter

from sve_carddb.build.model import Column, ForeignKey, Kind, QueryCheck, Table, Unique
from sve_carddb.build.scalars import LANG
from sve_carddb.contracts import four_layer
from sve_carddb.contracts.annotations import Annotation, RenderLeafOccurrence
from sve_carddb.contracts.four_layer import (
    Code,
    FormPart,
    LeafSchema,
    Projection,
    SemanticVariant,
    Signature,
    Span,
    Target,
)
from sve_carddb.contracts.source_binding import (
    SourceDescriptor,
    SourceSpan,
    TracePiece,
    TypedValue,
)
from sve_carddb.core.json import canonical, parse
from sve_carddb.core.models import RecordData, UInt

HASH = r"[0-9a-f]{64}"
CODE = r"[a-z][a-z0-9_.-]*"
QUALITY = (
    Column("authored_source_id", Kind.ID),
    Column("record_key", Kind.TEXT),
    Column("origin", Kind.TEXT, choices=("official", "project", "machine")),
    Column("low_confidence", Kind.BOOL),
)
SOURCE = ForeignKey(("authored_source_id",), "source_record", ("id",))


def _json(name: str, schema: str) -> Column:
    return Column(name, Kind.JSON, json_schema=schema)


def _fk(column: str, table: str) -> ForeignKey:
    return ForeignKey((column,), table, ("id",))


TABLES = (
    Table(
        "sentence_template",
        (
            Column("id", Kind.ID, pattern="frame:" + HASH),
            _json("source", "FrameSource"),
            Column(
                "role",
                Kind.TEXT,
                choices=("body", "reminder", "token_header", "layout", "name", "label"),
            ),
            _json("semantic_variant", "FrameSemanticVariant"),
            _json("leaf_schema", "FrameLeafSchema"),
            _json("projection", "FrameProjection"),
            Column("canonical_source", Kind.TEXT),
            Column("content_hash", Kind.TEXT, pattern=HASH),
            Column("interface_key", Kind.TEXT, pattern=HASH),
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
            Column("variant_key", Kind.ID, pattern=CODE),
            _json("target", "FrameTarget"),
            *QUALITY,
        ),
        ("template_id", "lang", "variant_key"),
        foreign_keys=(_fk("template_id", "sentence_template"), SOURCE),
    ),
    Table(
        "translation_form",
        (
            Column("id", Kind.ID, pattern=CODE),
            Column("lang", Kind.ID, pattern=LANG),
            Column("rule", Kind.TEXT, pattern=CODE),
            _json("signature", "FormSignature"),
            _json("cases", "FormCases"),
            *QUALITY,
        ),
        ("id", "lang"),
        foreign_keys=(SOURCE,),
    ),
    Table(
        "text_template_binding",
        (
            Column("id", Kind.ID, pattern="bind:" + HASH),
            Column("use_id", Kind.ID),
            Column("ordinal", Kind.UINT),
            Column("line_ordinal", Kind.UINT),
            Column("frame_id", Kind.ID),
            _json("source", "BindingSource"),
            _json("source_span", "BindingSpan"),
            _json("values", "BindingValues"),
            _json("trace", "BindingTrace"),
        ),
        ("id",),
        foreign_keys=(
            _fk("use_id", "translation_use"),
            _fk("frame_id", "sentence_template"),
        ),
        unique=(Unique(("use_id", "ordinal")),),
    ),
    Table(
        "binding_leaf_occurrence",
        (
            Column("binding_id", Kind.ID),
            Column("position", Kind.UINT),
            Column("slot", Kind.TEXT, pattern=CODE),
            Column("ordinal", Kind.UINT),
            _json("raw_spans", "LeafSpans"),
            _json("canonical_spans", "LeafSpans"),
            Column("source_unit", Kind.TEXT, nullable=True),
            Column("source_presence", Kind.TEXT, choices=("explicit", "omitted")),
            Column("resolution_rule", Kind.TEXT, nullable=True, pattern=CODE),
        ),
        ("binding_id", "slot", "ordinal"),
        foreign_keys=(_fk("binding_id", "text_template_binding"),),
        unique=(Unique(("binding_id", "position")),),
    ),
    Table(
        "render_leaf_occurrence",
        (
            Column("translation_id", Kind.ID),
            Column("binding_id", Kind.ID),
            _json("node_path", "RenderNodePath"),
            Column("slot", Kind.TEXT, pattern=CODE),
            _json("source_ordinals", "RenderSourceOrdinals"),
            _json("ranges", "RenderRanges"),
        ),
        ("translation_id", "binding_id", "node_path"),
        foreign_keys=(
            _fk("translation_id", "translation"),
            _fk("binding_id", "text_template_binding"),
        ),
    ),
    Table(
        "translation_binding",
        (
            Column("translation_id", Kind.ID),
            Column("binding_id", Kind.ID),
            Column("template_id", Kind.ID),
            Column("lang", Kind.ID, pattern=LANG),
            Column("variant_key", Kind.ID, pattern=CODE),
        ),
        ("translation_id", "binding_id"),
        foreign_keys=(
            _fk("translation_id", "translation"),
            _fk("binding_id", "text_template_binding"),
            ForeignKey(
                ("template_id", "lang", "variant_key"),
                "template_translation",
                ("template_id", "lang", "variant_key"),
            ),
        ),
        query_checks=(
            QueryCheck(
                "four_layer_translation_binding_exact",
                "SELECT 1 FROM translation_binding AS x "
                "JOIN translation AS t ON t.id=x.translation_id "
                "JOIN text_template_binding AS b ON b.id=x.binding_id "
                "JOIN translation_use AS u ON u.id=b.use_id "
                "WHERE x.template_id!=b.frame_id OR x.lang!=t.target_lang "
                "OR u.context_id!=t.context_id LIMIT 1",
                (
                    "translation_binding",
                    "translation",
                    "text_template_binding",
                    "translation_use",
                ),
            ),
        ),
    ),
    Table(
        "translation_term",
        (Column("translation_id", Kind.ID), Column("term_id", Kind.ID)),
        ("translation_id", "term_id"),
        foreign_keys=(
            _fk("translation_id", "translation"),
            _fk("term_id", "glossary_term"),
        ),
    ),
    Table(
        "annotation_set",
        (
            Column("id", Kind.ID, pattern="ann:" + HASH),
            Column("text_unit_id", Kind.ID),
            _json("occurrences", "AnnotationOccurrences"),
        ),
        ("id",),
        foreign_keys=(_fk("text_unit_id", "text_unit"),),
    ),
    Table(
        "translation_annotation",
        (Column("translation_id", Kind.ID), Column("annotation_set_id", Kind.ID)),
        ("translation_id",),
        foreign_keys=(
            _fk("translation_id", "translation"),
            _fk("annotation_set_id", "annotation_set"),
        ),
    ),
    Table(
        "translation_use_annotation",
        (Column("use_id", Kind.ID), Column("annotation_set_id", Kind.ID)),
        ("use_id",),
        foreign_keys=(
            _fk("use_id", "translation_use"),
            _fk("annotation_set_id", "annotation_set"),
        ),
        query_checks=(
            QueryCheck(
                "four_layer_use_annotation_text_exact",
                "SELECT 1 FROM translation_use_annotation AS a "
                "JOIN translation_use AS u ON u.id=a.use_id "
                "JOIN translation_context AS c ON c.id=u.context_id "
                "JOIN annotation_set AS s ON s.id=a.annotation_set_id "
                "WHERE s.text_unit_id!=c.source_unit_id LIMIT 1",
                (
                    "translation_use_annotation",
                    "translation_use",
                    "translation_context",
                    "annotation_set",
                ),
            ),
        ),
    ),
)


def schemas() -> dict[str, JsonValue]:
    """JSON declarations share the closed models used on typed reads."""
    declarations: dict[str, JsonValue] = {}
    models = (
        ("FrameSource", four_layer.Source),
        ("FrameSemanticVariant", SemanticVariant),
        ("FrameLeafSchema", LeafSchema),
        ("FrameProjection", Projection),
        ("FrameTarget", Target),
        ("FormSignature", tuple[Signature, ...]),
        ("FormCases", dict[Code, Annotated[tuple[FormPart, ...], Field(min_length=1)]]),
        ("BindingSource", SourceDescriptor),
        ("BindingSpan", SourceSpan),
        ("BindingValues", dict[Code, TypedValue]),
        ("BindingTrace", Annotated[tuple[TracePiece, ...], Field(min_length=1)]),
        ("LeafSpans", tuple[Span, ...]),
        ("AnnotationOccurrences", tuple[Annotation, ...]),
        ("RenderNodePath", Annotated[tuple[UInt, ...], Field(min_length=1)]),
        ("RenderSourceOrdinals", Annotated[tuple[UInt, ...], Field(min_length=1)]),
        ("RenderRanges", Annotated[tuple[Span, ...], Field(min_length=1)]),
        ("RenderLeafOccurrence", RenderLeafOccurrence),
    )
    for name, model in models:
        adapter = (
            TypeAdapter(model)
            if isinstance(model, type) and issubclass(model, RecordData)
            else TypeAdapter(
                model, config=ConfigDict(strict=True, regex_engine="python-re")
            )
        )
        declarations[name] = parse(canonical(adapter.json_schema()))
    return declarations

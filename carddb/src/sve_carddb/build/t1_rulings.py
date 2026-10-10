"""Build-only ruling reference storage never grants a template-wide applicability edge."""

from pydantic import JsonValue, TypeAdapter

from sve_carddb.build.model import Column, ForeignKey, Kind, Table
from sve_carddb.contracts.rulings import Resolution, RetainedReference
from sve_carddb.core.json import canonical, parse

DOCUMENT = ForeignKey(
    ("ruling_id", "revision"), "ruling_document", ("ruling_id", "revision")
)
TABLES = (
    Table(
        "ruling_document",
        (
            Column("ruling_id", Kind.ID),
            Column("revision", Kind.UINT),
            Column("source_id", Kind.ID),
            Column("source_path", Kind.TEXT),
            Column("source_hash", Kind.TEXT),
            Column("raw_text", Kind.TEXT),
        ),
        ("ruling_id", "revision"),
        foreign_keys=(ForeignKey(("source_id",), "source_record", ("id",)),),
    ),
    Table(
        "ruling_resolution",
        (
            Column("id", Kind.ID),
            Column("ruling_id", Kind.ID),
            Column("revision", Kind.UINT),
            Column("frame_id", Kind.ID, nullable=True),
            Column("payload", Kind.JSON, json_schema="RulingResolution"),
        ),
        ("id",),
        foreign_keys=(
            DOCUMENT,
            ForeignKey(("frame_id",), "sentence_template", ("id",)),
        ),
    ),
    Table(
        "ruling_retained_reference",
        (
            Column("ruling_id", Kind.ID),
            Column("revision", Kind.UINT),
            Column("reference_ordinal", Kind.UINT),
            Column("payload", Kind.JSON, json_schema="RulingRetainedReference"),
        ),
        ("ruling_id", "revision", "reference_ordinal"),
        foreign_keys=(DOCUMENT,),
    ),
)


def schemas() -> dict[str, JsonValue]:
    """SQLite JSON validation shares the closed authored and resolution shapes."""
    return {
        name: parse(canonical(TypeAdapter(model).json_schema()))
        for name, model in (
            ("RulingResolution", Resolution),
            ("RulingRetainedReference", RetainedReference),
        )
    }

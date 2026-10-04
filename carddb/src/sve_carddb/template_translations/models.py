"""Shared semantic definition fields for editable current templates."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves constrained wire annotations
from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.products.models import Code
from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.template_parameters.models import Schema, SourceSpan
from sve_carddb.template_translations.flavor_models import FlavorSpan

TemplateId = Annotated[
    str, Field(pattern=r"^[TC](?:[0-9a-f]{10}|(?:[0-9a-f]{2}){8,32})\Z")
]


class Definition(RecordData):
    id: TemplateId
    inventory_id: Text
    source_span: Annotated[SourceSpan | FlavorSpan, Field(discriminator="role")]
    source_lang: Literal["ja", "en"]
    normalizer_version: Code
    semantic_variant: Code
    parameter_schema: Schema
    content_hash: Hash
    supersedes_id: TemplateId | None

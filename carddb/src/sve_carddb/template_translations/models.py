"""Shared semantic definition fields for editable current templates."""

from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.contracts.template_parameters import Role, Schema
from sve_carddb.core.models import Hash, RecordData
from sve_carddb.products.models import Code

TemplateId = Annotated[str, Field(pattern=r"^[TC](?:[0-9a-f]{2}){8,32}\Z")]


class Definition(RecordData):
    id: TemplateId
    normalized_hash: Hash
    role: Role
    source_lang: Literal["ja", "en"]
    normalizer_version: Code
    semantic_variant: Code
    parameter_schema: Schema
    content_hash: Hash

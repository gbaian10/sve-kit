"""Build-time source positions; none of these records asserts human adoption."""

from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.template_sources.normalizer import Role


class Entry(RecordData):
    id: Text
    level: Literal["sentence"] = "sentence"
    source_ref: SourceRef
    line_ordinal: Annotated[int, Field(ge=0)]
    role: Role
    normalizer_id: Text
    normalized_hash: Hash

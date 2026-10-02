"""Hash-only candidate inventories; none of these records asserts human adoption."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic resolves constrained nested types at runtime

from typing import Annotated, Literal

from pydantic import Field, JsonValue

from sve_carddb.build_inputs import Revision
from sve_carddb.catalog.adoption_models import SourceRef
from sve_carddb.registry.records import Hash, RecordData, Text
from sve_carddb.template_sources.normalizer import Role


class Recipe(RecordData):
    id: Text
    code_revision: Revision
    code_path: Text
    code_hash: Hash
    config: dict[str, JsonValue]
    config_hash: Hash


class Entry(RecordData):
    id: Text
    level: Literal["sentence"] = "sentence"
    source_ref: SourceRef
    line_ordinal: Annotated[int, Field(ge=0)]
    role: Role
    normalizer_id: Text
    normalized_hash: Hash
    legacy_fingerprint: Hash | None


class Inventory(RecordData):
    template_source_format: Literal[1] = 1
    kind: Literal["template_source_inventory"] = "template_source_inventory"
    recipes: tuple[Recipe, ...]
    entries: tuple[Entry, ...]

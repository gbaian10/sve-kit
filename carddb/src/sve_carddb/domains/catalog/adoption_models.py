"""Source references and editable catalog values."""

from typing import Annotated, Literal

from pydantic import Field, JsonValue

from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.provenance import BuildContext, Version
from sve_carddb.core.regions import Region
from sve_carddb.domains.catalog.symbols import Spelling
from sve_carddb.domains.products.models import Code, Lang

Kind = Literal[
    "vocabulary_adoption",
    "language_adoption",
    "search_alias_adoption",
    "text_symbol_adoption",
    "rules_name_adoption",
    "route_override_adoption",
    "default_printing_adoption",
]
VocabularyKind = Literal[
    "class", "type", "special_kind", "rarity", "trait", "title", "frame", "stamp_series"
]


class Batch(RecordData):
    batch_id: Hash


class ReviewContext(RecordData):
    context: BuildContext
    source_batches: tuple[Batch, ...]


class SourceRef(Batch):
    source_version_id: Version
    parser: Text
    locator: Text
    text_hash: Hash


class ImageRef(Batch):
    source_version_id: Version
    raw_hash: Hash
    printing_id: Text
    face_id: Text


class TextEvidence(RecordData):
    source_ref: SourceRef
    role: Text


class ImageEvidence(RecordData):
    image_ref: ImageRef
    role: Text


Evidence = TextEvidence | ImageEvidence


class AuthoredText(RecordData):
    kind: Literal["authored"]
    lang: Lang
    text: Text


class SourceText(RecordData):
    kind: Literal["source"]
    source_ref: SourceRef


TextValue = Annotated[AuthoredText | SourceText, Field(discriminator="kind")]


class VocabularySubject(RecordData):
    kind: VocabularyKind
    code: Code


class RawMapping(RecordData):
    region: Region
    lang: Lang
    raw: Text
    source_ref: SourceRef
    special_kinds: tuple[Code, ...]


class VocabularyValue(RecordData):
    label: TextValue
    raw_mappings: tuple[RawMapping, ...]
    active: bool


class LanguageSubject(RecordData):
    code: Lang


class LanguageValue(RecordData):
    display_name: Text
    fallback_order: tuple[Lang, ...]


class AliasSubject(RecordData):
    kind: Code
    code: Text
    lang: Lang
    text: Text


class Normalizer(RecordData):
    version: Text
    config: dict[str, JsonValue]


class AliasValue(RecordData):
    normalized: Text
    normalizer: Normalizer


class SymbolSubject(RecordData):
    id: Text


class SourceLocalization(RecordData):
    lang: Lang
    name: TextValue
    tooltip: TextValue
    copy_pattern: TextValue


class SymbolValue(RecordData):
    code: Code
    parameter_schema: dict[str, JsonValue]
    keyword_id: Text | None
    spellings: Annotated[tuple[Spelling, ...], Field(min_length=1)]
    source_localization: SourceLocalization


class NameSubject(RecordData):
    face_id: Text
    region: Region
    role: Literal["collab", "treated_as"]


class NameIdentity(RecordData):
    face_id: Text
    card_id: Text


class NameObservation(RecordData):
    printing_id: Text
    face_id: Text
    source_ref: SourceRef


class NameValue(RecordData):
    names: Annotated[tuple[SourceText, ...], Field(min_length=1)]
    identity_ref: NameIdentity
    observations: Annotated[tuple[NameObservation, ...], Field(min_length=1)]
    name_basis_hash: Hash


class RouteSubject(RecordData):
    region: Region
    route_key: Text


class RouteCandidate(RecordData):
    printing_id: Text
    card_id: Text
    region: Region
    card_no: Text
    card_no_state: Literal["official"]
    variant_key: Text


class RouteValue(RecordData):
    printing_id: Text
    candidates: Annotated[tuple[RouteCandidate, ...], Field(min_length=2)]
    candidates_hash: Hash


class DefaultSubject(RecordData):
    card_id: Text
    region: Region


class DefaultValue(RecordData):
    printing_id: Text
    candidates: Annotated[tuple[Text, ...], Field(min_length=1)]
    candidates_hash: Hash

"""Strict catalog/display adoption wire types; candidates never enter this boundary."""

# ruff: file-ignore[typing-only-first-party-import] -- Pydantic evaluates nested types and constrained aliases at runtime

from typing import Annotated, Literal

from pydantic import Field, JsonValue

from sve_carddb.build_inputs import BuildContext, Revision, Version
from sve_carddb.catalog.symbols import Spelling
from sve_carddb.products.models import Code, DecisionMetadata, Lang
from sve_carddb.registry.records import Hash, RecordData, Region, Text

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
    store_id: Text
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


class Predecessor(RecordData):
    record_key: Text
    record_hash: Hash
    decision_id: Text


class Dependency(RecordData):
    table: Literal[
        "vocabulary",
        "language",
        "text_symbol",
        "keyword",
        "stamp",
        "product_family",
        "card",
        "face",
        "printing",
        "rules_name",
    ]
    key: dict[str, JsonValue]


class AdoptionData(RecordData):
    adoption_no: Annotated[int, Field(ge=1)]
    predecessor: Predecessor | None
    review_context_hash: Hash
    dependencies: tuple[Dependency, ...]
    reason: Text


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


class VocabularyData(AdoptionData):
    subject: VocabularySubject
    value: VocabularyValue | None


class LanguageSubject(RecordData):
    code: Lang


class LanguageValue(RecordData):
    display_name: Text
    fallback_order: tuple[Lang, ...]


class LanguageData(AdoptionData):
    subject: LanguageSubject
    value: LanguageValue | None


class AliasSubject(RecordData):
    kind: Code
    code: Text
    lang: Lang
    text: Text


class Normalizer(RecordData):
    version: Text
    program_revision: Revision
    code_path: Text
    code_hash: Hash
    config: dict[str, JsonValue]
    config_hash: Hash


class AliasValue(RecordData):
    normalized: Text
    normalizer: Normalizer


class AliasData(AdoptionData):
    subject: AliasSubject
    value: AliasValue | None


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


class SymbolData(AdoptionData):
    subject: SymbolSubject
    value: SymbolValue | None


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


class NameData(AdoptionData):
    subject: NameSubject
    value: NameValue | None


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
    identity_ref: Predecessor


class RouteValue(RecordData):
    printing_id: Text
    candidates: Annotated[tuple[RouteCandidate, ...], Field(min_length=2)]
    candidates_hash: Hash


class RouteData(AdoptionData):
    subject: RouteSubject
    value: RouteValue | None


class DefaultSubject(RecordData):
    card_id: Text
    region: Region


class DefaultValue(RecordData):
    printing_id: Text
    candidates: Annotated[tuple[Text, ...], Field(min_length=1)]
    candidates_hash: Hash


class DefaultData(AdoptionData):
    subject: DefaultSubject
    value: DefaultValue | None


class AdoptionRecord(RecordData):
    record_key: Text
    filing_key: Annotated[str, Field(pattern=r"^[A-Za-z0-9_-]+\Z")]
    evidence: tuple[Evidence, ...]


class VocabularyRecord(AdoptionRecord):
    kind: Literal["vocabulary_adoption"]
    data: VocabularyData


class LanguageRecord(AdoptionRecord):
    kind: Literal["language_adoption"]
    data: LanguageData


class AliasRecord(AdoptionRecord):
    kind: Literal["search_alias_adoption"]
    data: AliasData


class SymbolRecord(AdoptionRecord):
    kind: Literal["text_symbol_adoption"]
    data: SymbolData


class NameRecord(AdoptionRecord):
    kind: Literal["rules_name_adoption"]
    data: NameData


class RouteRecord(AdoptionRecord):
    kind: Literal["route_override_adoption"]
    data: RouteData


class DefaultRecord(AdoptionRecord):
    kind: Literal["default_printing_adoption"]
    data: DefaultData


Record = Annotated[
    VocabularyRecord
    | LanguageRecord
    | AliasRecord
    | SymbolRecord
    | NameRecord
    | RouteRecord
    | DefaultRecord,
    Field(discriminator="kind"),
]


class Decision(DecisionMetadata):
    state: Literal["confirmed"]
    category: Kind
    policy_id: Text


class Shard(RecordData):
    review_context: ReviewContext
    default_decision_id: Text
    records: Annotated[tuple[Record, ...], Field(min_length=1)]
    decisions: Annotated[tuple[Decision, ...], Field(min_length=1, max_length=1)]


class CatalogShard(Shard):
    catalog_adoption_format: Literal[1]
    kind: Literal["catalog_adoption_shard"]


class DisplayShard(Shard):
    display_override_format: Literal[1]
    kind: Literal["display_override_shard"]


class CatalogIndex(RecordData):
    catalog_adoption_format: Literal[1, 2]
    kind: Literal["catalog_adoption_index"]
    includes: dict[str, Hash]


class DisplayIndex(RecordData):
    display_override_format: Literal[1]
    kind: Literal["display_override_index"]
    includes: dict[str, Hash]

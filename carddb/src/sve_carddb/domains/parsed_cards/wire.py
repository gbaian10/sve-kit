"""Readable card documents and separately interned, lossless build evidence."""

from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.core.models import Hash, Instant, RecordData, Text, UInt
from sve_carddb.core.provenance import Revision, Version
from sve_carddb.core.regions import Region
from sve_carddb.domains.card_extras.models import RelatedLink
from sve_carddb.domains.parsed_cards.models import Family, QAIdentity
from sve_carddb.domains.products.identity_models import ExpansionLink, ProductLink
from sve_carddb.domains.text_observations.presence import PresenceState
from sve_carddb.parse.pages import extract_en, extract_jp

Key = Annotated[str, Field(pattern=r"^[a-z]+-[0-9a-f]{16}$")]


class SourceRef(RecordData):
    id: Version
    url: Text
    fetched_at: Instant


class JPRecord(RecordData):
    number: str
    faces: tuple[extract_jp.Face, ...]
    release_date: str | None
    errata_url: str | None
    notes: tuple[str, ...]
    qa: tuple[extract_jp.QA, ...]
    products_ref: Key
    related_cards: tuple[extract_jp.RelatedHint, ...]


class ENRecord(RecordData):
    number: str
    faces: tuple[extract_en.Face, ...]
    release_date: str | None
    errata_url: str | None
    notes: tuple[str, ...]
    qa: tuple[extract_en.QA, ...]
    products_ref: Key
    related_cards: tuple[extract_jp.RelatedHint, ...]


class JPContent(RecordData):
    region: Literal["jp"]
    record: JPRecord


class ENContent(RecordData):
    region: Literal["en"]
    record: ENRecord


class Document(RecordData):
    source: SourceRef
    document: Annotated[JPContent | ENContent, Field(discriminator="region")]


class DocumentRef(RecordData):
    source: SourceRef
    document_ref: Key


class CardDocuments(RecordData):
    card_no: Text
    documents: dict[
        Key, Annotated[JPContent | ENContent, Field(discriminator="region")]
    ]
    observations: tuple[DocumentRef, ...]


class CardFile(RecordData):
    format: Literal[1] = 1
    region: Region
    family: Family
    range: str
    products: dict[Key, tuple[extract_jp.ProductHint, ...]]
    cards: tuple[CardDocuments, ...]


class Recipe(RecordData):
    recipe: Literal["sve-parsed-card-v1"]
    producer_revision: Revision


class PresenceParser(RecordData):
    recipe: Literal["effect-presence-v1"]
    parser_version: Literal["effect-presence-v1/detail-v1"]
    template_id: Text | None


class PresenceTemplate(RecordData):
    parser: Key
    source_index: UInt
    container_locator: Key
    state: PresenceState
    reason_code: Text


class LocalSourceBlock(RecordData):
    kind: Literal["source_block"]
    product_block_ordinal: UInt


class ProductTemplate(RecordData):
    ordinal: UInt
    name: str
    date_raw: str | None
    href_raw: tuple[str, ...]
    resolved_urls: tuple[str, ...]
    matches: tuple[
        Annotated[
            ProductLink | ExpansionLink | LocalSourceBlock, Field(discriminator="kind")
        ],
        ...,
    ]
    diagnostics: tuple[str, ...]


class Shared(RecordData):
    recipes: dict[Key, Recipe]
    parsers: dict[Key, PresenceParser]
    locators: dict[Key, Text]
    presence: dict[Key, PresenceTemplate]
    products: dict[Key, tuple[ProductTemplate, ...]]
    qa: dict[Key, tuple[QAIdentity, ...]]
    related: dict[Key, tuple[RelatedLink, ...]]
    errata_urls: dict[Key, tuple[str, ...]]


class RawEvidence(RecordData):
    raw_sha256: Hash
    raw_bytes: UInt
    first_receipt_id: Hash
    descriptor_sha256: Hash
    etag: str | None
    last_modified: str | None


class PresenceRef(RecordData):
    template: Key
    result_hash: Hash


class LayoutRefs(RecordData):
    presence: tuple[PresenceRef, ...]
    whitespace: tuple[str | None, ...]
    products: Key
    qa: Key
    related: Key
    errata_urls: Key


class ObservationEvidence(RecordData):
    recipe: Key
    raw: RawEvidence
    layout: LayoutRefs
    content_hash: Hash


class EvidenceFile(RecordData):
    format: Literal[1] = 1
    region: Region
    family: Family
    range: str
    shared: Shared
    observations: dict[Version, ObservationEvidence]

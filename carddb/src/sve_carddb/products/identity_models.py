"""Closed product-identity-v1 envelopes and exact source match types."""

from typing import Annotated, Literal
from urllib.parse import parse_qsl, urlsplit

from pydantic import Field, field_validator, model_validator

from sve_carddb.build_inputs import Version
from sve_carddb.products.models import Code, DecisionMetadata, Evidence
from sve_carddb.registry.records import Hash, RecordData, Region, Text


def official_url(value: str, region: Region, purpose: str) -> bool:
    """Validate link purpose without rewriting the exact resolved URL."""
    try:
        parts = urlsplit(value)
        host = (
            "shadowverse-evolve.com" if region == "jp" else "en.shadowverse-evolve.com"
        )
        search = "/cardlist/cardsearch" if region == "jp" else "/cards/searchresults"
        return (
            parts.scheme == "https"
            and parts.netloc == host
            and (
                parts.path.startswith("/products/")
                if purpose == "product"
                else parts.path in {search, search + "/"}
            )
        )
    except ValueError:
        return False


def expansion(url: str) -> tuple[str | None, bool]:
    """Decode once, distinguishing missing parameters from empty or repeated ones."""
    values = [
        value
        for key, value in parse_qsl(urlsplit(url).query, keep_blank_values=True)
        if key == "expansion"
    ]
    if not values:
        return None, False
    if len(values) != 1 or not values[0]:
        return None, True
    return values[0], False


class ProductLink(RecordData):
    kind: Literal["product_link"]
    product_url: Text
    expansion_code: Text | None


class ExpansionLink(RecordData):
    kind: Literal["expansion_link"]
    search_url: Text
    expansion_code: Text


class SourceBlock(RecordData):
    kind: Literal["source_block"]
    source_version_id: Version
    product_block_ordinal: Annotated[int, Field(ge=0, le=9_007_199_254_740_991)]


Match = Annotated[
    ProductLink | ExpansionLink | SourceBlock, Field(discriminator="kind")
]


class IdentityData(RecordData):
    product_id: Code
    region: Region
    match: Match

    @model_validator(mode="after")
    def _link(self) -> IdentityData:
        match = self.match
        if isinstance(match, ProductLink) and not official_url(
            match.product_url, self.region, "product"
        ):
            raise ValueError("Product identity URL purpose/region mismatch")
        if isinstance(match, ExpansionLink):
            code, ambiguous = expansion(match.search_url)
            if (
                not official_url(match.search_url, self.region, "search")
                or ambiguous
                or code != match.expansion_code
            ):
                raise ValueError("Product identity expansion link mismatch")
        return self


class IdentityRecord(RecordData):
    record_key: Text
    kind: Literal["product_identity"]
    filing_key: Region
    data: IdentityData
    evidence: Annotated[tuple[Evidence, ...], Field(min_length=1)]


class IdentityDecision(DecisionMetadata):
    state: Literal["confirmed"]
    category: Literal["product_identity"]
    policy_id: Literal["product-identity-v1"]


class _Envelope(RecordData):
    product_identity_format: Literal[1]

    @field_validator("product_identity_format", mode="before")
    @classmethod
    def _version(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Product identity format must be an integer")
        return value


class IdentityIndex(_Envelope):
    kind: Literal["product_identity_index"]
    includes: dict[str, Hash]


class IdentityShard(_Envelope):
    kind: Literal["product_identity_shard"]
    default_decision_id: Text
    records: Annotated[tuple[IdentityRecord, ...], Field(min_length=1)]
    decisions: Annotated[
        tuple[IdentityDecision, ...], Field(min_length=1, max_length=1)
    ]

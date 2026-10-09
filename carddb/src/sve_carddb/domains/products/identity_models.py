"""Closed product-identity-v1 shards and exact source match types."""

from typing import Annotated, Literal
from urllib.parse import parse_qsl, urlsplit

from pydantic import Field, computed_field, field_validator, model_validator

from sve_carddb.core.json import canonical
from sve_carddb.core.models import RecordData, Text
from sve_carddb.core.provenance import Version
from sve_carddb.core.regions import Region
from sve_carddb.domains.products.models import Code, Evidence


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
    kind: Literal["product_identity"]
    filing_key: Region
    data: IdentityData
    evidence: Annotated[tuple[Evidence, ...], Field(min_length=1)]

    @computed_field  # type: ignore[prop-decorator]  # Pydantic serializes this property; mypy cannot compose property decorators.
    @property
    def record_key(self) -> str:
        """Derive identity independently of mutable values and authored metadata."""
        return canonical(
            [self.kind, self.data.region, self.data.match.model_dump(mode="json")]
        ).decode()


class IdentityShard(RecordData):
    format: Literal[1]
    kind: Literal["product_identity_shard"]
    records: Annotated[tuple[IdentityRecord, ...], Field(min_length=1)]

    @field_validator("format", mode="before")
    @classmethod
    def _version(cls, value: object) -> object:
        if type(value) is not int:
            raise ValueError("Product identity format must be an integer")
        return value

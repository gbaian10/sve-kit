"""Exact official card-page product blocks, independent of owner and ID allocation."""

import re
from dataclasses import dataclass
from datetime import date
from typing import TYPE_CHECKING
from urllib.parse import urljoin

from sve_carddb.core.json import canonical
from sve_carddb.domains.products.identity_models import (
    ExpansionLink,
    ProductLink,
    SourceBlock,
    expansion,
    official_url,
)
from sve_carddb.parse.html import Queryable, attribute, select_all

if TYPE_CHECKING:
    from selectolax.lexbor import LexborNode

    from sve_carddb.core.provenance import Source
    from sve_carddb.core.regions import Region
    from sve_carddb.domains.products.identity_models import Match
    from sve_carddb.domains.products.models import Precision

PARSER = "official-product-block-v1"
_MONTHS = {
    name: index
    for index, name in enumerate(
        (
            "Jan",
            "Feb",
            "Mar",
            "Apr",
            "May",
            "Jun",
            "Jul",
            "Aug",
            "Sep",
            "Oct",
            "Nov",
            "Dec",
        ),
        1,
    )
}


@dataclass(frozen=True)
class ProductBlock:
    ordinal: int
    name: str
    date_raw: str | None
    href_raw: tuple[str, ...]
    resolved_urls: tuple[str, ...]
    matches: tuple[Match, ...]
    diagnostics: tuple[str, ...]

    @property
    def locator(self) -> str:
        """Use the specified canonical document-order locator."""
        return canonical({"product_block_ordinal": self.ordinal}).decode()


@dataclass(frozen=True)
class ProductPage:
    source: Source
    region: Region
    card_no: str
    blocks: tuple[ProductBlock, ...]


def products_tree(
    tree: Queryable, source: Source, region: Region, number: str
) -> ProductPage:
    """Project product blocks from one validated physical page DOM."""
    blocks = tuple(
        _block(node, ordinal, source, region)
        for ordinal, node in enumerate(
            select_all(tree, ".cardlist-Under .cardlist-Detail_Products_Inner")
        )
    )
    if len(select_all(tree, ".cardlist-Detail_Products_Inner")) != len(blocks):
        raise ValueError("Unrecognized official product block layout")
    return ProductPage(source, region, number, blocks)


def _block(
    node: LexborNode, ordinal: int, source: Source, region: Region
) -> ProductBlock:
    names = select_all(node, ".ttl")
    dates = select_all(node, ".date")
    if len(names) != 1 or len(dates) > 1:
        raise ValueError("Ambiguous official product name/date layout")
    name = names[0].text(strip=True)
    if not name:
        raise ValueError("Official product block has no name")
    raw_date = dates[0].text(strip=True) if dates else None
    hrefs = tuple(
        href
        for link in select_all(node, "a")
        if (href := attribute(link, "href")) is not None
    )
    urls = tuple(urljoin(source.url, href) for href in hrefs)
    matches, diagnostics = _matches(urls, source.id, ordinal, region)
    return ProductBlock(ordinal, name, raw_date, hrefs, urls, matches, diagnostics)


def _matches(
    urls: tuple[str, ...], version: str, ordinal: int, region: Region
) -> tuple[tuple[Match, ...], tuple[str, ...]]:
    products: set[str] = set()
    searches: set[str] = set()
    codes: set[str] = set()
    diagnostics: set[str] = set()
    for url in urls:
        if official_url(url, region, "product"):
            products.add(url)
        elif official_url(url, region, "search"):
            searches.add(url)
            code, ambiguous = expansion(url)
            if ambiguous:
                diagnostics.add("ambiguous_expansion_parameter")
            elif code is not None:
                codes.add(code)
        else:
            diagnostics.add("unverified_link")
    ambiguous = (
        "ambiguous_expansion_parameter" in diagnostics
        or len(products) > 1
        or len(codes) > 1
    )
    matches: list[Match] = []
    if ambiguous:
        diagnostics.add("ambiguous_product_block")
    elif products:
        matches.append(
            ProductLink(
                kind="product_link",
                product_url=next(iter(products)),
                expansion_code=next(iter(codes)) if codes else None,
            )
        )
    elif codes:
        matches.extend(
            ExpansionLink(kind="expansion_link", search_url=url, expansion_code=code)
            for url in sorted(searches)
            if (code := expansion(url)[0]) is not None
        )
    matches.append(
        SourceBlock(
            kind="source_block",
            source_version_id=version,
            product_block_ordinal=ordinal,
        )
    )
    return tuple(matches), tuple(sorted(diagnostics))


def date_fields(raw: str | None) -> tuple[str | None, Precision]:
    """Recognize explicit dates; never turn partial or invalid dates into a day."""
    raw = "" if raw is None else raw
    full = re.fullmatch(r"(\d{4})-(\d{2})-(\d{2})", raw)
    english = re.fullmatch(r"([A-Za-z]{3})\.? (\d{1,2}), (\d{4})", raw)
    try:
        if full:
            return date(*map(int, full.groups())).isoformat(), "day"
        if english and english[1] in _MONTHS:
            return date(
                int(english[3]), _MONTHS[english[1]], int(english[2])
            ).isoformat(), "day"
        if re.fullmatch(r"\d{4}-(0[1-9]|1[0-2])", raw) and int(raw[:4]) > 0:
            return None, "month"
        if re.fullmatch(r"\d{4}", raw) and int(raw) > 0:
            return None, "year"
    except ValueError:
        return None, "unknown"
    return None, "unknown"

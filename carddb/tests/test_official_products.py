"""Pinned official link grammar, block boundaries and date precision."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.products.identity_models import (
    ExpansionLink,
    ProductLink,
    SourceBlock,
    expansion,
)
from sve_carddb.products.official import date_fields
from sve_carddb.products.official import parse_products as parse_verified_products
from sve_carddb.snapshot.values import digest
from sve_carddb.sources import official_en, official_jp

from .product_identity_fixtures import IdentityFixture, html
from .product_identity_fixtures import identity_fixture as identity_fixture  # ruff: ignore[useless-import-alias] -- shared fixture

if TYPE_CHECKING:
    from sve_carddb.build_inputs import Source
    from sve_carddb.products.models import Precision
    from sve_carddb.products.official import ProductPage
    from sve_carddb.registry.records import Region


def parse_products(raw: bytes, source: Source, region: Region) -> ProductPage:
    return parse_verified_products(
        raw, source.model_copy(update={"sha256": digest(raw)}), region
    )


@pytest.mark.parametrize(
    ("query", "code", "ambiguous"),
    [
        ("", None, False),
        ("Expansion=A", None, False),
        ("expansion=", None, True),
        ("expansion=A&expansion=A", None, True),
        ("expansion=A&expansion=B", None, True),
        ("expansion=&expansion=A", None, True),
        ("expansion=%2541", "%41", False),
        ("expansion=BP12-BP13", "BP12-BP13", False),
        ("expansion=a%2Bb", "a+b", False),
        ("expansion=a+b", "a b", False),
        ("expansion=lower", "lower", False),
    ],
)
def test_expansion_decodes_once_preserves_empty_duplicates_case_and_compound(
    query: str, code: str | None, ambiguous: bool
) -> None:
    assert expansion("https://shadowverse-evolve.com/cardlist/cardsearch?" + query) == (
        code,
        ambiguous,
    )


@pytest.mark.parametrize(
    "query",
    [
        "expansion=",
        "expansion=A&expansion=A",
        "expansion=A&expansion=B",
        "expansion=&expansion=A",
    ],
)
def test_ambiguous_query_never_falls_back_to_url_only(
    identity_fixture: IdentityFixture, query: str
) -> None:
    page = parse_products(
        html(links=("/products/synthetic/", "/cardlist/cardsearch?" + query)),
        identity_fixture.pages[0].source,
        "jp",
    )
    assert len(page.blocks[0].matches) == 1
    assert isinstance(page.blocks[0].matches[0], SourceBlock)
    assert "ambiguous_expansion_parameter" in page.blocks[0].diagnostics
    assert "ambiguous_product_block" in page.blocks[0].diagnostics


def test_missing_code_is_null_and_never_a_wildcard(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    page = parse_products(
        html(links=("/products/synthetic/",)), fixture.pages[0].source, "jp"
    )
    assert page.blocks[0].matches[0] == ProductLink(
        kind="product_link",
        product_url="https://shadowverse-evolve.com/products/synthetic/",
        expansion_code=None,
    )
    assert fixture.load().match("jp", page.blocks[0].matches) is None


def test_entity_relative_url_query_order_and_exact_code(
    identity_fixture: IdentityFixture,
) -> None:
    raw = html(
        links=(
            "../products/synthetic/?b=2&amp;a=1",
            "/cardlist/cardsearch?unused=X&amp;expansion=a%2Bb",
        )
    )
    page = parse_products(raw, identity_fixture.pages[0].source, "jp")
    assert page.blocks[0].resolved_urls == (
        "https://shadowverse-evolve.com/products/synthetic/?b=2&a=1",
        "https://shadowverse-evolve.com/cardlist/cardsearch?unused=X&expansion=a%2Bb",
    )
    assert page.blocks[0].matches[0] == ProductLink(
        kind="product_link",
        product_url="https://shadowverse-evolve.com/products/synthetic/?b=2&a=1",
        expansion_code="a+b",
    )
    assert page.blocks[0].href_raw[0] == "../products/synthetic/?b=2&a=1"


@pytest.mark.parametrize(
    "link",
    [
        "http://shadowverse-evolve.com/products/test/",
        "https://dev.shadowverse-evolve.com/products/test/",
        "https://en.shadowverse-evolve.com/products/test/",
        "https://other.invalid/products/test/",
        "/events/",
        "/notsearch?expansion=Test-A",
        "/cardlist/cardsearch_ex?expansion=Test-A",
    ],
)
def test_unverified_links_are_diagnostics_only(
    identity_fixture: IdentityFixture, link: str
) -> None:
    page = parse_products(html(links=(link,)), identity_fixture.pages[0].source, "jp")
    assert all(isinstance(match, SourceBlock) for match in page.blocks[0].matches)
    assert "unverified_link" in page.blocks[0].diagnostics
    assert page.blocks[0].href_raw == (link,)


def test_dev_url_is_retained_while_official_search_can_match(
    identity_fixture: IdentityFixture,
) -> None:
    page = parse_products(
        html(
            links=(
                "https://dev.shadowverse-evolve.com/products/bp14/",
                "/cardlist/cardsearch?expansion=BP14",
            )
        ),
        identity_fixture.pages[0].source,
        "jp",
    )
    assert isinstance(page.blocks[0].matches[0], ExpansionLink)
    assert (
        page.blocks[0].matches[0].search_url
        == "https://shadowverse-evolve.com/cardlist/cardsearch?expansion=BP14"
    )
    assert "dev.shadowverse-evolve.com" in page.blocks[0].resolved_urls[0]


@pytest.mark.parametrize(
    "links",
    [
        ("/products/a/", "/products/b/"),
        (
            "/products/a/",
            "/cardlist/cardsearch?expansion=A",
            "/cardlist/cardsearch?expansion=B",
        ),
    ],
)
def test_multiple_clues_never_form_cartesian_product(
    identity_fixture: IdentityFixture, links: tuple[str, ...]
) -> None:
    page = parse_products(html(links=links), identity_fixture.pages[0].source, "jp")
    assert len(page.blocks[0].matches) == 1
    assert isinstance(page.blocks[0].matches[0], SourceBlock)
    assert "ambiguous_product_block" in page.blocks[0].diagnostics


def test_document_order_not_name_order_and_no_cross_block_clues(
    identity_fixture: IdentityFixture,
) -> None:
    extra = '<div class="cardlist-Detail_Products_Inner"><p class="ttl">Another synthetic product</p><a href="/cardlist/cardsearch?expansion=OTHER">List</a></div>'
    page = parse_products(
        html(extra=extra, links=("/products/synthetic/",)),
        identity_fixture.pages[0].source,
        "jp",
    )
    assert [block.ordinal for block in page.blocks] == [0, 1]
    assert isinstance(page.blocks[0].matches[0], ProductLink)
    assert page.blocks[0].matches[0].expansion_code is None
    assert isinstance(page.blocks[1].matches[0], ExpansionLink)
    assert page.blocks[1].matches[0].expansion_code == "OTHER"
    assert page.blocks[1].locator == '{"product_block_ordinal":1}'


def test_shared_product_url_with_distinct_codes_remains_distinct(
    identity_fixture: IdentityFixture,
) -> None:
    source = identity_fixture.pages[0].source
    pages = [
        parse_products(
            html(links=("/products/csd03/", "/cardlist/cardsearch?expansion=" + code)),
            source,
            "jp",
        )
        for code in ("CSD03A", "CSD03B")
    ]
    assert pages[0].blocks[0].matches[0] != pages[1].blocks[0].matches[0]


@pytest.mark.parametrize(
    ("raw", "expected", "precision"),
    [
        (None, None, "unknown"),
        ("2026-09-30", "2026-09-30", "day"),
        ("Sep. 30, 2026", "2026-09-30", "day"),
        ("Sep 30, 2026", "2026-09-30", "day"),
        ("2026-09", None, "month"),
        ("2026", None, "year"),
        ("2026-02-30", None, "unknown"),
        ("0000", None, "unknown"),
        ("0000-01", None, "unknown"),
        ("2026-13", None, "unknown"),
        ("TBA", None, "unknown"),
    ],
)
def test_partial_unknown_and_invalid_dates_never_gain_a_day(
    raw: str | None, expected: str | None, precision: Precision
) -> None:
    assert date_fields(raw) == (expected, precision)


def test_english_search_uses_exact_expansion_parameter(
    identity_fixture: IdentityFixture,
) -> None:
    raw = html(
        number="TEST-001EN",
        links=("/products/test/", "/cards/searchresults?expansion=En-Code"),
    )
    source = identity_fixture.pages[0].source.model_copy(
        update={"url": official_en.card_url("TEST-001EN"), "sha256": digest(raw)}
    )
    page = parse_products(raw, source, "en")
    assert page.region == "en"
    assert page.card_no == "TEST-001EN"
    assert page.blocks[0].matches[0] == ProductLink(
        kind="product_link",
        product_url="https://en.shadowverse-evolve.com/products/test/",
        expansion_code="En-Code",
    )


@pytest.mark.parametrize(
    "problem",
    [
        "wrong_number",
        "wrong_region",
        "wrong_kind",
        "extra_query",
        "outside_layout",
        "two_names",
        "two_dates",
        "empty_name",
    ],
)
def test_page_and_layout_boundaries_fail(
    identity_fixture: IdentityFixture, problem: str
) -> None:
    raw = html()
    source = identity_fixture.pages[0].source
    if problem == "wrong_number":
        source = source.model_copy(update={"url": official_jp.card_url("WRONG")})
    elif problem == "wrong_region":
        source = source.model_copy(update={"url": official_en.card_url("TEST-001")})
    elif problem == "wrong_kind":
        source = source.model_copy(update={"kind": "official_api"})
    elif problem == "extra_query":
        source = source.model_copy(update={"url": source.url + "&extra=1"})
    elif problem == "outside_layout":
        raw = raw.replace(b'class="cardlist-Under"', b'class="unknown-layout"')
    elif problem == "two_names":
        raw = raw.replace(
            b"Booster Pack Synthetic</p>",
            b'Booster Pack Synthetic</p><p class="ttl">Extra</p>',
        )
    elif problem == "two_dates":
        raw = raw.replace(b"2026-09-30</p>", b'2026-09-30</p><p class="date">Extra</p>')
    else:
        raw = html(name="")
    with pytest.raises(ValueError, match=r"mismatch|HTML|layout|name|asked for"):
        parse_products(raw, source, "jp")


@pytest.mark.parametrize("field", ["sha256", "parser_version"])
def test_parser_rejects_incorrect_raw_binding_and_parser_pin(
    identity_fixture: IdentityFixture, field: str
) -> None:
    source = identity_fixture.pages[0].source.model_copy(
        update={field: "sha256:" + "0" * 64 if field == "sha256" else "not-the-parser"}
    )
    with pytest.raises(ValueError, match="mismatch"):
        parse_verified_products(html(), source, "jp")


def test_formal_product_url_never_also_yields_expansion_link(
    identity_fixture: IdentityFixture,
) -> None:
    page = parse_products(html(), identity_fixture.pages[0].source, "jp")
    assert [match.kind for match in page.blocks[0].matches] == [
        "product_link",
        "source_block",
    ]

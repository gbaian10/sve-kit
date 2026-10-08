import re
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

import httpx
import orjson
import pytest

from sve_carddb.ingest.archive.manifest import Kind, Manifest, Region
from sve_carddb.ingest.crawl.crawl import Crawler
from sve_carddb.ingest.crawl.crawl_svwb import SVWB_SITE, cards, stored_image_urls
from sve_carddb.ingest.http.client import Client, ClientPolicy, FetchError
from sve_carddb.ingest.http.throttle import CircuitBreaker, Throttle
from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.ingest.http.writer import Writer
from sve_carddb.parse.pages import official_svwb as svwb

from .fakewb import FakeWb, image_hash

if TYPE_CHECKING:
    from .conftest import FakeClock


def make_crawler(
    manifest: Manifest, root: Path, clock: FakeClock, site: FakeWb
) -> Crawler:
    http = httpx.AsyncClient(transport=httpx.MockTransport(site))
    throttle = Throttle(0.0, 0.0, clock=clock, sleep=clock.sleep)
    policy = ClientPolicy(wait_initial=0.0, wait_max=0.0, wait_jitter=0.0)
    client = Client(http, throttle, manifest, run_id="test", policy=policy, clock=clock)
    return Crawler(
        client=client,
        writer=Writer(root, manifest),
        manifest=manifest,
        breaker=CircuitBreaker(5),
        site=SVWB_SITE,
    )


def page_body(**data: object) -> bytes:
    base: dict[str, object] = {
        "count": svwb.MIN_CARDS,
        "sort_card_id_list": [1],
        "card_details": {"1": {}},
    }
    return orjson.dumps({"data_headers": {"result_code": 1}, "data": base | data})


# --- URLs and parsing ---------------------------------------------------------


def test_list_url_names_the_language_the_header_sends() -> None:
    url = svwb.list_url("cht", 60)
    assert url == (
        "https://shadowverse-wb.com/web/CardList/cardList"
        "?include_token=1&lang=cht&offset=60"
    )
    assert svwb.headers(url) == (("Lang", "cht"),)
    assert svwb.list_path("cht", 60) == PurePosixPath(
        "raw/svwb/api/cht/cardList-0060.json"
    )


def test_unknown_languages_are_refused() -> None:
    with pytest.raises(ValueError, match="unknown language"):
        svwb.list_url("ko", 0)
    with pytest.raises(ValueError, match="without one known lang"):
        svwb.headers("https://shadowverse-wb.com/web/CardList/cardList?offset=0")
    assert svwb.headers("https://shadowverse-wb.com/ja/") == ()


def test_only_the_official_site_is_allowed() -> None:
    assert svwb.allowed(svwb.list_url("ja", 0))
    assert not svwb.allowed("http://shadowverse-wb.com/")
    assert not svwb.allowed("https://example.com/web/CardList/cardList")


def test_image_url_and_path_follow_the_official_pattern() -> None:
    url = svwb.image_url("0123456789abcdef0123456789abcdef")
    assert url == (
        "https://shadowverse-wb.com/uploads/card_image/jpn/card/"
        "0123456789abcdef0123456789abcdef.png"
    )
    assert svwb.image_path(url) == PurePosixPath(
        "media/images/svwb/uploads/card_image/jpn/card/"
        "0123456789abcdef0123456789abcdef.png"
    )
    with pytest.raises(ValidationError):
        svwb.image_path("https://shadowverse-wb.com/x.png")


def test_image_hashes_of_a_synthetic_page() -> None:
    site = FakeWb(904)
    body = site(
        httpx.Request("GET", svwb.list_url("ja", 900), headers={"Lang": "ja"})
    ).content
    assert svwb.image_hashes(body) == [
        image_hash(10_000_900, "c"),
        image_hash(10_000_900, "e"),
        image_hash(10_000_900, "s"),
        image_hash(10_000_901, "c"),
        image_hash(10_000_902, "c"),
        image_hash(10_000_902, "e"),
        image_hash(10_000_903, "c"),
    ]


def test_image_hashes_cover_evolved_and_styles() -> None:
    card = FakeWb.details(10, "x")
    body = orjson.dumps({"data": {"card_details": {"10": card}}})
    assert svwb.image_hashes(body) == [
        image_hash(10, "c"),
        image_hash(10, "e"),
        image_hash(10, "s"),
    ]
    card["common"] = {"card_image_hash": "not-a-hash"}
    with pytest.raises(ValidationError, match="image hash"):
        svwb.image_hashes(orjson.dumps({"data": {"card_details": {"10": card}}}))


def test_parses_a_synthetic_page() -> None:
    site = FakeWb(904)
    body = site(
        httpx.Request("GET", svwb.list_url("ja", 902), headers={"Lang": "ja"})
    ).content
    parsed = svwb.parse_list(body)
    assert parsed.count == 904
    assert parsed.card_ids == [10_000_902, 10_000_903]


@pytest.mark.parametrize(
    ("body", "reason"),
    [
        (b"<html>", "not JSON"),
        (orjson.dumps({"data_headers": {"result_code": 0}}), "result_code"),
        (page_body(count=10), "expected at least"),
        (page_body(count=True), "expected at least"),
        (page_body(sort_card_id_list=[1, "2"]), "list of ints"),
        (page_body(sort_card_id_list=[1, 1]), "repeats"),
        (page_body(sort_card_id_list=list(range(31))), "lists 31"),
        (page_body(card_details={}), "no details"),
        (page_body(card_details=[]), "not an object"),
    ],
    ids=lambda value: value if isinstance(value, str) else "body",
)
def test_bad_pages_are_rejected(body: bytes, reason: str) -> None:
    with pytest.raises(ValidationError, match=reason):
        svwb.parse_list(body)


# --- crawling -----------------------------------------------------------------


async def test_every_page_of_every_language_is_stored(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    site = FakeWb()
    crawler = make_crawler(manifest, tmp_path, clock, site)
    for lang in svwb.LANGUAGES:
        assert await cards(crawler, lang) == svwb.MIN_CARDS + 5
    pages = -(-(svwb.MIN_CARDS + 5) // svwb.PAGE_SIZE)
    assert len(site.calls) == pages * len(svwb.LANGUAGES)
    last = svwb.list_url("en", (pages - 1) * svwb.PAGE_SIZE)
    assert (last, "en") in site.calls
    resource = manifest.resources.get(last)
    assert resource is not None
    assert (resource.region, resource.kind) == (Region.SVWB, Kind.API)
    body = orjson.loads((tmp_path / resource.path).read_bytes())
    assert body["data"]["card_details"][str(site.ids[-1])]["common"]["name"].startswith(
        "en "
    )


async def test_a_list_that_grows_mid_run_stops_the_language(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    site = FakeWb()
    site.grow_after = 2
    crawler = make_crawler(manifest, tmp_path, clock, site)
    with pytest.raises(FetchError, match="changed"):
        await cards(crawler, "ja")


async def test_pages_repeating_cards_stop_the_language(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    site = FakeWb()
    site.repeat = True
    crawler = make_crawler(manifest, tmp_path, clock, site)
    with pytest.raises(FetchError, match="repeats a card"):
        await cards(crawler, "ja")


async def test_image_urls_need_every_stored_page(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    crawler = make_crawler(manifest, tmp_path, clock, FakeWb())
    assert stored_image_urls(crawler.writer) is None
    await cards(crawler, "ja")
    urls = stored_image_urls(crawler.writer)
    assert urls is not None
    # 505 cards, half with an evolved side, one in ten with a style.
    assert len(urls) == 505 + 253 + 51
    assert urls[0] == svwb.image_url(image_hash(10_000_000, "c"))


@pytest.mark.parametrize(
    ("data", "message"),
    [
        ({"count": True}, "API count True, expected at least 500"),
        ({"sort_card_id_list": [True]}, "API sort_card_id_list is not a list of ints"),
        ({"sort_card_id_list": 1}, "API sort_card_id_list is not a list of ints"),
        ({"card_details": []}, "API card_details is not an object"),
        ({"card_details": {}}, "API page has no details for [1]"),
        ({"sort_card_id_list": [1, 1]}, "API page lists 2 cards or repeats one"),
    ],
    ids=[
        "bool-total",
        "bool-id",
        "wrong-id-type",
        "details-type",
        "missing-details",
        "duplicate-id",
    ],
)
def test_list_guard_messages_are_precise(data: dict[str, object], message: str) -> None:
    with pytest.raises(ValidationError, match="^" + re.escape(message) + "$"):
        svwb.parse_list(page_body(**data))


def test_image_hashes_keep_style_evolution_and_remove_duplicates() -> None:
    card = FakeWb.details(10, "Synthetic digital card")
    card["style_card_list"] = [
        {"hash": image_hash(10, "c"), "evo_hash": image_hash(10, "z")},
        {"hash": image_hash(10, "s"), "evo_hash": ""},
    ]
    assert svwb.image_hashes(
        orjson.dumps({"data": {"card_details": {"10": card}}})
    ) == [
        image_hash(10, "c"),
        image_hash(10, "e"),
        image_hash(10, "z"),
        image_hash(10, "s"),
    ]


@pytest.mark.parametrize(
    ("value", "message"),
    [
        ("a" * 31, "unexpected image hash '" + "a" * 31 + "'"),
        ("g" * 32, "unexpected image hash '" + "g" * 32 + "'"),
        (None, "unexpected image hash None"),
    ],
    ids=["length", "hex", "type"],
)
def test_image_hash_validation_is_precise(value: object, message: str) -> None:
    card = FakeWb.details(1, "Synthetic digital card")
    card["common"] = {"card_image_hash": value}
    with pytest.raises(ValidationError, match="^" + re.escape(message) + "$"):
        svwb.image_hashes(orjson.dumps({"data": {"card_details": {"1": card}}}))


def test_image_style_list_is_required_even_when_empty() -> None:
    card = FakeWb.details(1, "Synthetic digital card")
    card["style_card_list"] = {}
    with pytest.raises(ValidationError, match=r"^API style_card_list is not a list$"):
        svwb.image_hashes(orjson.dumps({"data": {"card_details": {"1": card}}}))


def test_api_mapping_keys_do_not_make_a_list_an_object() -> None:
    with pytest.raises(ValidationError, match=r"^API response has no 'data_headers'$"):
        svwb.parse_list(orjson.dumps(["data_headers"]))
    with pytest.raises(ValidationError, match=r"^API response has no 'data'$"):
        svwb.image_hashes(orjson.dumps(["data"]))


def test_image_card_details_are_an_object() -> None:
    with pytest.raises(ValidationError, match=r"^API card_details is not an object$"):
        svwb.image_hashes(orjson.dumps({"data": {"card_details": []}}))

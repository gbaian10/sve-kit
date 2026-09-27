from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

import httpx
import orjson
import pytest

from sve_carddb.crawl import Crawler
from sve_carddb.crawl_svwb import SVWB_SITE, cards
from sve_carddb.fetch.client import Client, ClientPolicy, FetchError
from sve_carddb.fetch.throttle import CircuitBreaker, Throttle
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import Writer
from sve_carddb.manifest import Kind, Manifest, Region
from sve_carddb.sources import official_svwb as svwb

from .fakewb import FakeWb

if TYPE_CHECKING:
    from .conftest import FakeClock

FIXTURES = Path(__file__).parent / "fixtures" / "official_svwb"


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


def test_only_the_official_site_is_allowed_and_no_images() -> None:
    assert svwb.allowed(svwb.list_url("ja", 0))
    assert not svwb.allowed("http://shadowverse-wb.com/")
    assert not svwb.allowed("https://example.com/web/CardList/cardList")
    with pytest.raises(ValidationError):
        svwb.image_path("https://shadowverse-wb.com/x.png")


def test_parses_a_real_page() -> None:
    body = (FIXTURES / "cardlist_ja_offset30_trimmed.json").read_bytes()
    page = svwb.parse_list(body)
    assert page.count == 904
    assert page.card_ids == [10711310, 10712310]


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

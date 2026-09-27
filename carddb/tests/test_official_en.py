from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.crawl import (
    EN_CATALOG,
    EN_SITE,
    Crawler,
    ListSummary,
    current_sets,
    list_root,
    sets_root,
)
from sve_carddb.fetch.client import Client, ClientPolicy, FetchError
from sve_carddb.fetch.throttle import CircuitBreaker, Throttle
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import Writer
from sve_carddb.manifest import Kind, Manifest, Region
from sve_carddb.sources import official_en as en
from sve_carddb.sources import official_jp as jp

from .fakesite import IMG, FakeSite

if TYPE_CHECKING:
    from .conftest import FakeClock

FIXTURES = Path(__file__).parent / "fixtures" / "official_en"


def fixture(name: str) -> bytes:
    return (FIXTURES / name).read_bytes()


def make_crawler(
    manifest: Manifest, root: Path, clock: FakeClock, site: FakeSite
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
        site=EN_SITE,
        catalog=EN_CATALOG,
    )


# --- URLs and paths -------------------------------------------------------------


def test_urls() -> None:
    assert en.sets_url() == "https://en.shadowverse-evolve.com/cards/"
    assert en.list_url("BP19-BP20", 1) == (
        "https://en.shadowverse-evolve.com/cards/searchresults/"
        "?expansion=BP19-BP20&view=text"
    )
    assert en.list_url("BP01", 2) == (
        "https://en.shadowverse-evolve.com/cards/searchresults_ex"
        "?expansion=BP01&page=2&view=text"
    )
    assert en.card_url("BP18-SP01EN") == (
        "https://en.shadowverse-evolve.com/cards/?cardno=BP18-SP01EN"
    )


def test_paths_mirror_the_japanese_layout() -> None:
    assert en.sets_path() == PurePosixPath("raw/en/sets.html.zst")
    assert en.list_path("BP01", 3) == PurePosixPath("raw/en/list/BP01/3.html.zst")
    assert en.card_path("BP18-SP01EN") == PurePosixPath(
        "raw/en/card/BP18-SP01EN.html.zst"
    )
    image = "https://en.shadowverse-evolve.com/wordpress/wp-content/images/cardlist/BP01/BP01-001EN.png"
    assert en.image_path(image) == PurePosixPath("media/images/en/BP01/BP01-001EN.png")


@pytest.mark.parametrize(
    ("url", "expected"),
    [
        ("https://en.shadowverse-evolve.com/cards/", True),
        ("http://en.shadowverse-evolve.com/cards/", False),
        ("https://shadowverse-evolve.com/cardlist/", False),
        ("https://en.shadowverse-evolve.com.evil.example/", False),
    ],
)
def test_allowed(url: str, *, expected: bool) -> None:
    assert en.allowed(url) is expected


# --- real pages -------------------------------------------------------------------


def test_parse_sets() -> None:
    sets = jp.parse_sets(fixture("sets.html"))
    assert len(sets) == 50
    assert sets[0].code == "BP19-BP20"
    assert sets[-1] == jp.CardSet(code="PR", name="Promo Cards")


def test_parse_list_pages() -> None:
    first = jp.parse_list_first(fixture("list_BP01_1.html"))
    assert (first.total, first.max_page) == (273, 19)
    assert first.card_numbers[:2] == ["BP01-001EN", "BP01-002EN"]
    second = jp.parse_list_more(
        fixture("list_BP01_2.html"), page=2, max_page=19, total=273
    )
    assert second.card_numbers[0] == "BP01-016EN"
    assert len(second.card_numbers) == 15
    last = jp.parse_list_more(
        fixture("list_BP01_19.html"), page=19, max_page=19, total=273
    )
    assert last.card_numbers == ["BP01-U05EN", "BP01-U06EN", "BP01-U07EN"]


def test_card_page_resolves_images_on_the_english_site() -> None:
    card = en.parse_card(fixture("card_BP01-001EN.html"), expected_number="BP01-001EN")
    assert card.name == "Rose Queen"
    assert card.image_urls == [
        "https://en.shadowverse-evolve.com/wordpress/wp-content/images/cardlist/BP01/BP01-001EN.png"
    ]


def test_special_card_keeps_its_number() -> None:
    card = en.parse_card(
        fixture("card_BP18-SP01EN.html"), expected_number="BP18-SP01EN"
    )
    assert card.card_number == "BP18-SP01EN"
    assert card.image_originals[0].endswith("/BP18/BP18-SP01EN.png")


def test_card_page_must_be_the_requested_card() -> None:
    # A Japanese number without the suffix is a different card.
    with pytest.raises(ValidationError, match="asked for BP01-001"):
        en.parse_card(fixture("card_BP01-001EN.html"), expected_number="BP01-001")


# --- crawling ---------------------------------------------------------------------


async def test_p0_to_p2_store_under_the_english_region(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 17, "PR": 2}, english=True)
    crawler = make_crawler(manifest, tmp_path, clock, site)
    assert [s.code for s in await crawler.discover_sets()] == ["BP01", "PR"]
    assert await crawler.first_page("BP01") == ListSummary("BP01", 17, 2)
    assert await crawler.discover_list("BP01") == ListSummary("BP01", 17, 2)
    assert crawler.card_numbers(["BP01"]) == site.numbers("BP01")
    card = await crawler.card("BP01-001EN")
    assert card.image_urls == [
        f"https://en.shadowverse-evolve.com{IMG}/BP01/bp01-001en.png"
    ]
    assert all(
        c.startswith("https://en.shadowverse-evolve.com/cards/") for c in site.calls
    )
    resource = manifest.resources.get(en.card_url("BP01-001EN"))
    assert resource is not None
    assert (resource.region, resource.kind) == (Region.EN, Kind.CARD)
    assert resource.path == en.card_path("BP01-001EN")
    assert (tmp_path / en.list_path("BP01", 2)).is_file()


async def test_english_and_japanese_generations_are_separate(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    await make_crawler(
        manifest, tmp_path, clock, FakeSite({"BP01": 3}, english=True)
    ).discover_sets()
    assert current_sets(manifest, Region.EN) == ["BP01"]
    assert current_sets(manifest) == []
    assert manifest.generations.current(sets_root(Region.JP)) is None
    assert manifest.generations.current(list_root("BP01", Region.EN)) is None


async def test_a_japanese_page_on_the_english_site_is_rejected(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 3}, english=True)
    site.card_number_override["BP01-001EN"] = "BP01-001"
    crawler = make_crawler(manifest, tmp_path, clock, site)
    with pytest.raises(FetchError, match="asked for BP01-001EN"):
        await crawler.card("BP01-001EN")
    assert manifest.resources.get(en.card_url("BP01-001EN")) is None

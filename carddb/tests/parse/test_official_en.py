from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.core.regions import SourceRegion
from sve_carddb.ingest.archive.manifest import Kind, Manifest
from sve_carddb.ingest.crawl.crawl import EN_CATALOG, EN_SITE, Crawler, ListSummary
from sve_carddb.ingest.http.client import Client, ClientPolicy, FetchError
from sve_carddb.ingest.http.throttle import CircuitBreaker, Throttle
from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.ingest.http.writer import Writer
from sve_carddb.ingest.queries import current_sets, list_root, sets_root
from sve_carddb.parse.pages import official_en as en
from sve_carddb.parse.pages import official_jp as jp

from ..support.fakesite import IMG, FakeSite

if TYPE_CHECKING:
    from ..conftest import FakeClock

FIXTURES = Path(__file__).parents[1] / "fixtures" / "synthetic_en"


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


# --- synthetic pages -------------------------------------------------------------------


def test_parse_sets() -> None:
    sets = jp.parse_sets(fixture("F06-sets.html"))
    assert len(sets) == 50
    assert [item.code for item in sets] == [
        "SN19-SN20",
        "SN21-SN22",
        "SN23-SN24",
        *[f"SN{index:02}" for index in range(1, 28)],
        *[f"SYN{index:02}" for index in range(28, 42)],
        *[f"SYN{index:02}A" for index in range(42, 47)],
        "SN",
    ]
    assert sets[0] == jp.CardSet(code="SN19-SN20", name="SVE-KIT 合成F06段落002。")
    assert sets[3].code == "SN01"
    assert sets[30].code == "SYN28"
    assert sets[44].code == "SYN42A"
    assert sets[-1] == jp.CardSet(code="SN", name="SVE-KIT 合成F06段落051。")


def test_parse_list_pages() -> None:
    first = jp.parse_list_first(fixture("F03-list.html"))
    assert (first.total, first.max_page) == (273, 19)
    assert first.card_numbers == [f"SYN01-{index:03}EN" for index in range(1, 16)]
    second = jp.parse_list_more(
        fixture("F05-list.html"), page=2, max_page=19, total=273
    )
    assert second.card_numbers == [f"SYN01-{index:03}EN" for index in range(16, 31)]
    last = jp.parse_list_more(fixture("F04-list.html"), page=19, max_page=19, total=273)
    assert last.card_numbers == ["SN01-U31EN", "SN01-U32EN", "SN01-U33EN"]
    assert len(set(first.card_numbers + second.card_numbers + last.card_numbers)) == 33


def test_card_page_resolves_images_on_the_english_site() -> None:
    card = en.parse_card(fixture("F01-card.html"), expected_number="SYN01-001EN")
    assert card.card_number == "SYN01-001EN"
    assert card.name == "SVE-KIT 合成測試卡 F01-01"
    assert card.image_originals == [
        "/wordpress/wp-content/images/cardlist/synthetic/SYN01-001EN-1.png"
    ]
    assert card.image_urls == [
        "https://en.shadowverse-evolve.com/wordpress/wp-content/images/cardlist/synthetic/SYN01-001EN-1.png"
    ]


def test_special_card_keeps_its_number() -> None:
    card = en.parse_card(fixture("F02-card.html"), expected_number="SYN01-SP01EN")
    assert card.card_number == "SYN01-SP01EN"
    assert card.name == "SVE-KIT 合成測試卡 F02-01"
    assert card.image_originals == [
        "/wordpress/wp-content/images/cardlist/synthetic/SYN01-SP01EN-1.png"
    ]


def test_card_page_must_be_the_requested_card() -> None:
    # Suffix stripping would conflate distinct regional card numbers.
    with pytest.raises(
        ValidationError, match=r"^asked for SYN01-001, page shows SYN01-001EN$"
    ):
        en.parse_card(fixture("F01-card.html"), expected_number="SYN01-001")


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
    assert (resource.region, resource.kind) == (SourceRegion.EN, Kind.CARD)
    assert resource.path == en.card_path("BP01-001EN")
    assert (tmp_path / en.list_path("BP01", 2)).is_file()


async def test_english_and_japanese_generations_are_separate(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    await make_crawler(
        manifest, tmp_path, clock, FakeSite({"BP01": 3}, english=True)
    ).discover_sets()
    assert current_sets(manifest, SourceRegion.EN) == ["BP01"]
    assert current_sets(manifest) == []
    assert manifest.generations.current(sets_root(SourceRegion.JP)) is None
    assert manifest.generations.current(list_root("BP01", SourceRegion.EN)) is None


async def test_a_japanese_page_on_the_english_site_is_rejected(
    manifest: Manifest, tmp_path: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 3}, english=True)
    site.card_number_override["BP01-001EN"] = "BP01-001"
    crawler = make_crawler(manifest, tmp_path, clock, site)
    with pytest.raises(FetchError, match="asked for BP01-001EN"):
        await crawler.card("BP01-001EN")
    assert manifest.resources.get(en.card_url("BP01-001EN")) is None

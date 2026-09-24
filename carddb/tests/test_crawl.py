from typing import TYPE_CHECKING
from urllib.parse import parse_qs, urlsplit

import httpx
import pytest

from sve_carddb.crawl import (
    SETS_ROOT,
    Crawler,
    LimitReachedError,
    ListInconsistentError,
    ListSummary,
    Mode,
    list_root,
)
from sve_carddb.fetch.client import Client, ClientPolicy, FetchError
from sve_carddb.fetch.throttle import CircuitBreaker, Throttle
from sve_carddb.fetch.writer import LocalState, Writer
from sve_carddb.manifest import Manifest, Outcome
from sve_carddb.sources import official_jp as jp

if TYPE_CHECKING:
    from pathlib import Path

    from .conftest import FakeClock

PADDING = "<!--" + "x" * 1200 + "-->"
IMG = "/wordpress/wp-content/images/cardlist"


class FakeSite:
    """Serves pages shaped like the real site, from an editable product catalogue."""

    def __init__(self, sets: dict[str, int]) -> None:
        self.sets = sets
        self.calls: list[str] = []
        self.card_number_override: dict[str, str] = {}
        self.list_page_one_totals: list[int] = []

    def numbers(self, code: str) -> list[str]:
        return [f"{code}-{i:03d}" for i in range(1, self.sets[code] + 1)]

    def __call__(self, request: httpx.Request) -> httpx.Response:
        self.calls.append(str(request.url))
        parts = urlsplit(str(request.url))
        query = {k: v[0] for k, v in parse_qs(parts.query).items()}
        if parts.path == "/cardlist/" and "cardno" in query:
            return html(self.card_page(query["cardno"]))
        if parts.path == "/cardlist/":
            return html(self.sets_page())
        code = query["expansion_name"]
        if parts.path == "/cardlist/cardsearch/":
            return html(self.list_first(code))
        return html(self.list_more(code, int(query["page"])))

    def sets_page(self) -> str:
        options = "".join(f'<option value="{c}">{c} pack</option>' for c in self.sets)
        return page(
            f'<select name="expansion_name"><option value="">指定なし</option>{options}</select>'
        )

    def list_first(self, code: str) -> str:
        total = (
            self.list_page_one_totals.pop(0)
            if self.list_page_one_totals
            else self.sets[code]
        )
        max_page = -(-total // jp.PAGE_SIZE)
        items = self.items(code, 1)
        return page(
            f'<span class="num bold">{total}</span>'
            f'<ul class="cardlist-Result_List">{items}</ul>'
            f"<script>var max_page = {max_page};</script>"
        )

    def list_more(self, code: str, page_no: int) -> str:
        return self.items(code, page_no, css="ex-item")

    def items(self, code: str, page_no: int, css: str = "") -> str:
        chunk = self.numbers(code)[
            (page_no - 1) * jp.PAGE_SIZE : page_no * jp.PAGE_SIZE
        ]
        return "".join(
            f'<li class="{css}"><p class="number">{n}</p></li>' for n in chunk
        )

    def card_page(self, number: str) -> str:
        shown = self.card_number_override.get(number, number)
        code = number.split("-", maxsplit=1)[0]
        return page(
            '<div class="cardlist-Detail">'
            f'<div class="img"><img src="{IMG}/{code}/{number.lower()}.png"></div>'
            f'<p class="ttl">Card {number}</p>'
            f'<div class="illustrator"><span class="heading">{shown}</span></div>'
            "</div>"
        )


def page(body: str) -> str:
    return f"<!DOCTYPE html><html><body>{body}{PADDING}</body></html>"


def html(body: str) -> httpx.Response:
    return httpx.Response(
        200, text=body, headers={"content-type": "text/html; charset=UTF-8"}
    )


def make_crawler(
    manifest: Manifest,
    root: Path,
    clock: FakeClock,
    site: FakeSite,
    **options: Mode | int,
) -> Crawler:
    http = httpx.AsyncClient(transport=httpx.MockTransport(site))
    throttle = Throttle(0.0, 0.0, clock=clock, sleep=clock.sleep)
    policy = ClientPolicy(wait_initial=0.0, wait_max=0.0, wait_jitter=0.0)
    client = Client(http, throttle, manifest, run_id="test", policy=policy, clock=clock)
    mode = options.get("mode", Mode.RESUME)
    limit = options.get("limit")
    assert isinstance(mode, Mode)
    assert limit is None or isinstance(limit, int)
    return Crawler(
        client=client,
        writer=Writer(root, manifest),
        manifest=manifest,
        breaker=CircuitBreaker(5),
        mode=mode,
        limit=limit,
    )


@pytest.fixture
def root(tmp_path: Path) -> Path:
    return tmp_path / "data"


async def test_p0_discovers_products_and_first_pages(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 17, "BP02": 3})
    crawler = make_crawler(manifest, root, clock, site)
    sets = await crawler.discover_sets()
    assert [s.code for s in sets] == ["BP01", "BP02"]
    assert crawler.current_sets() == ["BP01", "BP02"]
    assert await crawler.first_page("BP01") == ListSummary("BP01", 17, 2)
    assert await crawler.first_page("BP02") == ListSummary("BP02", 3, 1)


async def test_p1_validates_a_whole_list_as_one_generation(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 17})
    crawler = make_crawler(manifest, root, clock, site)
    await crawler.discover_sets()
    assert await crawler.discover_list("BP01") == ListSummary("BP01", 17, 2)
    assert crawler.card_numbers() == site.numbers("BP01")
    # page 1, page 2, and the recheck of page 1
    list_calls = [c for c in site.calls if "cardsearch" in c]
    assert len(list_calls) == 3


async def test_p1_list_that_keeps_changing_is_never_published(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 17})
    site.list_page_one_totals = [17, 18, 17, 18]
    crawler = make_crawler(manifest, root, clock, site)
    await crawler.discover_sets()
    with pytest.raises(ListInconsistentError):
        await crawler.discover_list("BP01")
    assert manifest.generations.current(list_root("BP01")) is None
    assert crawler.card_numbers() == []


async def test_p1_shrunk_list_no_longer_schedules_old_pages(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 31})
    crawler = make_crawler(manifest, root, clock, site)
    await crawler.discover_sets()
    await crawler.discover_list("BP01")
    assert len(crawler.card_numbers()) == 31
    site.sets["BP01"] = 17
    await crawler.discover_list("BP01")
    assert crawler.card_numbers() == site.numbers("BP01")


async def test_p1_interrupted_refresh_keeps_the_old_generation(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 17})
    crawler = make_crawler(manifest, root, clock, site)
    await crawler.discover_sets()
    await crawler.discover_list("BP01")
    before = manifest.generations.current(list_root("BP01"))
    limited = make_crawler(manifest, root, clock, site, limit=1)
    with pytest.raises(LimitReachedError):
        await limited.discover_list("BP01")
    assert manifest.generations.current(list_root("BP01")) == before
    assert crawler.card_numbers() == site.numbers("BP01")


async def test_p2_fetches_cards_and_records_images(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 2})
    crawler = make_crawler(manifest, root, clock, site)
    await crawler.discover_sets()
    await crawler.discover_list("BP01")
    card = await crawler.card("BP01-001")
    assert card.image_urls == [f"https://shadowverse-evolve.com{IMG}/BP01/bp01-001.png"]
    assert [
        link.original for link in manifest.links.current(jp.card_url("BP01-001"))
    ] == [f"{IMG}/BP01/bp01-001.png"]


async def test_resume_skips_trusted_cards_without_a_request(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 1})
    crawler = make_crawler(manifest, root, clock, site)
    await crawler.card("BP01-001")
    calls = len(site.calls)
    await make_crawler(manifest, root, clock, site).card("BP01-001")
    assert len(site.calls) == calls


async def test_refresh_rechecks_trusted_cards(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 1})
    await make_crawler(manifest, root, clock, site).card("BP01-001")
    calls = len(site.calls)
    await make_crawler(manifest, root, clock, site, mode=Mode.REFRESH).card("BP01-001")
    assert len(site.calls) == calls + 1
    assert manifest.requests.outcomes(jp.card_url("BP01-001"))[-1] is Outcome.UNCHANGED


async def test_repair_fetches_only_damaged_copies(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 2})
    first = make_crawler(manifest, root, clock, site)
    await first.card("BP01-001")
    await first.card("BP01-002")
    (root / jp.card_path("BP01-002")).write_bytes(b"damaged")
    calls = len(site.calls)
    repair = make_crawler(manifest, root, clock, site, mode=Mode.REPAIR)
    await repair.card("BP01-001")
    await repair.card("BP01-002")
    assert len(site.calls) == calls + 1
    assert repair.writer.local_state(jp.card_url("BP01-002")) is LocalState.TRUSTED


async def test_wrong_card_page_is_rejected_and_not_stored(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP02": 71})
    site.card_number_override["BP02-070"] = "BP02-071"
    crawler = make_crawler(manifest, root, clock, site)
    with pytest.raises(FetchError, match="asked for BP02-070"):
        await crawler.card("BP02-070")
    url = jp.card_url("BP02-070")
    assert manifest.resources.get(url) is None
    assert manifest.requests.outcomes(url) == [Outcome.FAILED]


async def test_limit_counts_fetched_urls(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 3})
    crawler = make_crawler(manifest, root, clock, site, limit=2)
    await crawler.card("BP01-001")
    await crawler.card("BP01-002")
    with pytest.raises(LimitReachedError):
        await crawler.card("BP01-003")


async def test_sets_discovery_always_refetches(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    site = FakeSite({"BP01": 1})
    await make_crawler(manifest, root, clock, site).discover_sets()
    site.sets["BP02"] = 1
    crawler = make_crawler(manifest, root, clock, site)
    await crawler.discover_sets()
    assert crawler.current_sets() == ["BP01", "BP02"]
    assert manifest.generations.current(SETS_ROOT) is not None

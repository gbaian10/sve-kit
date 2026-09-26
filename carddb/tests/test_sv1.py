from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

import httpx
import orjson
import pytest

from sve_carddb.crawl import Crawler, LimitReachedError
from sve_carddb.crawl_sv1 import (
    SV1_SITE,
    ImageResult,
    Sv1Crawler,
    image_jobs,
    stored_cards,
)
from sve_carddb.fetch.client import Client, ClientPolicy, FetchError
from sve_carddb.fetch.throttle import CircuitBreaker, CircuitOpenError, Throttle
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import LocalState, Writer
from sve_carddb.html import MissingElementError
from sve_carddb.manifest import Kind, Link, Manifest, Outcome, Region
from sve_carddb.sources import official_sv1 as sv1

from .fakeportal import ETAG, FakePortal, card_id

if TYPE_CHECKING:
    from .conftest import FakeClock

FIXTURES = Path(__file__).parent / "fixtures" / "official_sv1"
FOLLOWER = card_id(0)
SPELL = card_id(1)


def make_sv1(
    manifest: Manifest,
    root: Path,
    clock: FakeClock,
    portal: FakePortal,
    *,
    limit: int | None = None,
) -> Sv1Crawler:
    http = httpx.AsyncClient(transport=httpx.MockTransport(portal))
    throttle = Throttle(0.0, 0.0, clock=clock, sleep=clock.sleep)
    policy = ClientPolicy(wait_initial=0.0, wait_max=0.0, wait_jitter=0.0)
    client = Client(http, throttle, manifest, run_id="test", policy=policy, clock=clock)
    crawler = Crawler(
        client=client,
        writer=Writer(root, manifest),
        manifest=manifest,
        breaker=CircuitBreaker(5),
        limit=limit,
        site=SV1_SITE,
    )
    return Sv1Crawler(crawler, CircuitBreaker(3))


@pytest.fixture
def root(tmp_path: Path) -> Path:
    return tmp_path / "data"


# --- URLs and parsing ---------------------------------------------------------


def test_urls_follow_the_official_patterns() -> None:
    assert (
        sv1.api_url("zh-tw")
        == "https://shadowverse-portal.com/api/v1/cards?format=json&lang=zh-tw"
    )
    assert (
        sv1.card_url(100011010)
        == "https://shadowverse-portal.com/card/100011010?lang=ja"
    )
    assert (
        sv1.image_url(100011010, sv1.Face.EVOLVED)
        == "https://shadowverse-portal.com/image/card/phase2/common/E/E_100011010.png"
    )


def test_template_matches_the_real_card_page() -> None:
    # The template is an exception to reading `<img src>`; it must keep matching the site.
    images = sv1.parse_card_images((FIXTURES / "card_100011010.html").read_bytes())
    assert images.urls == [sv1.image_url(100011010, face) for face in sv1.FACES]
    assert images.originals[0].endswith("C_100011010.png?202609261156")


def test_the_real_error_page_has_no_images() -> None:
    body = (FIXTURES / "card_930844060_error.html").read_bytes()
    assert sv1.parse_card_images(body).urls == []


def test_a_page_that_is_neither_card_nor_error_is_rejected() -> None:
    body = f"<html><body><h1>Maintenance</h1>{'x' * 2000}</body></html>".encode()
    with pytest.raises(MissingElementError):
        sv1.parse_card_images(body)


def test_image_path_keeps_the_official_path() -> None:
    url = sv1.image_url(100011010, sv1.Face.BASE)
    assert sv1.image_path(url) == PurePosixPath(
        "media/images/sv1/image/card/phase2/common/C/C_100011010.png"
    )
    with pytest.raises(ValidationError):
        sv1.image_path("https://shadowverse-portal.com/public/logo.png")


def test_resolve_image_drops_the_cache_buster() -> None:
    assert sv1.resolve_image("/image/card/phase2/common/C/C_1.png?2026") == (
        "https://shadowverse-portal.com/image/card/phase2/common/C/C_1.png"
    )


def test_api_cards_and_faces() -> None:
    body = FakePortal().api(httpx.Request("GET", sv1.api_url("ja"))).content
    cards = sv1.parse_cards(body)
    assert len(cards) == sv1.MIN_CARDS
    assert cards[0].faces == [sv1.Face.BASE, sv1.Face.EVOLVED]
    assert cards[1].faces == [sv1.Face.BASE]
    assert (cards[0].name, cards[1].name) == ("Card 0", None)
    # By card_id, not API order: the API lists tokens (high ids) first.
    assert image_jobs([cards[1], cards[0]]) == [
        (FOLLOWER, sv1.Face.BASE),
        (FOLLOWER, sv1.Face.EVOLVED),
        (SPELL, sv1.Face.BASE),
    ]


def api_body(
    cards: list[dict[str, object]], errors: list[object] | None = None
) -> bytes:
    return orjson.dumps({"data": {"cards": cards, "errors": errors or []}})


@pytest.mark.parametrize(
    ("body", "reason"),
    [
        (b"<html>maintenance</html>", "not JSON"),
        (api_body(FakePortal().cards, ["down"]), "reports errors"),
        (api_body(FakePortal(10).cards), "expected at least"),
        (api_body(FakePortal().cards * 2), "twice"),
        (api_body([{"card_id": 12, "card_name": "x", "char_type": 1}]), "card_id"),
        (api_body([{"card_id": True, "card_name": "x", "char_type": 1}]), "card_id"),
        (api_body([{"card_name": "x"}]), "card_id"),
        (orjson.dumps({"cards": []}), "'data'"),
    ],
    ids=[
        "html",
        "errors",
        "too-few",
        "duplicate",
        "short-id",
        "bool-id",
        "no-id",
        "no-data",
    ],
)
def test_bad_api_responses_are_rejected(body: bytes, reason: str) -> None:
    with pytest.raises(ValidationError, match=reason):
        sv1.parse_cards(body)


# --- sv1-cards ------------------------------------------------------------------


async def test_api_is_stored_as_json_and_recorded(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    crawler = make_sv1(manifest, root, clock, portal)
    assert await crawler.cards("zh-tw") == sv1.MIN_CARDS
    url = sv1.api_url("zh-tw")
    resource = manifest.resources.get(url)
    assert resource is not None
    assert resource.path == PurePosixPath("raw/sv1/api/cards-zh-tw.json")
    assert (resource.region, resource.kind, resource.etag) == (
        Region.SV1,
        Kind.API,
        ETAG,
    )
    stored = (root / resource.path).read_bytes()
    assert orjson.loads(stored)["data"]["cards"][0]["card_id"] == FOLLOWER


async def test_api_is_rechecked_with_its_etag(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    await make_sv1(manifest, root, clock, portal).cards("ja")
    await make_sv1(manifest, root, clock, portal).cards("ja")
    assert portal.conditional == [None, ETAG]
    assert manifest.requests.outcomes(sv1.api_url("ja")) == [
        Outcome.CHANGED,
        Outcome.NOT_MODIFIED,
    ]


# --- sv1-images -------------------------------------------------------------------


async def test_images_come_from_the_template(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    crawler = make_sv1(manifest, root, clock, portal)
    for face in sv1.FACES:
        assert await crawler.image(FOLLOWER, face) is ImageResult.FETCHED
    url = sv1.image_url(FOLLOWER, sv1.Face.EVOLVED)
    assert portal.calls == [sv1.image_url(FOLLOWER, face) for face in sv1.FACES]
    resource = manifest.resources.get(url)
    assert resource is not None
    assert (resource.region, resource.kind) == (Region.SV1, Kind.IMAGE)
    assert resource.path == PurePosixPath(
        f"media/images/sv1/image/card/phase2/common/E/E_{FOLLOWER}.png"
    )
    assert crawler.crawler.writer.local_state(url) is LocalState.TRUSTED


async def test_stored_images_are_skipped(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    await make_sv1(manifest, root, clock, portal).image(SPELL, sv1.Face.BASE)
    calls = len(portal.calls)
    again = await make_sv1(manifest, root, clock, portal).image(SPELL, sv1.Face.BASE)
    assert again is ImageResult.STORED
    assert len(portal.calls) == calls


async def test_missing_template_falls_back_to_the_card_page(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    template = sv1.image_url(SPELL, sv1.Face.BASE)
    actual = f"{sv1.BASE}/image/card/phase2/common/C/C_{SPELL}_v2.png"
    portal.missing.add(f"/image/card/phase2/common/C/C_{SPELL}.png")
    portal.page_images[SPELL] = [f"{actual}?123"]
    crawler = make_sv1(manifest, root, clock, portal)

    assert await crawler.image(SPELL, sv1.Face.BASE) is ImageResult.FETCHED

    # The site redirects a missing image to an HTML page instead of a 404.
    assert portal.calls == [
        template,
        f"{template}?lang=ja",
        sv1.card_url(SPELL),
        actual,
    ]
    assert manifest.requests.outcomes(template) == [Outcome.REDIRECTED, Outcome.FAILED]
    assert manifest.resources.get(template) is None
    assert manifest.links.current(sv1.card_url(SPELL)) == [
        Link(actual, Kind.IMAGE, 0, f"{actual}?123")
    ]
    resource = manifest.resources.get(actual)
    assert resource is not None
    assert resource.path.name == f"C_{SPELL}_v2.png"
    card_page = manifest.resources.get(sv1.card_url(SPELL))
    assert card_page is not None
    assert card_page.path == PurePosixPath(f"raw/sv1/card/{SPELL}.html.zst")

    calls = len(portal.calls)
    again = await make_sv1(manifest, root, clock, portal).image(SPELL, sv1.Face.BASE)
    assert again is ImageResult.STORED
    assert len(portal.calls) == calls


async def test_card_page_showing_a_stored_image_is_shared_not_a_miss(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    shared = sv1.image_url(FOLLOWER, sv1.Face.BASE)
    reprints = [card_id(i) for i in range(1, 9, 2)]
    for reprint in reprints:
        portal.missing.add(f"/image/card/phase2/common/C/C_{reprint}.png")
        portal.page_images[reprint] = [shared]
    crawler = make_sv1(manifest, root, clock, portal)
    assert await crawler.image(FOLLOWER, sv1.Face.BASE) is ImageResult.FETCHED
    fetched = portal.calls.count(shared)

    # make_sv1 trips `misses` after 3 in a row; four reprints must not trip it.
    for reprint in reprints:
        assert await crawler.image(reprint, sv1.Face.BASE) is ImageResult.SHARED
    assert portal.calls.count(shared) == fetched
    assert await crawler.image(reprints[0], sv1.Face.BASE) is ImageResult.STORED


async def test_card_page_showing_a_later_cards_template_is_shared(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    later = sv1.image_url(FOLLOWER, sv1.Face.BASE)
    reprints = [card_id(i) for i in range(1, 9, 2)]
    for reprint in reprints:
        portal.missing.add(f"/image/card/phase2/common/C/C_{reprint}.png")
        portal.page_images[reprint] = [later]
    crawler = make_sv1(manifest, root, clock, portal)

    # The first reprint fetches the art before its own card; none may trip `misses`.
    for reprint in reprints:
        assert await crawler.image(reprint, sv1.Face.BASE) is ImageResult.SHARED
    assert portal.calls.count(later) == 1
    assert await crawler.image(FOLLOWER, sv1.Face.BASE) is ImageResult.STORED


async def test_card_page_showing_the_other_faces_template_is_not_a_miss(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    cards = [card_id(i) for i in range(1, 9, 2)]
    for card in cards:
        portal.missing.add(f"/image/card/phase2/common/C/C_{card}.png")
        portal.page_images[card] = [sv1.image_url(card, sv1.Face.EVOLVED)]
    crawler = make_sv1(manifest, root, clock, portal)

    # make_sv1 trips `misses` after 3 in a row; a base face pointing at the evolved
    # template (910xxxxxx cards) is not a changed template.
    for card in cards:
        assert await crawler.image(card, sv1.Face.BASE) is ImageResult.SHARED


async def test_card_page_linking_a_missing_template_is_no_image(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    cards = [card_id(i) for i in range(1, 9, 2)]
    for card in cards:
        portal.missing.add(f"/image/card/phase2/common/C/C_{card}.png")
        portal.missing.add(f"/image/card/phase2/common/E/E_{card}.png")
        portal.page_images[card] = [sv1.image_url(card, sv1.Face.EVOLVED)]
    crawler = make_sv1(manifest, root, clock, portal)

    for card in cards:
        assert await crawler.image(card, sv1.Face.BASE) is ImageResult.NO_IMAGE


def test_is_template_matches_any_card_but_not_other_shapes() -> None:
    assert sv1.is_template(sv1.image_url(900344080, sv1.Face.BASE), sv1.Face.BASE)
    assert not sv1.is_template(
        sv1.image_url(900344080, sv1.Face.BASE), sv1.Face.EVOLVED
    )
    v2 = f"{sv1.BASE}/image/card/phase2/common/C/C_{SPELL}_v2.png"
    assert not sv1.is_template(v2, sv1.Face.BASE)


async def test_a_404_template_also_falls_back(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    actual = f"{sv1.BASE}/image/card/phase2/common/C/C_{SPELL}_v2.png"
    portal.gone.add(f"/image/card/phase2/common/C/C_{SPELL}.png")
    portal.page_images[SPELL] = [actual]
    crawler = make_sv1(manifest, root, clock, portal)
    assert await crawler.image(SPELL, sv1.Face.BASE) is ImageResult.FETCHED
    assert portal.calls[-1] == actual


async def test_fallback_to_the_same_url_fails(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    portal.missing.add(f"/image/card/phase2/common/C/C_{SPELL}.png")
    crawler = make_sv1(manifest, root, clock, portal)
    with pytest.raises(FetchError, match="yet the card page links it"):
        await crawler.image(SPELL, sv1.Face.BASE)


async def test_card_page_without_the_face_means_no_image(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    portal.missing.add(f"/image/card/phase2/common/E/E_{FOLLOWER}.png")
    portal.page_images[FOLLOWER] = [sv1.image_url(FOLLOWER, sv1.Face.BASE)]
    crawler = make_sv1(manifest, root, clock, portal)
    result = await crawler.image(FOLLOWER, sv1.Face.EVOLVED)
    assert result is ImageResult.NO_IMAGE


async def test_card_without_a_page_has_no_image_and_is_rechecked_cheaply(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    template = sv1.image_url(SPELL, sv1.Face.BASE)
    portal.missing.add(f"/image/card/phase2/common/C/C_{SPELL}.png")
    portal.no_page.add(SPELL)
    crawler = make_sv1(manifest, root, clock, portal)
    assert await crawler.image(SPELL, sv1.Face.BASE) is ImageResult.NO_IMAGE
    assert manifest.resources.get(sv1.card_url(SPELL)) is not None
    assert manifest.links.current(sv1.card_url(SPELL)) == []

    portal.calls.clear()
    again = await make_sv1(manifest, root, clock, portal).image(SPELL, sv1.Face.BASE)
    assert again is ImageResult.NO_IMAGE
    assert portal.calls == [template, f"{template}?lang=ja"]


async def test_cards_without_images_do_not_stop_the_run(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    ids = [card_id(i) for i in range(1, 21, 2)]
    for number in ids:
        portal.missing.add(f"/image/card/phase2/common/C/C_{number}.png")
        portal.no_page.add(number)
    crawler = make_sv1(manifest, root, clock, portal)
    for number in ids:
        assert await crawler.image(number, sv1.Face.BASE) is ImageResult.NO_IMAGE


async def test_repeated_template_misses_stop_the_run(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    ids = [card_id(i) for i in range(1, 7, 2)]
    for number in ids:
        portal.missing.add(f"/image/card/phase2/common/C/C_{number}.png")
        portal.page_images[number] = [f"{sv1.BASE}/image/card/x/C_{number}.png"]
    crawler = make_sv1(manifest, root, clock, portal)
    await crawler.image(ids[0], sv1.Face.BASE)
    await crawler.image(ids[1], sv1.Face.BASE)
    with pytest.raises(CircuitOpenError):
        await crawler.image(ids[2], sv1.Face.BASE)


async def test_limit_counts_image_urls(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    portal = FakePortal()
    crawler = make_sv1(manifest, root, clock, portal, limit=1)
    await crawler.image(FOLLOWER, sv1.Face.BASE)
    with pytest.raises(LimitReachedError):
        await crawler.image(FOLLOWER, sv1.Face.EVOLVED)


async def test_stored_cards_reads_the_japanese_api(
    manifest: Manifest, root: Path, clock: FakeClock
) -> None:
    crawler = make_sv1(manifest, root, clock, FakePortal())
    assert stored_cards(crawler.crawler.writer) is None
    await crawler.cards("ja")
    cards = stored_cards(crawler.crawler.writer)
    assert cards is not None
    assert cards[0].card_id == FOLLOWER

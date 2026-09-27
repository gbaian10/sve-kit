from functools import partial
from typing import TYPE_CHECKING

import httpx
import orjson
import pytest
import stamina
from typer.testing import CliRunner

from sve_carddb import cli
from sve_carddb.config import Settings
from sve_carddb.fetch.throttle import Throttle
from sve_carddb.manifest import ExclusiveLock, Manifest
from sve_carddb.sources import official_jp as jp
from sve_carddb.sources import official_sv1 as sv1
from sve_carddb.sources import official_svwb as svwb

from .conftest import FakeClock
from .fakeportal import FakePortal, card_id
from .fakesite import IMG, FakeSite
from .fakewb import FakeWb

if TYPE_CHECKING:
    from collections.abc import Iterator
    from pathlib import Path

    from typer.testing import Result

runner = CliRunner()
pytestmark = pytest.mark.usefixtures("no_retry_waits")


@pytest.fixture
def data_dir(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    root = tmp_path / "data"
    monkeypatch.setenv("SVE_DATA_DIR", str(root))
    return root


@pytest.fixture
def clock(monkeypatch: pytest.MonkeyPatch) -> FakeClock:
    fake = FakeClock()
    monkeypatch.setattr(
        cli, "Throttle", partial(Throttle, clock=fake, sleep=fake.sleep)
    )
    return fake


@pytest.fixture
def site(monkeypatch: pytest.MonkeyPatch, data_dir: Path, clock: FakeClock) -> FakeSite:
    del data_dir, clock  # requested for their side effects
    fake = FakeSite({"BP01": 17, "BP02": 3})

    def factory(settings: Settings) -> httpx.AsyncClient:
        return httpx.AsyncClient(
            transport=httpx.MockTransport(fake),
            headers={"User-Agent": settings.user_agent},
        )

    monkeypatch.setattr(cli, "http_factory", factory)
    return fake


@pytest.fixture
def no_retry_waits() -> Iterator[None]:
    stamina.set_testing(testing=True)
    yield
    stamina.set_testing(testing=False)


def invoke(*args: str) -> Result:
    return runner.invoke(cli.app, list(args))


def manifest_at(data_dir: Path) -> Manifest:
    return Manifest.open(data_dir / "manifest" / "manifest.sqlite")


@pytest.mark.usefixtures("site")
def test_full_pipeline(data_dir: Path) -> None:
    p0 = invoke("crawl", "p0")
    assert p0.exit_code == 0, p0.output
    assert "2 products" in p0.output
    p1 = invoke("crawl", "p1")
    assert p1.exit_code == 0, p1.output
    assert "BP01: 17 cards on 2 pages" in p1.output
    p2 = invoke("crawl", "p2")
    assert p2.exit_code == 0, p2.output
    with manifest_at(data_dir) as manifest:
        assert manifest.resources.get(jp.card_url("BP02-003")) is not None


def test_p0_stopped_early_still_reports_what_it_read(site: FakeSite) -> None:
    result = invoke("crawl", "p0", "--max-requests", "2")
    assert result.exit_code == 0, result.output
    assert "BP01:   17 cards,   2 pages" in result.output
    assert len(site.calls) == 2


def test_extract_cards_after_crawl(site: FakeSite, data_dir: Path) -> None:
    for stage in ("p0", "p1", "p2"):
        assert invoke("crawl", stage).exit_code == 0
    result = invoke("extract", "cards")
    assert result.exit_code == 0, result.output
    lines = (data_dir / "derived" / "jp" / "cards.jsonl").read_bytes().splitlines()
    assert len(lines) == sum(site.sets.values())
    first = orjson.loads(lines[0])
    assert first["number"] == "BP01-001"
    assert first["faces"][0]["card_type"] == "フォロワー"


@pytest.mark.usefixtures("site")
def test_requests_are_spaced_by_the_configured_interval(clock: FakeClock) -> None:
    assert invoke("crawl", "p0").exit_code == 0
    assert clock.sleeps
    assert min(clock.sleeps) >= Settings.model_fields["interval"].default


def test_resume_sends_nothing_for_stored_cards(site: FakeSite) -> None:
    for stage in ("p0", "p1", "p2"):
        assert invoke("crawl", stage).exit_code == 0
    calls = len(site.calls)
    result = invoke("crawl", "p2")
    assert result.exit_code == 0
    assert len(site.calls) == calls
    assert "HTTP requests sent: 0" in result.output


def test_dry_run_sends_nothing(site: FakeSite) -> None:
    assert invoke("crawl", "p0").exit_code == 0
    calls = len(site.calls)
    result = invoke("crawl", "p1", "--dry-run")
    assert result.exit_code == 0, result.output
    assert len(site.calls) == calls
    assert jp.list_url("BP01", 2) in result.output
    assert "3 of 3 URLs would be requested" in result.output


@pytest.mark.usefixtures("site")
def test_dry_run_p2_lists_only_missing_cards() -> None:
    for stage in ("p0", "p1"):
        assert invoke("crawl", stage).exit_code == 0
    assert invoke("crawl", "p2", "--set", "BP02", "--limit", "1").exit_code == 0
    result = invoke("crawl", "p2", "--dry-run")
    assert "19 of 20 URLs would be requested" in result.output
    assert jp.card_url("BP02-001") not in result.output


def test_dry_run_p0_skips_stored_first_pages(site: FakeSite) -> None:
    assert invoke("crawl", "p0", "--max-requests", "2").exit_code == 0
    calls = len(site.calls)
    result = invoke("crawl", "p0", "--dry-run")
    assert len(site.calls) == calls
    # The product page is always re-read; BP01 page 1 is stored, BP02 is not.
    assert "2 of 3 URLs would be requested" in result.output
    assert jp.list_url("BP01", 1) not in result.output


def test_refresh_needs_a_scope(site: FakeSite) -> None:
    result = invoke("crawl", "p2", "--mode", "refresh")
    assert result.exit_code == 2
    assert site.calls == []


def test_missing_data_dir_is_a_configuration_error(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SVE_DATA_DIR", raising=False)
    result = invoke("crawl", "p0")
    assert result.exit_code == 2
    assert "SVE_DATA_DIR" in result.output


def test_limit_stops_cleanly(site: FakeSite) -> None:
    result = invoke("crawl", "p0", "--limit", "1")
    assert result.exit_code == 0
    assert "--limit of 1 URLs reached" in result.output
    assert len(site.calls) == 1


def test_request_budget_stops_cleanly(site: FakeSite) -> None:
    result = invoke("crawl", "p0", "--max-requests", "2")
    assert result.exit_code == 0, result.output
    assert len(site.calls) == 2


def test_second_crawler_is_refused(site: FakeSite, data_dir: Path) -> None:
    with ExclusiveLock(data_dir / "manifest" / ".lock"):
        result = invoke("crawl", "p0")
    assert result.exit_code == 1
    assert "another crawler" in result.output
    assert site.calls == []


def test_p2_before_p1_says_what_to_run(site: FakeSite) -> None:
    result = invoke("crawl", "p2")
    assert result.exit_code == 1
    assert "crawl p1" in result.output
    assert site.calls == []


def test_one_bad_card_does_not_stop_the_others(site: FakeSite, data_dir: Path) -> None:
    for stage in ("p0", "p1"):
        assert invoke("crawl", stage).exit_code == 0
    site.card_number_override["BP02-001"] = "BP02-002"
    result = invoke("crawl", "p2", "--set", "BP02")
    assert result.exit_code == 1
    assert "1 failed" in result.output
    with manifest_at(data_dir) as manifest:
        assert manifest.resources.get(jp.card_url("BP02-001")) is None
        assert manifest.resources.get(jp.card_url("BP02-003")) is not None


@pytest.mark.usefixtures("site")
def test_leftovers_of_a_crash_are_cleaned_up(data_dir: Path) -> None:
    leftover = data_dir / "raw" / "jp" / ".tmp-crashed"
    leftover.parent.mkdir(parents=True)
    leftover.write_bytes(b"partial")
    result = invoke("crawl", "p0", "--limit", "1")
    assert "1 temp files removed" in result.output
    assert not leftover.exists()


@pytest.mark.usefixtures("site")
def test_manifest_check_reports_damage(data_dir: Path) -> None:
    assert invoke("crawl", "p0").exit_code == 0
    clean = invoke("manifest", "check")
    assert clean.exit_code == 0, clean.output
    (data_dir / jp.sets_path()).write_bytes(b"damaged")
    damaged = invoke("manifest", "check")
    assert damaged.exit_code == 1
    assert f"damaged: {jp.sets_url()}" in damaged.output


@pytest.mark.usefixtures("site")
def test_manifest_backup(tmp_path: Path) -> None:
    assert invoke("crawl", "p0").exit_code == 0
    dest = tmp_path / "backup" / "manifest.sqlite"
    result = invoke("manifest", "backup", str(dest))
    assert result.exit_code == 0, result.output
    with Manifest.open(dest) as copy:
        assert copy.resources.get(jp.sets_url()) is not None


def test_p5_fetches_each_card_image_once(site: FakeSite, data_dir: Path) -> None:
    for stage in ("p0", "p1", "p2"):
        assert invoke("crawl", stage).exit_code == 0
    result = invoke("crawl", "p5")
    assert result.exit_code == 0, result.output
    images = [c for c in site.calls if c.endswith(".png")]
    assert len(images) == len(set(images)) == sum(site.sets.values())
    stored = data_dir / jp.image_path(images[0])
    assert stored.read_bytes().startswith(b"\x89PNG")
    again = invoke("crawl", "p5")
    assert "HTTP requests sent: 0" in again.output


def test_p5_records_a_broken_image_and_goes_on(site: FakeSite, data_dir: Path) -> None:
    for stage in ("p0", "p1", "p2"):
        assert invoke("crawl", stage).exit_code == 0
    broken = f"{IMG}/BP02/bp02-001.png"
    site.broken_images.add(broken)
    result = invoke("crawl", "p5", "--set", "BP02")
    assert result.exit_code == 1
    assert "1 failed" in result.output
    with manifest_at(data_dir) as manifest:
        assert manifest.resources.get(jp.image_url(broken)) is None
        assert (
            manifest.resources.get(jp.image_url(f"{IMG}/BP02/bp02-002.png")) is not None
        )


@pytest.mark.usefixtures("site")
def test_p5_before_p2_says_what_to_run() -> None:
    assert invoke("crawl", "p0").exit_code == 0
    result = invoke("crawl", "p5")
    assert result.exit_code == 1
    assert "run `crawl p2` first" in result.output


def test_p5_dry_run_lists_missing_images(site: FakeSite) -> None:
    for stage in ("p0", "p1", "p2"):
        assert invoke("crawl", stage).exit_code == 0
    calls = len(site.calls)
    result = invoke("crawl", "p5", "--dry-run", "--set", "BP02")
    assert result.exit_code == 0, result.output
    assert len(site.calls) == calls
    assert "3 of 3 URLs would be requested" in result.output


@pytest.fixture
def portal(
    monkeypatch: pytest.MonkeyPatch, data_dir: Path, clock: FakeClock
) -> FakePortal:
    del data_dir, clock  # requested for their side effects
    fake = FakePortal()

    def factory(settings: Settings) -> httpx.AsyncClient:
        return httpx.AsyncClient(
            transport=httpx.MockTransport(fake),
            headers={"User-Agent": settings.user_agent},
        )

    monkeypatch.setattr(cli, "http_factory", factory)
    return fake


def test_sv1_cards_then_images(portal: FakePortal, data_dir: Path) -> None:
    cards = invoke("crawl", "sv1-cards")
    assert cards.exit_code == 0, cards.output
    assert " zh-tw: 1000 cards" in cards.output
    for lang in sv1.LANGUAGES:
        assert (data_dir / sv1.api_path(lang)).is_file()
    images = invoke("crawl", "sv1-images", "--limit", "3")
    assert images.exit_code == 0, images.output
    assert "1500 images, 3 to fetch" in images.output
    assert portal.calls[3:] == [
        sv1.image_url(card_id(0), sv1.Face.BASE),
        sv1.image_url(card_id(0), sv1.Face.EVOLVED),
        sv1.image_url(card_id(1), sv1.Face.BASE),
    ]
    dry = invoke("crawl", "sv1-images", "--dry-run")
    assert "1497 of 1500 URLs would be requested" in dry.output
    with manifest_at(data_dir) as manifest:
        resource = manifest.resources.get(portal.calls[3])
        assert resource is not None
        assert (data_dir / resource.path).read_bytes().startswith(b"\x89PNG")


def test_sv1_images_reports_a_missing_image_and_goes_on(portal: FakePortal) -> None:
    assert invoke("crawl", "sv1-cards").exit_code == 0
    for number in (card_id(1), card_id(3)):
        portal.missing.add(f"/image/card/phase2/common/C/C_{number}.png")
    portal.no_page.add(card_id(1))
    result = invoke("crawl", "sv1-images", "--limit", "10")
    assert f"{card_id(1)} base: no image on the site" in result.output
    assert f"{card_id(3)} base:" in result.output
    assert "yet the card page links it" in result.output
    assert sv1.image_url(card_id(4), sv1.Face.EVOLVED) in portal.calls


def test_sv1_cards_reports_a_failed_language(portal: FakePortal) -> None:
    portal.gone.add("/api/v1/cards")
    result = invoke("crawl", "sv1-cards")
    assert result.exit_code == 1
    assert "zh-tw:" in result.output
    assert "3 failed" in result.output


def test_sv1_images_before_sv1_cards_says_what_to_run(portal: FakePortal) -> None:
    result = invoke("crawl", "sv1-images")
    assert result.exit_code == 1
    assert "run `crawl sv1-cards` first" in result.output
    assert portal.calls == []
    dry = invoke("crawl", "sv1-images", "--dry-run")
    assert "run `crawl sv1-cards` first" in dry.output


def test_sv1_cards_dry_run_sends_nothing(portal: FakePortal) -> None:
    result = invoke("crawl", "sv1-cards", "--dry-run")
    assert result.exit_code == 0, result.output
    assert "3 of 3 URLs would be requested" in result.output
    assert portal.calls == []


@pytest.fixture
def wb(monkeypatch: pytest.MonkeyPatch, data_dir: Path, clock: FakeClock) -> FakeWb:
    del data_dir, clock  # requested for their side effects
    fake = FakeWb()

    def factory(settings: Settings) -> httpx.AsyncClient:
        del settings
        return httpx.AsyncClient(transport=httpx.MockTransport(fake))

    monkeypatch.setattr(cli, "http_factory", factory)
    return fake


def test_svwb_cards_dry_run_then_crawl(wb: FakeWb, data_dir: Path) -> None:
    before = invoke("crawl", "svwb-cards", "--dry-run")
    assert before.exit_code == 0, before.output
    assert "3 URLs would be requested" in before.output
    result = invoke("crawl", "svwb-cards")
    assert result.exit_code == 0, result.output
    assert "   cht: 505 cards" in result.output
    assert (data_dir / svwb.list_path("cht", 480)).is_file()
    after = invoke("crawl", "svwb-cards", "--dry-run")
    assert "51 URLs would be requested" in after.output
    assert len(wb.calls) == 51


def test_svwb_cards_reports_a_failed_language(wb: FakeWb) -> None:
    wb.grow_after = 1
    result = invoke("crawl", "svwb-cards")
    assert result.exit_code == 1, result.output
    assert "ja:" in result.output
    assert "changed" in result.output


def test_svwb_images_after_cards(wb: FakeWb, data_dir: Path) -> None:
    before = invoke("crawl", "svwb-images")
    assert before.exit_code == 1, before.output
    assert "run `crawl svwb-cards` first" in before.output
    assert invoke("crawl", "svwb-cards").exit_code == 0
    result = invoke("crawl", "svwb-images", "--limit", "3")
    assert result.exit_code == 0, result.output
    assert "809 images, 3 to fetch" in result.output
    image_calls = [u for u, _ in wb.calls if "/uploads/" in u]
    assert len(image_calls) == 3
    with manifest_at(data_dir) as manifest:
        resource = manifest.resources.get(image_calls[0])
        assert resource is not None
        assert (data_dir / resource.path).read_bytes().startswith(b"\x89PNG")
    dry = invoke("crawl", "svwb-images", "--dry-run")
    assert "806 of 809 URLs would be requested" in dry.output

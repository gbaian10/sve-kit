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

from .conftest import FakeClock
from .fakesite import IMG, FakeSite

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

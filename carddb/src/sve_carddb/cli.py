"""Command line entry point: `sve-carddb`."""

import asyncio
import uuid
from contextlib import AbstractContextManager, nullcontext
from dataclasses import dataclass
from enum import StrEnum
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- typer reads annotations at runtime
from typing import TYPE_CHECKING, Annotated

import httpx
import typer
from pydantic import ValidationError as SettingsError
from rich.console import Console

from sve_carddb.config import Settings
from sve_carddb.crawl import (
    Crawler,
    LimitReachedError,
    ListInconsistentError,
    Mode,
    card_numbers,
    current_sets,
)
from sve_carddb.fetch.client import (
    BudgetExhaustedError,
    Client,
    ClientPolicy,
    FetchError,
    StopCrawlError,
)
from sve_carddb.fetch.throttle import CircuitBreaker, CircuitOpenError, Throttle
from sve_carddb.fetch.writer import DiskFullError, LocalState, Writer, remove_temp_files
from sve_carddb.manifest import AlreadyRunningError, ExclusiveLock, Manifest
from sve_carddb.sources import official_jp as jp

if TYPE_CHECKING:
    from collections.abc import Callable

app = typer.Typer(no_args_is_help=True, help="Crawl and build the SVE card database.")
crawl_app = typer.Typer(
    no_args_is_help=True, help="Fetch official pages into SVE_DATA_DIR."
)
manifest_app = typer.Typer(
    no_args_is_help=True, help="Inspect and back up the manifest."
)
app.add_typer(crawl_app, name="crawl")
app.add_typer(manifest_app, name="manifest")

console = Console(soft_wrap=True)

# Runs that stop on purpose (--limit, --max-requests) exit 0; these exit 1.
_FATAL = (AlreadyRunningError, StopCrawlError, CircuitOpenError, DiskFullError)


class Stage(StrEnum):
    P0 = "p0"
    P1 = "p1"
    P2 = "p2"


@dataclass(frozen=True, slots=True)
class Job:
    """One `crawl` invocation, as given on the command line."""

    stage: Stage
    mode: Mode = Mode.RESUME
    sets: list[str] | None = None
    limit: int | None = None
    max_requests: int | None = None
    dry_run: bool = False


def make_http(settings: Settings) -> httpx.AsyncClient:
    """Build the HTTP client used for crawling."""
    return httpx.AsyncClient(
        headers={"User-Agent": settings.user_agent}, timeout=settings.timeout
    )


# Tests replace this to serve a fake site.
http_factory: Callable[[Settings], httpx.AsyncClient] = make_http

ModeOption = Annotated[
    Mode,
    typer.Option(
        help="resume: skip trusted copies; refresh: re-check them; "
        "repair: fetch only damaged copies."
    ),
]
SetOption = Annotated[
    list[str] | None,
    typer.Option("--set", help="Only this product code (repeatable)."),
]
LimitOption = Annotated[
    int | None, typer.Option(help="Stop after fetching this many URLs.")
]
BudgetOption = Annotated[
    int | None,
    typer.Option(help="Hard cap on HTTP requests, retries and redirects included."),
]
DryRunOption = Annotated[
    bool, typer.Option(help="List the URLs that would be fetched; send nothing.")
]


@crawl_app.command("p0")
def crawl_p0(
    limit: LimitOption = None,
    max_requests: BudgetOption = None,
    dry_run: DryRunOption = False,
) -> None:
    """Discover products and fetch page 1 of every product list."""
    _run(Job(Stage.P0, limit=limit, max_requests=max_requests, dry_run=dry_run))


@crawl_app.command("p1")
def crawl_p1(
    mode: ModeOption = Mode.RESUME,
    sets: SetOption = None,
    limit: LimitOption = None,
    max_requests: BudgetOption = None,
    dry_run: DryRunOption = False,
) -> None:
    """Read every page of the product lists, one validated generation per product."""
    _run(Job(Stage.P1, mode, sets, limit, max_requests, dry_run))


@crawl_app.command("p2")
def crawl_p2(
    mode: ModeOption = Mode.RESUME,
    sets: SetOption = None,
    limit: LimitOption = None,
    max_requests: BudgetOption = None,
    dry_run: DryRunOption = False,
) -> None:
    """Fetch the card pages listed by the validated product lists."""
    _run(Job(Stage.P2, mode, sets, limit, max_requests, dry_run))


@manifest_app.command("check")
def manifest_check() -> None:
    """Verify every stored file against the manifest; exit 1 if any is damaged."""
    settings = _settings()
    with Manifest.open(settings.manifest_path) as manifest:
        writer = Writer(settings.data_dir, manifest)
        counts = dict.fromkeys(LocalState, 0)
        damaged: list[str] = []
        for resource in manifest.resources.all():
            state = writer.local_state(resource.url)
            counts[state] += 1
            if state in {LocalState.UNTRUSTED, LocalState.MISSING}:
                damaged.append(resource.url)
    for state, count in counts.items():
        console.print(f"{state:>10}: {count}")
    for url in damaged[:20]:
        console.print(f"  damaged: {url}")
    if damaged:
        raise typer.Exit(1)


@manifest_app.command("backup")
def manifest_backup(dest: Path) -> None:
    """Write a verified snapshot of the manifest to DEST, which must not exist."""
    settings = _settings()
    with Manifest.open(settings.manifest_path) as manifest:
        info = manifest.backup(dest)
    console.print(f"backup: {info.path}\nsha256: {info.sha256}")


def main() -> None:
    """Run the CLI."""
    app()


# --- implementation ---------------------------------------------------------


def _settings() -> Settings:
    try:
        return Settings()  # pyright: ignore[reportCallIssue] -- read from the environment
    except SettingsError as exc:
        console.print(f"[red]configuration error[/red] (is SVE_DATA_DIR set?)\n{exc}")
        raise typer.Exit(2) from exc


def _run(job: Job) -> None:
    if job.mode is Mode.REFRESH and not job.sets:
        console.print("[red]--mode refresh needs --set[/red]")
        raise typer.Exit(2)
    settings = _settings()
    lock: AbstractContextManager[object] = (
        nullcontext() if job.dry_run else ExclusiveLock(settings.lock_path)
    )
    try:
        with lock, Manifest.open(settings.manifest_path) as manifest:
            writer = Writer(settings.data_dir, manifest)
            if job.dry_run:
                _dry_run(job, writer, manifest)
                return
            _recover(settings, manifest)
            failures = asyncio.run(_crawl(job, settings, manifest, writer))
    except (LimitReachedError, BudgetExhaustedError) as exc:
        console.print(f"[yellow]stopped:[/yellow] {exc}")
        return
    except _FATAL as exc:
        console.print(f"[red]stopped:[/red] {exc}")
        raise typer.Exit(1) from exc
    if failures:
        console.print(f"[red]{failures} failed[/red]")
        raise typer.Exit(1)


def _recover(settings: Settings, manifest: Manifest) -> None:
    """Clean up after a crashed run before fetching anything."""
    removed = remove_temp_files(settings.data_dir)
    interrupted = manifest.requests.mark_interrupted()
    if removed or interrupted:
        console.print(
            f"recovered: {len(removed)} temp files removed, "
            f"{interrupted} interrupted requests marked"
        )


async def _crawl(
    job: Job, settings: Settings, manifest: Manifest, writer: Writer
) -> int:
    async with http_factory(settings) as http:
        client = Client(
            http,
            Throttle(settings.interval, settings.jitter),
            manifest,
            run_id=uuid.uuid4().hex,
            policy=ClientPolicy(max_requests=job.max_requests),
        )
        crawler = Crawler(
            client=client,
            writer=writer,
            manifest=manifest,
            breaker=CircuitBreaker(settings.breaker_threshold),
            mode=job.mode,
            limit=job.limit,
        )
        try:
            match job.stage:
                case Stage.P0:
                    return await _p0(crawler)
                case Stage.P1:
                    return await _p1(crawler, job.sets)
                case Stage.P2:
                    return await _p2(crawler, job.sets)
        finally:
            console.print(f"HTTP requests sent: {client.requests_sent}")


async def _p0(crawler: Crawler) -> int:
    sets = await crawler.discover_sets()
    console.print(f"{len(sets)} products")
    pages = cards = 0
    # One line per product, so a run stopped by --limit still shows what it read.
    for card_set in sets:
        summary = await crawler.first_page(card_set.code)
        console.print(
            f"{card_set.code:>8}: {summary.total:>4} cards, {summary.max_page:>3} pages"
        )
        pages += summary.max_page
        cards += summary.total
    console.print(
        f"P1 needs about {pages + len(sets)} requests; "
        f"P2 at most {cards} (fewer, as products share cards)."
    )
    return 0


async def _p1(crawler: Crawler, sets: list[str] | None) -> int:
    codes = sets or crawler.current_sets()
    if not codes:
        console.print("[red]no products known; run `crawl p0` first[/red]")
        return 1
    failures = 0
    for code in codes:
        try:
            summary = await crawler.discover_list(code)
        except (ListInconsistentError, FetchError) as exc:
            failures += 1
            console.print(f"[red]{code}:[/red] {exc}")
            continue
        console.print(f"{code}: {summary.total} cards on {summary.max_page} pages")
    return failures


async def _p2(crawler: Crawler, sets: list[str] | None) -> int:
    numbers = crawler.card_numbers(sets)
    if not numbers:
        console.print("[red]no validated product lists; run `crawl p1` first[/red]")
        return 1
    failures = 0
    for index, number in enumerate(numbers, start=1):
        # One bad card page is recorded and skipped; the circuit breaker still
        # stops the run if failures pile up.
        try:
            await crawler.card(number)
        except FetchError as exc:
            failures += 1
            console.print(f"[red]{number}:[/red] {exc}")
        if index % 100 == 0:
            console.print(f"{index}/{len(numbers)} cards")
    return failures


def _dry_run(job: Job, writer: Writer, manifest: Manifest) -> None:
    known = current_sets(manifest)
    urls: list[str] = []
    match job.stage:
        case Stage.P0:
            urls = [jp.sets_url(), *(jp.list_url(code, 1) for code in known)]
            if not known:
                console.print("products not discovered yet: page 1 URLs unknown")
        case Stage.P1:
            for code in job.sets or known:
                urls += _list_urls(code, writer)
        case Stage.P2:
            urls = [jp.card_url(n) for n in card_numbers(manifest, job.sets)]
    fetch = [u for u in urls if _would_fetch(job, writer.local_state(u))]
    for url in fetch:
        console.print(url)
    console.print(f"{len(fetch)} of {len(urls)} URLs would be requested")


def _list_urls(code: str, writer: Writer) -> list[str]:
    first = jp.list_url(code, 1)
    if writer.local_state(first) is not LocalState.TRUSTED:
        console.print(f"{code}: page 1 not stored yet; run `crawl p0` first")
        return []
    max_page = jp.parse_list_first(writer.read(first)).max_page or 1
    return [jp.list_url(code, n) for n in range(1, max_page + 1)]


def _would_fetch(job: Job, state: LocalState) -> bool:
    # P0 and P1 always re-read their pages to build a new generation.
    if job.stage is not Stage.P2 or state is not LocalState.TRUSTED:
        return True
    return job.mode is Mode.REFRESH

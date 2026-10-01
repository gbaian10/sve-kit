"""Only in-process fake HTTP and temporary protected stores; no official content."""

import asyncio
import shutil
from dataclasses import dataclass, replace
from itertools import pairwise
from typing import TYPE_CHECKING, cast

import httpx
import pytest

from sve_carddb.card_extras.generation import root_key
from sve_carddb.card_extras.incremental import QACrawler
from sve_carddb.card_extras.qa_archive import FrozenOfficialExtras
from sve_carddb.config import Settings
from sve_carddb.fetch.client import FetchError
from sve_carddb.fetch.refresh import RefreshWriter
from sve_carddb.fetch.writer import Writer
from sve_carddb.manifest import ExclusiveLock, GenerationStatus, Kind, Manifest
from sve_carddb.source_archive import ArchiveError, ArchiveStore, seal_batch
from sve_carddb.sources import official_en, official_jp

from .official_qa_fixtures import (
    DETAIL,
    ROOT,
    SECOND,
    Clock,
    Server,
    SyntheticInterruptedError,
    bodies,
)
from .test_card_extras_archive import RAW

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.card_extras.models import QAPage
    from sve_carddb.registry.records import Region


def store_at(root: Path) -> ArchiveStore:
    data = root / "data"
    return ArchiveStore(
        data,
        data / "manifest/manifest.sqlite",
        data / "manifest/.lock",
        root / "archive",
        "synthetic",
    )


def protected(
    store: ArchiveStore, manifest: Manifest, lock: ExclusiveLock
) -> RefreshWriter:
    return RefreshWriter(
        store,
        manifest,
        lock,
        store.root.parent / "backup",
        restore_root=store.root.parent / "restore",
        require_separate_device=False,
    )


@dataclass(frozen=True)
class Sealed:
    store: ArchiveStore
    batch: str
    requests: tuple[tuple[str, float, int], ...]
    pages: tuple[QAPage, ...]
    sizes: tuple[int, ...]


@pytest.fixture(scope="module")
def sealed(tmp_path_factory: pytest.TempPathFactory) -> Sealed:
    store = store_at(tmp_path_factory.mktemp("official-qa"))
    server = Server(Clock())
    sizes: list[int] = []
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        writer = protected(store, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(server.respond),
                headers={
                    "User-Agent": Settings(data_dir=writer.store.data_root).user_agent
                },
            ) as http:
                crawler = QACrawler(
                    http,
                    writer,
                    region="jp",
                    run_id="synthetic",
                    clock=server.clock.read,
                    sleep=server.clock.sleep,
                )
                assert (await crawler.collect(ROOT)).complete
                sizes.append(len(list(store.root.glob("descriptors/*.json"))))
                assert (await crawler.collect(ROOT)).complete
                sizes.append(len(list(store.root.glob("descriptors/*.json"))))
                server.unchanged_200 = True
                assert (await crawler.collect(ROOT)).complete
                sizes.append(len(list(store.root.glob("descriptors/*.json"))))
                server.unchanged_200 = False
                server.pages = bodies("Synthetic changed same-day answer.")
                assert (await crawler.collect(ROOT)).complete
                sizes.append(len(list(store.root.glob("descriptors/*.json"))))
                server.pages = bodies(
                    "Synthetic changed same-day answer.", state="withdrawn"
                )
                assert (await crawler.collect(ROOT)).complete
                sizes.append(len(list(store.root.glob("descriptors/*.json"))))

        asyncio.run(run())
    batch = seal_batch(store).batch_id
    provider = FrozenOfficialExtras(store.root, store.store_id, batch, region="jp")
    return Sealed(
        store, batch, tuple(server.requests), tuple(provider.qa_pages()), tuple(sizes)
    )


class TestProtectedCollection:
    def test_minimum_two_seconds_on_all_requests(self, sealed: Sealed) -> None:
        times = [time for _, time, _ in sealed.requests]
        assert all(right - left >= 2.0 for left, right in pairwise(times))
        assert 304 in {status for _, _, status in sealed.requests}

    def test_304_and_identical_200_do_not_create_source_versions(
        self, sealed: Sealed
    ) -> None:
        assert sealed.sizes == (3, 3, 3, 4, 5)
        assert (
            len([request for request in sealed.requests if request[0] == DETAIL]) == 5
        )

    def test_old_raw_survives_replacement(self, sealed: Sealed) -> None:
        answers = {block.entry.answer for page in sealed.pages for block in page.blocks}
        assert {"Synthetic answer B.", "Synthetic changed same-day answer."} <= answers
        assert {
            block.entry.state for page in sealed.pages for block in page.blocks
        } == {"active", "withdrawn"}

    def test_pinned_generation_rechecks_without_live_access(
        self, sealed: Sealed, monkeypatch: pytest.MonkeyPatch
    ) -> None:
        def forbidden(*_args: object, **_kwargs: object) -> None:
            raise AssertionError("Live access forbidden")

        monkeypatch.setattr(Manifest, "open", forbidden)
        monkeypatch.setattr(Manifest, "open_live", forbidden)
        monkeypatch.setattr(httpx.AsyncClient, "send", forbidden)
        provider = FrozenOfficialExtras(
            sealed.store.root, sealed.store.store_id, sealed.batch, region="jp"
        )
        coverage = provider.coverage(ROOT)
        assert coverage.closure.complete
        assert len(coverage.source_ids) == 3
        assert len(tuple(provider.qa_pages())) == 5
        report = provider.report(ROOT)
        assert report["complete"] is True
        variants = report["conflicts"]
        assert isinstance(variants, list)
        assert len(variants) == 1
        assert "Synthetic answer" not in str(report)


@pytest.fixture
def copied(sealed: Sealed, tmp_path: Path) -> ArchiveStore:
    shutil.copytree(sealed.store.root.parent, tmp_path / "copy")
    return store_at(tmp_path / "copy")


@pytest.mark.parametrize(
    ("fault", "expected"),
    [
        ("page", "index_unfetched"),
        ("detail", "detail_unfetched"),
        ("edges", "generation_edges_disagreement"),
        ("metadata", "generation_metadata_disagreement"),
        ("status", "generation_not_validated"),
    ],
)
def test_validated_flag_cannot_replace_evidence(
    copied: ArchiveStore, fault: str, expected: str
) -> None:
    with Manifest.open(copied.manifest_path) as manifest:
        original = manifest.generations.current(root_key(ROOT, "jp"))
        assert original is not None
        new = manifest.generations.start(original.root)
        for page in manifest.generations.pages(original.id):
            if page.url == {"page": SECOND, "detail": DETAIL}.get(fault):
                continue
            edges = [
                edge.link
                for edge in manifest.generations.edges(original.id)
                if edge.from_url == page.url
            ]
            with manifest.transaction():
                manifest.generations.add_page(
                    new.id, page.url, page.sha256, [] if fault == "edges" else edges
                )
        if fault == "status":
            manifest.generations.fail(new.id)
        else:
            manifest.generations.validate(
                new.id, declared_total=99 if fault == "metadata" else 5, max_page=2
            )
    batch = seal_batch(copied).batch_id
    checked = FrozenOfficialExtras(
        copied.root, copied.store_id, batch, region="jp"
    ).coverage(ROOT)
    assert not checked.closure.complete
    assert expected in checked.closure.issues


def test_interruption_preserves_partial_and_cannot_reuse_previous_complete(
    copied: ArchiveStore,
) -> None:
    server = Server(
        Clock(), pages=bodies("Synthetic interrupted new version."), interrupt=DETAIL
    )
    with (
        ExclusiveLock(copied.lock_path) as lock,
        Manifest.open(copied.manifest_path) as manifest,
    ):
        writer = protected(copied, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(server.respond),
                headers={
                    "User-Agent": Settings(data_dir=writer.store.data_root).user_agent
                },
            ) as http:
                crawler = QACrawler(
                    http,
                    writer,
                    region="jp",
                    run_id="interrupted",
                    clock=server.clock.read,
                    sleep=server.clock.sleep,
                )
                with pytest.raises(SyntheticInterruptedError):
                    await crawler.collect(ROOT)
                latest = manifest.generations.latest(root_key(ROOT, "jp"))
                assert latest is not None
                assert latest.status is GenerationStatus.FAILED
                assert manifest.generations.current(root_key(ROOT, "jp")) is not None

        asyncio.run(run())
    batch = seal_batch(copied).batch_id
    provider = FrozenOfficialExtras(copied.root, copied.store_id, batch, region="jp")
    assert not provider.coverage(ROOT).closure.complete
    assert any(
        block.entry.answer == "Synthetic interrupted new version."
        for page in provider.qa_pages()
        for block in page.blocks
    )


@pytest.mark.parametrize("fault", ["changed_root", "missing_pagination", "detail_404"])
def test_incomplete_fake_discovery_never_validates(
    copied: ArchiveStore, fault: str
) -> None:
    server = Server(Clock())
    if fault == "changed_root":
        server.change_on_recheck = server.pages[ROOT].replace(
            b"Synthetic answer A.", b"Synthetic changed root."
        )
    elif fault == "missing_pagination":
        server.pages[ROOT] = server.pages[ROOT].replace(
            b'data-total="5"', b'unknown-total="5"'
        )
    else:
        del server.pages[DETAIL]
    with (
        ExclusiveLock(copied.lock_path) as lock,
        Manifest.open(copied.manifest_path) as manifest,
    ):
        writer = protected(copied, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(server.respond),
                headers={
                    "User-Agent": Settings(data_dir=writer.store.data_root).user_agent
                },
            ) as http:
                crawler = QACrawler(
                    http,
                    writer,
                    region="jp",
                    run_id="partial",
                    clock=server.clock.read,
                    sleep=server.clock.sleep,
                )
                if fault == "detail_404":
                    with pytest.raises(FetchError):
                        await crawler.collect(ROOT)
                else:
                    assert not (await crawler.collect(ROOT)).complete
                latest = manifest.generations.latest(root_key(ROOT, "jp"))
                assert latest is not None
                assert latest.status is GenerationStatus.FAILED

        asyncio.run(run())


def test_unprotected_writer_rejected_before_network(tmp_path: Path) -> None:
    with Manifest.open_empty() as manifest:
        writer = Writer(tmp_path, manifest)
        with pytest.raises(TypeError, match="protected refresh writer"):
            QACrawler(
                cast("httpx.AsyncClient", None),
                cast("RefreshWriter", writer),
                region="jp",
                run_id="forbidden",
            )


@pytest.mark.parametrize("region", ["jp", "en"])
def test_regional_related_card_incremental_contract(
    tmp_path: Path, region: Region
) -> None:
    store = store_at(tmp_path)
    adapter = official_jp if region == "jp" else official_en
    url = adapter.card_url("TEST-001Ⓢa")
    raw = (
        RAW.replace(
            b'<img alt="synthetic.icon">',
            b'<img src="/assets/images/common/icon/icon_evolve.png" alt="Evolve">',
        )
        if region == "en"
        else RAW
    )
    server = Server(Clock(), pages={url: raw})
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        writer = protected(store, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(server.respond),
                headers={
                    "User-Agent": Settings(data_dir=writer.store.data_root).user_agent
                },
            ) as http:
                crawler = QACrawler(
                    http,
                    writer,
                    region=region,
                    run_id="related",
                    clock=server.clock.read,
                    sleep=server.clock.sleep,
                )
                assert await crawler.cards(("TEST-001Ⓢa",)) == (url,)
                assert await crawler.cards(("TEST-001Ⓢa",)) == (url,)
                assert any(
                    link.to_kind is Kind.CARD for link in manifest.links.current(url)
                )

        asyncio.run(run())
    batch = seal_batch(store).batch_id
    provider = FrozenOfficialExtras(store.root, store.store_id, batch, region=region)
    pages = tuple(provider.card_pages())
    assert len(pages) == 1
    assert pages[0].related[0].href_raw == "?cardno=TEST-002"
    assert pages[0].region == region


def test_unknown_generation_is_not_coverage(sealed: Sealed) -> None:
    provider = FrozenOfficialExtras(
        sealed.store.root, sealed.store.store_id, sealed.batch, region="jp"
    )
    checked = provider.coverage(ROOT + "unknown/")
    assert not checked.closure.complete
    assert checked.closure.issues == ("generation_unknown",)


def test_corrupt_frozen_raw_cannot_supply_coverage(
    copied: ArchiveStore, sealed: Sealed
) -> None:
    provider = FrozenOfficialExtras(
        copied.root, copied.store_id, sealed.batch, region="jp"
    )
    source_id = provider.coverage(ROOT).source_ids[0]
    entry = provider.sources.entries[source_id]
    (copied.root / entry.blob.path).write_bytes(b"Synthetic corrupt raw")
    with pytest.raises(ArchiveError, match="hash"):
        provider.coverage(ROOT)


def test_en_listing_detail_fake_server_contract(tmp_path: Path) -> None:
    store = store_at(tmp_path)
    host = official_en.HOST
    root = ROOT.replace(official_jp.HOST, host)
    server = Server(
        Clock(),
        pages={
            url.replace(official_jp.HOST, host): raw.replace(
                official_jp.HOST.encode(), host.encode()
            )
            for url, raw in bodies().items()
        },
    )
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        writer = protected(store, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(server.respond),
                headers={
                    "User-Agent": Settings(data_dir=writer.store.data_root).user_agent
                },
            ) as http:
                crawler = QACrawler(
                    http,
                    writer,
                    region="en",
                    run_id="synthetic-en",
                    clock=server.clock.read,
                    sleep=server.clock.sleep,
                )
                assert (await crawler.collect(root)).complete
                assert (await crawler.collect(root)).complete

        asyncio.run(run())
    batch = seal_batch(store).batch_id
    provider = FrozenOfficialExtras(store.root, store.store_id, batch, region="en")
    report = provider.report(root)
    assert report["complete"] is True
    assert len(tuple(provider.qa_pages())) == 3
    assert report["conflicts"]


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("link_kind", ["pagination", "detail"])
@pytest.mark.parametrize(
    "target_kind", ["foreign_host", "other_region", "outside_namespace"]
)
def test_external_discovery_rejected_before_request_even_without_http_filter(
    tmp_path: Path, region: Region, link_kind: str, target_kind: str
) -> None:
    host = official_jp.HOST if region == "jp" else official_en.HOST
    other = official_en.HOST if region == "jp" else official_jp.HOST
    root = ROOT.replace(official_jp.HOST, host)
    pages = {
        url.replace(official_jp.HOST, host): raw.replace(
            official_jp.HOST.encode(), host.encode()
        )
        for url, raw in bodies().items()
    }
    targets = {
        "foreign_host": "https://example.invalid/qa/synthetic/",
        "other_region": f"https://{other}/qa/synthetic/",
        "outside_namespace": f"https://{host}/unrelated/",
    }
    target = targets[target_kind] + (
        "?page=2" if link_kind == "pagination" else "detail/"
    )
    old = (SECOND if link_kind == "pagination" else DETAIL).replace(
        official_jp.HOST, host
    )
    raw = pages[root].replace(old.encode(), target.encode())
    requests: list[str] = []

    def respond(request: httpx.Request) -> httpx.Response:
        # Record every attempted URL; this fake transport permits even foreign hosts.
        url = str(request.url)
        requests.append(url)
        content = (
            raw
            if url == root
            else pages.get(url, pages[DETAIL.replace(official_jp.HOST, host)])
        )
        return httpx.Response(
            200, headers={"Content-Type": "text/html"}, content=content
        )

    store = store_at(tmp_path)
    clock = Clock()
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        writer = protected(store, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(respond),
                headers={"User-Agent": Settings(data_dir=store.data_root).user_agent},
            ) as http:
                crawler = QACrawler(
                    http,
                    writer,
                    region=region,
                    run_id="source-boundary",
                    clock=clock.read,
                    sleep=clock.sleep,
                )
                # Isolate parser rejection from the lower Client/Site allow-list.
                crawler.crawler.site = replace(
                    crawler.crawler.site, allowed=lambda _url: True
                )
                with pytest.raises(FetchError):
                    await crawler.collect(root)

        asyncio.run(run())
        assert requests == [root]
        assert manifest.resources.get(root) is None
        latest = manifest.generations.latest(root_key(root, region))
        assert latest is not None
        assert latest.status is GenerationStatus.FAILED


def test_noncanonical_root_rejected_before_any_request(tmp_path: Path) -> None:
    requests: list[str] = []

    def respond(request: httpx.Request) -> httpx.Response:
        requests.append(str(request.url))
        return httpx.Response(
            200, headers={"Content-Type": "text/html"}, content=bodies()[ROOT]
        )

    store = store_at(tmp_path)
    root = ROOT + "?b=2&a=1"
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        writer = protected(store, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(respond)
            ) as http:
                crawler = QACrawler(http, writer, region="jp", run_id="noncanonical")
                crawler.crawler.site = replace(
                    crawler.crawler.site, allowed=lambda _url: True
                )
                with pytest.raises(ValueError, match="canonical regional HTTPS URL"):
                    await crawler.collect(root)

        asyncio.run(run())
        assert not requests
        assert manifest.generations.latest(root_key(root, "jp")) is None


@pytest.mark.parametrize("region", ["jp", "en"])
@pytest.mark.parametrize("configured", [False, True])
def test_qa_and_card_requests_inherit_crawler_settings_user_agent(
    tmp_path: Path, region: Region, configured: bool
) -> None:
    host = official_jp.HOST if region == "jp" else official_en.HOST
    root = ROOT.replace(official_jp.HOST, host)
    pages = {
        url.replace(official_jp.HOST, host): raw.replace(
            official_jp.HOST.encode(), host.encode()
        )
        for url, raw in bodies().items()
    }
    adapter = official_jp if region == "jp" else official_en
    pages[adapter.card_url("TEST-001Ⓢa")] = RAW
    server = Server(Clock(), pages=pages)
    received: list[str] = []

    def respond(request: httpx.Request) -> httpx.Response:
        received.append(request.headers["User-Agent"])
        return server.respond(request)

    store = store_at(tmp_path)
    settings = Settings(data_dir=store.data_root)
    if configured:
        settings = settings.model_copy(
            update={"user_agent": "Mozilla/5.0 (synthetic configured browser)"}
        )
    with (
        ExclusiveLock(store.lock_path) as lock,
        Manifest.open(store.manifest_path) as manifest,
    ):
        writer = protected(store, manifest, lock)

        async def run() -> None:
            async with httpx.AsyncClient(
                transport=httpx.MockTransport(respond),
                headers={"User-Agent": settings.user_agent},
                timeout=settings.timeout,
            ) as http:
                crawler = QACrawler(
                    http,
                    writer,
                    region=region,
                    run_id="configured-ua",
                    clock=server.clock.read,
                    sleep=server.clock.sleep,
                )
                assert (await crawler.collect(root)).complete
                await crawler.cards(("TEST-001Ⓢa",))

        asyncio.run(run())
    assert len(received) == 5
    assert all(value == settings.user_agent for value in received)

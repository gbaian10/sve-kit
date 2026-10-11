"""Parallel upload behavior over a delayed, entirely in-memory S3 adapter."""

import json
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager
from dataclasses import dataclass, field
from threading import Event, Lock
from time import perf_counter, sleep
from typing import TYPE_CHECKING, override

import httpx
import pytest
from sve_carddb.export.read_api import INDEX, ExportError, load_export
from typer.testing import CliRunner

from sve_publish import parallel
from sve_publish.adapter import R2Store, Stored
from sve_publish.cli import app
from sve_publish.publish import report, upload

from .r2_export_fixtures import export
from .r2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- synthetic corpus
from .r2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- isolated export roots
from .r2_fixtures import ACCOUNT, BUCKET, CREDENTIALS
from .r2_sdk_fixtures import install_mock_sdk, mock_client

if TYPE_CHECKING:
    from collections.abc import Iterator

    from sve_carddb.export.preview import Roots

    from .synthetic_images import PublicImages

pytestmark = pytest.mark.usefixtures("close_sdk_clients")


@dataclass
class DelayedS3(httpx.BaseTransport):
    delay: float = 0.02
    objects: dict[str, Stored] = field(default_factory=dict)
    requests: list[tuple[str, str]] = field(default_factory=list)
    lock: Lock = field(default_factory=Lock)
    read_back: set[str] = field(default_factory=set)
    image_keys: set[str] = field(default_factory=set)
    fail_key: str | None = None
    fault: str = "condition"
    block_reads: set[str] = field(default_factory=set)
    block_readback: str | None = None
    blocked: Event = field(default_factory=Event)
    release: Event = field(default_factory=Event)
    other_images_done: Event = field(default_factory=Event)
    active: int = 0
    peak: int = 0

    @override
    def handle_request(self, request: httpx.Request) -> httpx.Response:
        key = request.url.path.removeprefix("/" + BUCKET + "/")
        method = request.method
        with self.lock:
            self.requests.append((method, key))
            self.active += 1
            self.peak = max(self.peak, self.active)
            existing = self.objects.get(key)
        try:
            sleep(self.delay)
            if method == "GET" and (
                key in self.block_reads
                or (key == self.block_readback and existing is not None)
            ):
                self.blocked.set()
                assert self.release.wait(10), "test did not release held read"
            with self.lock:
                return self.respond(request, key, existing)
        finally:
            with self.lock:
                self.active -= 1

    def respond(
        self, request: httpx.Request, key: str, existing: Stored | None
    ) -> httpx.Response:
        if request.method == "GET":
            if existing is None:
                return httpx.Response(404)
            if key == self.fail_key and self.fault == "readback":
                return httpx.Response(
                    200,
                    stream=httpx.ByteStream(b"corrupt readback"),
                    headers=existing.headers | {"etag": existing.etag},
                )
            self.read_back.add(key)
            if self.image_keys - {self.block_readback} <= self.read_back:
                self.other_images_done.set()
            return httpx.Response(
                200,
                stream=httpx.ByteStream(existing.raw),
                headers=existing.headers | {"etag": existing.etag},
            )
        assert request.method == "PUT"
        new_conflict = (
            request.headers.get("if-none-match") == "*" and existing is not None
        )
        overwrite_conflict = "if-match" in request.headers and (
            existing is None or existing.etag != request.headers["if-match"]
        )
        forced_failure = key == self.fail_key and self.fault == "condition"
        if forced_failure or new_conflict or overwrite_conflict:
            return httpx.Response(412)
        assert "if-match" in request.headers or request.headers["if-none-match"] == "*"
        etag = f'"opaque-{len(self.requests)}"'
        self.objects[key] = Stored(
            request.content,
            etag,
            {
                name: request.headers[name]
                for name in ("content-type", "cache-control", "content-encoding")
                if name in request.headers
            },
        )
        if key == self.fail_key and self.fault == "transport":
            raise httpx.ReadTimeout("synthetic lost response")
        return httpx.Response(200, headers={"etag": etag})


@contextmanager
def memory_store(fake: DelayedS3) -> Iterator[R2Store]:
    with httpx.Client(transport=fake, trust_env=False) as http:
        yield R2Store(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(http, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS),
        )


def test_cli_jobs_16_is_faster_with_identical_writes_and_summary(
    images: PublicImages, roots: Roots, monkeypatch: pytest.MonkeyPatch
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    summaries = []
    objects = []
    timings = []
    for jobs in (1, 16):
        fake = DelayedS3()
        with monkeypatch.context() as patch:
            install_mock_sdk(patch, fake)
            patch.setenv("SVE_R2_ACCESS_KEY_ID", CREDENTIALS.access_key)
            patch.setenv("SVE_R2_SECRET_ACCESS_KEY", CREDENTIALS.secret_key)
            start = perf_counter()
            result = CliRunner().invoke(
                app,
                [
                    "upload",
                    "--export-dir",
                    str(roots.preview),
                    "--account-id",
                    ACCOUNT,
                    "--bucket",
                    BUCKET,
                    "--skip-cdn-verify",
                    "--execute",
                    "--jobs",
                    str(jobs),
                ],
            )
            timings.append(perf_counter() - start)
        assert result.exit_code == 0, result.output
        summaries.append(json.loads(result.stdout))
        assert "Upload:" in result.stderr
        assert f"{len(loaded.images) + len(loaded.members)}/" in result.stderr
        objects.append({k: (v.raw, v.headers) for k, v in fake.objects.items()})
        assert fake.peak <= jobs
        if jobs > 1:
            assert fake.peak > 1
        assert fake.requests[-2:] == [("PUT", INDEX), ("GET", INDEX)]
    assert summaries[0] == summaries[1]
    assert objects[0] == objects[1]
    assert summaries[0]["written_files"] == len(loaded.images) + len(loaded.members)
    local = report(loaded)
    assert isinstance(local["image_bytes"], int)
    assert isinstance(local["json_bytes"], int)
    assert sum(len(raw) for key, (raw, _) in objects[0].items() if key != INDEX) == (
        local["image_bytes"] + local["json_bytes"]
    )
    assert timings[1] < timings[0] / 2
    print(  # ruff: ignore[print] -- record acceptance timings without object contents
        f"20 ms/request: jobs=1 {timings[0]:.3f}s; jobs=16 {timings[1]:.3f}s"
    )


def test_json_waits_for_every_image_readback(
    images: PublicImages, roots: Roots
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    fake = DelayedS3(
        image_keys={item.key for item in loaded.images},
        block_readback=loaded.images[0].key,
    )
    with memory_store(fake) as store, ThreadPoolExecutor(max_workers=1) as runner:
        future = runner.submit(upload, store, loaded, None, jobs=16)
        try:
            assert fake.blocked.wait(5)
            assert fake.other_images_done.wait(5)
            assert not any(
                method == "PUT" and key.startswith("snapshots/")
                for method, key in fake.requests
            )
            assert not future.done()
        finally:
            fake.release.set()
        result = future.result(timeout=10)
    assert result["index"] == "updated"


@pytest.mark.parametrize("failure", ["condition", "transport", "readback", "immutable"])
def test_failure_stops_waiting_writes_drains_and_rerun_only_fills_missing(
    images: PublicImages, roots: Roots, failure: str
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    failed = loaded.members[2]
    blocked_keys = {member.key for member in loaded.members[3:]}
    fake = DelayedS3(fail_key=failed.key, fault=failure, block_reads=blocked_keys)
    stopped = Event()

    def progress(line: str) -> None:
        if "failed" in line:
            stopped.set()

    if failure == "immutable":
        fake.fail_key = None
        fake.objects[failed.key] = Stored(b"other", '"other"', {})
    with memory_store(fake) as store, ThreadPoolExecutor(max_workers=1) as runner:
        future = runner.submit(upload, store, loaded, None, jobs=4, progress=progress)
        try:
            assert stopped.wait(10)
            assert fake.blocked.wait(5)
            assert not future.done()
            assert INDEX not in fake.objects
        finally:
            fake.release.set()
        with pytest.raises(ExportError, match=failed.key):
            future.result(timeout=10)
        assert not blocked_keys & fake.objects.keys()
        assert INDEX not in fake.objects
        done = set(fake.objects)
        fake.fail_key = None
        fake.block_reads.clear()
        if failure == "immutable":
            del fake.objects[failed.key]
            done.remove(failed.key)
        before = len(fake.requests)
        result = upload(store, loaded, None, jobs=16)
        new_writes = {key for method, key in fake.requests[before:] if method == "PUT"}
        assert not done & new_writes
        if failure in {"condition", "immutable"}:
            assert failed.key in new_writes
        else:
            assert failed.key not in new_writes
        assert result["unchanged_files"] == len(done)
        assert result["index"] == "updated"


@pytest.mark.parametrize("jobs", [0, -1])
def test_invalid_jobs_are_rejected_before_export_or_credentials(
    images: PublicImages, roots: Roots, jobs: int
) -> None:
    result = CliRunner().invoke(app, ["upload", "--jobs", str(jobs)])
    assert result.exit_code != 0
    assert "--jobs" in result.output
    assert "range" in result.output
    export(images, roots, step=0)
    fake = DelayedS3()
    with memory_store(fake) as store:
        with pytest.raises(ExportError, match="jobs must be at least 1"):
            upload(store, load_export(roots.preview), None, jobs=jobs)
    assert fake.requests == []


def test_progress_keeps_reporting_while_an_object_waits_for_readback(
    images: PublicImages, roots: Roots, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(parallel, "PROGRESS_SECONDS", 0)
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    fake = DelayedS3(delay=0, block_readback=loaded.images[0].key)
    heartbeat = Event()

    def progress(line: str) -> None:
        if fake.blocked.is_set() and "complete" in line:
            heartbeat.set()

    with memory_store(fake) as store, ThreadPoolExecutor(max_workers=1) as runner:
        future = runner.submit(upload, store, loaded, None, jobs=1, progress=progress)
        try:
            assert fake.blocked.wait(5)
            assert heartbeat.wait(5)
            assert not future.done()
        finally:
            fake.release.set()
        assert future.result(timeout=10)["index"] == "updated"

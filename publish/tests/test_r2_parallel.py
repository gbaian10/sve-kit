"""Parallel upload behavior over a delayed, entirely in-memory S3 adapter."""

import json
from concurrent.futures import ThreadPoolExecutor
from threading import Event
from time import perf_counter
from typing import TYPE_CHECKING

import pytest
from sve_carddb.export.read_api import INDEX, ExportError, load_export
from typer.testing import CliRunner

from sve_publish import parallel
from sve_publish.adapter import Stored
from sve_publish.cli import app
from sve_publish.inventory import INVENTORY
from sve_publish.publish import report, upload

from .r2_export_fixtures import export
from .r2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- synthetic corpus
from .r2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- isolated export roots
from .r2_fixtures import ACCOUNT, BUCKET, CREDENTIALS
from .r2_parallel_fixtures import DelayedS3, memory_store, ready
from .r2_sdk_fixtures import install_mock_sdk

if TYPE_CHECKING:
    from sve_carddb.export.preview import Roots

    from .synthetic_images import PublicImages

pytestmark = pytest.mark.usefixtures("close_sdk_clients")


def test_cli_jobs_bounds_concurrency_with_identical_writes_and_summary(
    images: PublicImages, roots: Roots, monkeypatch: pytest.MonkeyPatch
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    summaries = []
    objects = []
    timings = []
    for jobs in (1, 16):
        fake = DelayedS3()
        ready(fake, roots)
        with monkeypatch.context() as patch:
            install_mock_sdk(patch, fake)
            patch.setenv("SVE_R2_ACCESS_KEY_ID", CREDENTIALS.access_key)
            patch.setenv("SVE_R2_SECRET_ACCESS_KEY", CREDENTIALS.secret_key)
            start = perf_counter()
            result = CliRunner().invoke(
                app,
                [
                    "snapshot",
                    "upload",
                    "--export-dir",
                    str(roots.preview),
                    "--account-id",
                    ACCOUNT,
                    "--bucket",
                    BUCKET,
                    "--execute",
                    "--jobs",
                    str(jobs),
                ],
            )
            timings.append(perf_counter() - start)
        assert result.exit_code == 0, result.output
        summaries.append(json.loads(result.stdout))
        assert "Upload:" in result.stderr
        assert f"{len(loaded.members)}/" in result.stderr
        objects.append({k: (v.raw, v.headers) for k, v in fake.objects.items()})
        assert fake.peak <= jobs
        if jobs > 1:
            assert fake.peak > 1
        assert fake.requests[-2:] == [("PUT", INDEX), ("GET", INDEX)]
    assert summaries[0] == summaries[1]
    assert objects[0] == objects[1]
    assert summaries[0]["written_files"] == len(loaded.members)
    local = report(loaded)
    assert isinstance(local["image_bytes"], int)
    assert isinstance(local["json_bytes"], int)
    assert (
        sum(
            len(raw)
            for key, (raw, _) in objects[0].items()
            if key not in {INDEX, INVENTORY}
        )
        == (local["json_bytes"])
    )
    print(  # ruff: ignore[print] -- record acceptance timings without object contents
        f"20 ms/request: jobs=1 {timings[0]:.3f}s; jobs=16 {timings[1]:.3f}s"
    )


@pytest.mark.parametrize("failure", ["condition", "transport", "immutable"])
def test_failure_stops_waiting_writes_drains_and_rerun_only_fills_missing(
    images: PublicImages, roots: Roots, failure: str
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    failed = loaded.members[2]
    blocked_keys = {member.key for member in loaded.members[3:]}
    fake = DelayedS3(fail_key=failed.key, fault=failure, block_writes=blocked_keys)
    ready(fake, roots)
    stopped = Event()

    def progress(line: str) -> None:
        if "failed" in line:
            stopped.set()

    if failure == "immutable":
        fake.fail_key = None
        fake.objects[failed.key] = Stored(b"other", '"other"', {})
    with memory_store(fake) as store, ThreadPoolExecutor(max_workers=1) as runner:
        future = runner.submit(upload, store, loaded, jobs=4, progress=progress)
        try:
            assert stopped.wait(10)
            assert fake.blocked.wait(5)
            assert not future.done()
            assert INDEX not in fake.objects
        finally:
            fake.release.set()
        with pytest.raises(ExportError, match=failed.key):
            future.result(timeout=10)
        assert len(blocked_keys & fake.objects.keys()) <= 3
        assert INDEX not in fake.objects
        done = set(fake.objects) - {INVENTORY}
        fake.fail_key = None
        fake.block_writes.clear()
        if failure == "immutable":
            del fake.objects[failed.key]
            done.remove(failed.key)
        before = len(fake.requests)
        result = upload(store, loaded, jobs=16)
        new_writes = {key for method, key in fake.requests[before:] if method == "PUT"}
        assert done <= new_writes
        assert all(
            fake.objects[key].raw == next(m.raw for m in loaded.members if m.key == key)
            for key in done
        )
        assert failed.key in new_writes
        assert {
            key
            for method, key in fake.requests[before:]
            if method == "GET" and key not in {INDEX, INVENTORY}
        } == done
        assert result["unchanged_files"] == len(done)
        assert result["index"] == "updated"


@pytest.mark.parametrize("jobs", [0, -1])
def test_invalid_jobs_are_rejected_before_export_or_credentials(
    images: PublicImages, roots: Roots, jobs: int
) -> None:
    result = CliRunner().invoke(app, ["snapshot", "upload", "--jobs", str(jobs)])
    assert result.exit_code != 0
    assert "--jobs" in result.output
    assert "range" in result.output
    export(images, roots, step=0)
    fake = DelayedS3()
    with memory_store(fake) as store:
        with pytest.raises(ExportError, match="jobs must be at least 1"):
            upload(store, load_export(roots.preview), jobs=jobs)
    assert fake.requests == []


def test_progress_keeps_reporting_while_an_object_waits_for_put(
    images: PublicImages, roots: Roots, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(parallel, "PROGRESS_SECONDS", 0)
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    fake = DelayedS3(delay=0, block_write=loaded.members[0].key)
    ready(fake, roots)
    heartbeat = Event()

    def progress(line: str) -> None:
        if fake.blocked.is_set() and "complete" in line:
            heartbeat.set()

    with memory_store(fake) as store, ThreadPoolExecutor(max_workers=1) as runner:
        future = runner.submit(upload, store, loaded, jobs=1, progress=progress)
        try:
            assert fake.blocked.wait(5)
            assert heartbeat.wait(5)
            assert not future.done()
        finally:
            fake.release.set()
        assert future.result(timeout=10)["index"] == "updated"

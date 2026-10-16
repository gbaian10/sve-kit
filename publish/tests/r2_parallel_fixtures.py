"""Delayed in-memory S3 service for snapshot publication acceptance tests."""

from contextlib import contextmanager
from dataclasses import dataclass, field
from threading import Event, Lock
from time import sleep
from typing import TYPE_CHECKING, override

import httpx

from sve_publish.adapter import R2Store, Stored
from sve_publish.headers import INDEX_HEADERS
from sve_publish.inventory import INVENTORY, inventory

from .r2_export_fixtures import image_description
from .r2_fixtures import ACCOUNT, BUCKET, CREDENTIALS
from .r2_sdk_fixtures import mock_client

if TYPE_CHECKING:
    from collections.abc import Generator

    from sve_carddb.export.preview import Roots


@dataclass
class DelayedS3(httpx.BaseTransport):
    delay: float = 0.02
    objects: dict[str, Stored] = field(default_factory=dict)
    requests: list[tuple[str, str]] = field(default_factory=list)
    lock: Lock = field(default_factory=Lock)
    fail_key: str | None = None
    fault: str = "condition"
    block_writes: set[str] = field(default_factory=set)
    blocked: Event = field(default_factory=Event)
    release: Event = field(default_factory=Event)
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
            if method == "PUT" and key in self.block_writes:
                self.blocked.set()
                assert self.release.wait(10), "test did not release held write"
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
def memory_store(fake: DelayedS3) -> Generator[R2Store]:
    with httpx.Client(transport=fake, trust_env=False) as http:
        yield R2Store(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(http, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS),
        )


def ready(fake: DelayedS3, roots: Roots) -> None:
    """The independent image stage has already recorded its successful uploads."""
    fake.objects[INVENTORY] = Stored(
        inventory(image_description(roots.preview).objects).content(),
        '"ready-images"',
        INDEX_HEADERS,
    )

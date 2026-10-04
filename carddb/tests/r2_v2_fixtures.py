"""Loopback-only S3/CDN server with real HTTP and independent SigV4 checking."""

import hashlib
import hmac
from dataclasses import dataclass, field
from datetime import UTC, datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import RLock, Thread
from typing import TYPE_CHECKING, override
from urllib.parse import parse_qs, parse_qsl, quote, unquote, urlsplit
from xml.sax.saxutils import escape

import httpx
import pytest

from sve_carddb.r2_upload.s3 import Credentials
from sve_carddb.r2_upload.v2.adapter import R2Store
from sve_carddb.snapshot.publish.storage import Stored

if TYPE_CHECKING:
    from collections.abc import Iterator

ACCOUNT = "d" * 32
BUCKET = "synthetic-v2"
NOW = datetime(2026, 10, 4, tzinfo=UTC)
CREDENTIALS = Credentials("synthetic-access", "synthetic-secret")


@dataclass(repr=False)
class ServerState:
    objects: dict[str, Stored] = field(default_factory=dict)
    operations: list[tuple[str, str, str | None]] = field(default_factory=list)
    requests: list[tuple[str, str, dict[str, str]]] = field(default_factory=list)
    lock: RLock = field(default_factory=RLock)
    sequence: int = 0
    status: int | None = None
    fail_put: int | None = None
    cdn_status: int | None = None
    race_lease: bool = False
    page_size: int = 2
    cdn: bool = False
    error_body: bytes = b"synthetic-secret synthetic-access Signature=DO-NOT-LOG"


def reference_authorization(
    method: str, path: str, headers: dict[str, str], raw: bytes
) -> str:
    timestamp = headers["x-amz-date"]
    day = timestamp[:8]
    split = urlsplit(path)
    query = "&".join(
        k + "=" + v
        for k, v in sorted(
            (quote(k, safe="-_.~"), quote(v, safe="-_.~"))
            for k, v in parse_qsl(split.query, keep_blank_values=True)
        )
    )
    selected = {
        k: " ".join(v.split())
        for k, v in headers.items()
        if k not in {"authorization", "content-length"}
    }
    names = ";".join(sorted(selected))
    canonical_request = "\n".join(
        (
            method,
            split.path,
            query,
            "".join(k + ":" + selected[k] + "\n" for k in sorted(selected)),
            names,
            hashlib.sha256(raw).hexdigest(),
        )
    )
    scope = day + "/auto/s3/aws4_request"
    to_sign = "\n".join(
        (
            "AWS4-HMAC-SHA256",
            timestamp,
            scope,
            hashlib.sha256(canonical_request.encode()).hexdigest(),
        )
    )
    signing_key = ("AWS4" + CREDENTIALS.secret_key).encode()
    for value in (day, "auto", "s3", "aws4_request"):
        signing_key = hmac.digest(signing_key, value.encode(), "sha256")
    return (
        "AWS4-HMAC-SHA256 Credential="
        + CREDENTIALS.access_key
        + "/"
        + scope
        + ", SignedHeaders="
        + names
        + ", Signature="
        + hmac.new(signing_key, to_sign.encode(), "sha256").hexdigest()
    )


class LocalS3(ThreadingHTTPServer):
    def __init__(self, state: ServerState) -> None:
        self.state = state
        super().__init__(("127.0.0.1", 0), Handler)


class Handler(BaseHTTPRequestHandler):
    @property
    def state(self) -> ServerState:
        assert isinstance(self.server, LocalS3)
        return self.server.state

    @override
    def log_message(self, fmt: str, *args: object) -> None:
        pass

    def respond(
        self, status: int, raw: bytes = b"", headers: dict[str, str] | None = None
    ) -> None:
        self.send_response(status)
        for k, v in (headers or {}).items():
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(raw)))
        self.end_headers()
        self.wfile.write(raw)

    def dispatch(self) -> None:
        raw = self.rfile.read(int(self.headers.get("Content-Length", "0")))
        state = self.state
        headers = {k.lower(): v for k, v in self.headers.items()}
        with state.lock:
            state.requests.append(
                (
                    self.command,
                    self.path,
                    {k: v for k, v in headers.items() if k != "authorization"},
                )
            )
            cdn_request = state.cdn or self.headers.get("Host") == "cdn.invalid"
            if not cdn_request and headers.get(
                "authorization"
            ) != reference_authorization(self.command, self.path, headers, raw):
                self.respond(403, state.error_body)
                return
            if cdn_request and state.cdn_status is not None:
                self.respond(state.cdn_status, state.error_body)
                return
            if state.status is not None:
                self.respond(
                    state.status,
                    state.error_body,
                    {"location": "https://forbidden.invalid/"},
                )
                return
            split = urlsplit(self.path)
            key = (
                unquote(split.path).removeprefix("/" + BUCKET + "/")
                if not cdn_request
                else split.path.lstrip("/")
            )
            if self.command == "GET" and "list-type" in parse_qs(split.query):
                self.listing(parse_qs(split.query))
                return
            obj = state.objects.get(key)
            if self.command == "GET":
                if obj is None:
                    self.respond(404)
                else:
                    self.respond(200, obj.raw, obj.headers | {"etag": obj.etag})
            elif self.command == "PUT":
                self.put_object(key, raw, headers)
            else:
                self.respond(405)

    def put_object(self, key: str, raw: bytes, headers: dict[str, str]) -> None:
        state = self.state
        if state.race_lease and key == "coordination/snapshot-v2-writer.json":
            state.race_lease = False
            state.objects[key] = Stored(
                b"foreign active owner",
                '"foreign"',
                {"content-type": "application/json", "cache-control": "no-store"},
            )
        obj = state.objects.get(key)
        if state.fail_put is not None and not key.startswith("coordination/"):
            self.respond(state.fail_put, state.error_body)
            return
        condition = headers.get("if-match")
        state.operations.append(("PUT", key, condition or headers.get("if-none-match")))
        if (headers.get("if-none-match") == "*" and obj is not None) or (
            condition is not None and (obj is None or condition != obj.etag)
        ):
            self.respond(412, state.error_body)
        else:
            state.sequence += 1
            etag = f'"opaque-{state.sequence}"'
            state.objects[key] = Stored(
                raw,
                etag,
                {
                    k: headers[k]
                    for k in (
                        "content-type",
                        "cache-control",
                        "content-encoding",
                    )
                    if k in headers
                },
            )
            self.respond(200, headers={"etag": etag})

    def listing(self, query: dict[str, list[str]]) -> None:
        state = self.state
        prefix = query["prefix"][0]
        start = int(query.get("continuation-token", ["0"])[0])
        keys = sorted(k for k in state.objects if k.startswith(prefix))
        page = keys[start : start + state.page_size]
        next_value = start + len(page)
        truncated = next_value < len(keys)
        xml = (
            '<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Prefix>'
            + escape(prefix)
            + "</Prefix><IsTruncated>"
            + str(truncated).lower()
            + "</IsTruncated>"
        )
        xml += "".join(
            "<Contents><Key>" + escape(k) + "</Key></Contents>" for k in page
        )
        if truncated:
            xml += (
                "<NextContinuationToken>" + str(next_value) + "</NextContinuationToken>"
            )
        self.respond(200, (xml + "</ListBucketResult>").encode())

    do_GET = dispatch  # ruff: ignore[mixed-case-variable-in-class-scope] -- BaseHTTPRequestHandler method dispatch name
    do_PUT = dispatch  # ruff: ignore[mixed-case-variable-in-class-scope] -- BaseHTTPRequestHandler method dispatch name
    do_DELETE = dispatch  # ruff: ignore[mixed-case-variable-in-class-scope] -- BaseHTTPRequestHandler method dispatch name


class Loopback(httpx.BaseTransport):
    def __init__(self, root: str) -> None:
        self.root = root
        self.inner = httpx.HTTPTransport()
        self.lose_next_put = False
        self.lose_key: str | None = None

    @override
    def handle_request(self, request: httpx.Request) -> httpx.Response:
        target = self.root + request.url.raw_path.decode()
        local = httpx.Request(
            request.method, target, headers=request.headers, content=request.content
        )
        response = self.inner.handle_request(local)
        if request.method == "PUT" and (
            self.lose_next_put
            or request.url.path.endswith("/" + (self.lose_key or "\0"))
        ):
            self.lose_next_put = False
            self.lose_key = None
            response.close()
            raise httpx.ReadTimeout(
                "synthetic-secret Signature=DO-NOT-LOG", request=request
            )
        return response

    @override
    def close(self) -> None:
        self.inner.close()


@pytest.fixture
def server() -> Iterator[tuple[ServerState, Loopback]]:
    state = ServerState()
    server = LocalS3(state)
    thread = Thread(
        target=server.serve_forever, kwargs={"poll_interval": 0.01}, daemon=True
    )
    thread.start()
    transport = Loopback(f"http://127.0.0.1:{server.server_port}")
    try:
        yield state, transport
    finally:
        transport.close()
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)


@pytest.fixture
def remote(
    server: tuple[ServerState, Loopback],
) -> Iterator[tuple[R2Store, ServerState, Loopback]]:
    state, transport = server
    with httpx.Client(
        transport=transport, trust_env=False, follow_redirects=False
    ) as client:
        yield (
            R2Store(ACCOUNT, BUCKET, CREDENTIALS, client, lambda: NOW),
            state,
            transport,
        )

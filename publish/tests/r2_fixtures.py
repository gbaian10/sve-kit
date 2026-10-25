"""Loopback-only S3 server with real HTTP and independent SigV4 checking."""

import hashlib
import hmac
from dataclasses import dataclass, field
from datetime import UTC, datetime
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import RLock, Thread
from typing import TYPE_CHECKING, override
from urllib.parse import parse_qs, parse_qsl, quote, unquote, urlsplit
from xml.sax.saxutils import escape, unescape

import httpx
import pytest

from sve_publish.adapter import MAX_DELETE_BATCH, R2Store, Stored
from sve_publish.sdk import Credentials

from .r2_sdk_fixtures import mock_client

if TYPE_CHECKING:
    from collections.abc import Iterator

ACCOUNT = "d" * 32
BUCKET = "synthetic-v2"
NOW = datetime(2026, 10, 4, tzinfo=UTC)
CREDENTIALS = Credentials("synthetic-access", "synthetic-secret")
DELETE_TAIL = "<Quiet>true</Quiet></Delete>"


def delete_keys(text: str) -> list[str]:
    """Keys of a DeleteObjects body, which the SDK writes as a flat <Delete><Object><Key>…, then Quiet."""
    # Plain string steps keep this linear (no backtracking) and avoid a general XML parser on request data.
    opening, _, rest = text.strip().partition(">")
    assert opening.startswith("<Delete")
    assert rest.endswith(DELETE_TAIL)
    objects = rest.removesuffix(DELETE_TAIL).split("</Object>")
    assert not objects.pop()
    keys: list[str] = []
    for item in objects:
        assert item.startswith("<Object><Key>")
        assert item.endswith("</Key>")
        keys.append(unescape(item.removeprefix("<Object><Key>").removesuffix("</Key>")))
    return keys


@dataclass(repr=False)
class ServerState:
    objects: dict[str, Stored] = field(default_factory=dict)
    operations: list[tuple[str, str, str | None]] = field(default_factory=list)
    requests: list[tuple[str, str, dict[str, str]]] = field(default_factory=list)
    lock: RLock = field(default_factory=RLock)
    sequence: int = 0
    status: int | None = None
    fail_put: int | None = None
    put_statuses: list[int] = field(default_factory=list)
    fail_key: str | None = None
    delete_status: int | None = None
    delete_errors: set[str] = field(default_factory=set)
    delete_batches: list[list[str]] = field(default_factory=list)
    index_after_delete: Stored | None = None
    page_size: int = 2
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
    names = headers["authorization"].split("SignedHeaders=", 1)[1].split(",", 1)[0]
    selected = {k: " ".join(headers[k].split()) for k in names.split(";")}
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
    # The default listen backlog can reject a burst from sixteen upload workers.
    request_queue_size = 64

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
            if headers.get("authorization") != reference_authorization(
                self.command, self.path, headers, raw
            ):
                self.respond(403, state.error_body)
                return
            if state.status is not None:
                self.respond(
                    state.status,
                    state.error_body,
                    {"location": "https://forbidden.invalid/"},
                )
                return
            split = urlsplit(self.path)
            key = unquote(split.path).removeprefix("/" + BUCKET + "/")
            if self.command == "GET" and "list-type" in parse_qs(split.query):
                self.listing(parse_qs(split.query))
                return
            if self.command == "POST" and split.query == "delete":
                self.delete_objects(raw)
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

    def delete_objects(self, raw: bytes) -> None:
        state = self.state
        keys = delete_keys(raw.decode())
        assert 1 <= len(keys) <= MAX_DELETE_BATCH
        state.delete_batches.append(keys)
        state.operations.extend(("DELETE", key, None) for key in keys)
        if state.delete_status is not None:
            self.respond(state.delete_status, state.error_body)
            return
        errors = ""
        for key in keys:
            if key in state.delete_errors:
                errors += (
                    "<Error><Key>"
                    + escape(key)
                    + "</Key><Code>AccessDenied</Code><Message>synthetic-secret</Message></Error>"
                )
            else:
                state.objects.pop(key, None)
        if state.index_after_delete is not None:
            state.objects["snapshots/versions/index.json"] = state.index_after_delete
        self.respond(
            200,
            (
                '<DeleteResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">'
                + errors
                + "</DeleteResult>"
            ).encode(),
        )

    def put_object(self, key: str, raw: bytes, headers: dict[str, str]) -> None:
        state = self.state
        obj = state.objects.get(key)
        if state.put_statuses:
            self.respond(state.put_statuses.pop(0), state.error_body)
            return
        if state.fail_put is not None or key == state.fail_key:
            self.respond(state.fail_put or 500, state.error_body)
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
    do_POST = dispatch  # ruff: ignore[mixed-case-variable-in-class-scope] -- BaseHTTPRequestHandler method dispatch name


class Loopback(httpx.BaseTransport):
    def __init__(self, root: str) -> None:
        self.root = root
        self.inner = httpx.HTTPTransport()
        self.lose_next_put = False
        self.lose_key: str | None = None
        self.reject_put_key: str | None = None
        self.reject_put_number = 0
        self.selected_puts = 0

    @override
    def handle_request(self, request: httpx.Request) -> httpx.Response:
        if request.method == "PUT" and request.url.path.endswith(
            "/" + (self.reject_put_key or "\0")
        ):
            self.selected_puts += 1
            if self.selected_puts == self.reject_put_number:
                return httpx.Response(412, stream=httpx.ByteStream(b""))
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
            R2Store(
                ACCOUNT,
                BUCKET,
                CREDENTIALS,
                mock_client(
                    client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
                ),
            ),
            state,
            transport,
        )

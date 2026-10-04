"""R2 S3 conditional boundary with a non-expiring deployment-wide CAS lease."""

import re
from contextlib import contextmanager
from dataclasses import dataclass, field
from datetime import UTC, datetime
from http import HTTPStatus
from typing import TYPE_CHECKING
from urllib.parse import quote
from uuid import uuid4
from xml.etree import ElementTree as ET  # ruff: ignore[suspicious-xml-etree-import] -- UTF-8-only bounded input rejects all declarations/entities before parse

import httpx

from sve_carddb.r2_upload.s3 import Credentials, sign
from sve_carddb.snapshot.publish.storage import PublishError, Stored
from sve_carddb.snapshot.values import canonical

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator

LEASE_KEY = "coordination/snapshot-v2-writer.json"
LEASE_HEADERS = {"content-type": "application/json", "cache-control": "no-store"}
PUBLIC_PREFIXES = frozenset(
    {"snapshots/blobs/", "snapshots/manifests/"}
    | {"images/" + s + "/" for s in ("card_s", "card_m", "card_l", "art_s", "art_m")}
)
MAX_OBJECT = 128 * 1024 * 1024
MAX_LIST = 2 * 1024 * 1024


def body(response: httpx.Response, limit: int) -> bytes:
    """Preserve stored compressed siblings instead of HTTP-decoding their bytes."""
    result = bytearray()
    for chunk in response.iter_raw():
        result.extend(chunk)
        if len(result) > limit:
            raise PublishError("Remote response exceeds the configured byte limit")
    return bytes(result)


@dataclass(repr=False)
class R2Store:
    """Single bucket boundary; inject a transport rather than a different endpoint."""

    account_id: str
    bucket: str
    credentials: Credentials
    client: httpx.Client
    clock: Callable[[], datetime] = field(default=lambda: datetime.now(UTC))
    _lease: Stored | None = field(default=None, init=False)

    def __post_init__(self) -> None:
        """Restrict signed requests to one explicitly selected R2 endpoint."""
        if not re.fullmatch(r"[0-9a-f]{32}", self.account_id):
            raise PublishError("Invalid explicit R2 account ID")
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,61}[a-z0-9]", self.bucket):
            raise PublishError("Invalid explicit R2 bucket")

    def _request(
        self,
        method: str,
        key: str,
        raw: bytes = b"",
        headers: dict[str, str] | None = None,
        params: dict[str, str] | None = None,
    ) -> httpx.Request:
        if (
            key.startswith("/")
            or any(p in {".", ".."} for p in key.split("/"))
            or "\\" in key
            or "\0" in key
        ):
            raise PublishError("Invalid S3 object key")
        request = httpx.Request(
            method,
            f"https://{self.account_id}.r2.cloudflarestorage.com/{self.bucket}/"
            + quote(key, safe="/"),
            params=params,
            headers=(headers or {}) | {"accept-encoding": "identity"},
            content=raw,
        )
        sign(request, self.credentials, self.clock())
        return request

    @contextmanager
    def _response(self, request: httpx.Request) -> Iterator[httpx.Response]:
        try:
            response = self.client.send(
                request, auth=None, stream=True, follow_redirects=False
            )
        except httpx.HTTPError, ValueError:
            raise PublishError("R2 transport or protocol failed") from None
        try:
            yield response
        except httpx.HTTPError:
            raise PublishError("R2 transport or protocol failed") from None
        finally:
            response.close()

    def get(self, key: str) -> Stored | None:
        """Read exact raw bytes, opaque ETag and only contractual content metadata."""
        with self._response(self._request("GET", key)) as response:
            if response.status_code == HTTPStatus.NOT_FOUND:
                return None
            if response.status_code != HTTPStatus.OK:
                raise PublishError("R2 object read failed")
            etag = response.headers.get("etag", "")
            if not etag or etag.startswith("W/"):
                raise PublishError("R2 object requires a strong opaque ETag")
            metadata = {
                k: response.headers[k]
                for k in ("content-type", "cache-control", "content-encoding")
                if k in response.headers
            }
            return Stored(body(response, MAX_OBJECT), etag, metadata)

    def put(
        self, key: str, raw: bytes, headers: dict[str, str], *, expected: str | None
    ) -> bool:
        """Never retry writes or downgrade an unsupported condition to a plain PUT."""
        if key != LEASE_KEY:
            self._verify_lease()
        if set(headers) - {"content-type", "cache-control", "content-encoding"}:
            raise PublishError("Unsupported object metadata")
        if expected is not None and (
            not expected or expected == "*" or expected.startswith("W/")
        ):
            raise PublishError("Conditional PUT requires an opaque object ETag")
        condition = (
            {"if-none-match": "*"} if expected is None else {"if-match": expected}
        )
        with self._response(
            self._request("PUT", key, raw, headers | condition)
        ) as response:
            if response.status_code == HTTPStatus.PRECONDITION_FAILED:
                return False
            if response.status_code not in {
                HTTPStatus.OK,
                HTTPStatus.CREATED,
                HTTPStatus.NO_CONTENT,
            }:
                raise PublishError(
                    "R2 conditional PUT failed; no unconditional fallback"
                )
            return True

    @staticmethod
    def delete(key: str, *, expected: str) -> bool:
        """R2's compatibility table does not promise atomic If-Match on DELETE."""
        if not expected or not any(key.startswith(p) for p in PUBLIC_PREFIXES):
            raise PublishError("Invalid conditional deletion request")
        raise PublishError(
            "R2 conditional DELETE is unverified; collection is disabled"
        )

    def keys(self, prefix: str) -> tuple[str, ...]:
        """Bounded ListObjectsV2 pagination only in explicitly public namespaces."""
        if prefix not in PUBLIC_PREFIXES:
            raise PublishError("List prefix is not explicitly public")
        keys: set[str] = set()
        seen: set[str] = set()
        token = None
        while True:
            params = {"list-type": "2", "prefix": prefix, "max-keys": "1000"}
            if token is not None:
                params["continuation-token"] = token
            with self._response(self._request("GET", "", params=params)) as response:
                if response.status_code != HTTPStatus.OK:
                    raise PublishError("R2 public inventory read failed")
                raw = body(response, MAX_LIST)
            page, token = _page(raw, prefix)
            if keys.intersection(page):
                raise PublishError("R2 inventory contains repeated keys")
            keys.update(page)
            if token is None:
                return tuple(sorted(keys))
            if token in seen:
                raise PublishError("R2 inventory pagination repeats a token")
            seen.add(token)

    def _verify_lease(self) -> None:
        if self._lease is None or self.get(LEASE_KEY) != self._lease:
            raise PublishError("Deployment writer lease changed; stop and review")

    @contextmanager
    def exclusive(self) -> Iterator[None]:
        """All machines share a CAS lock; crashes never authorize timed takeover.

        A stuck active record needs separate operator recovery after proving all
        writers stopped. Unlock uses conditional PUT, never unsupported DELETE.
        """
        if self._lease is not None:
            raise PublishError(
                "Deployment writer lease is already held by this adapter"
            )
        old = self.get(LEASE_KEY)
        if old is not None and (
            old.raw != canonical({"format": 1, "owner": None})
            or old.headers != LEASE_HEADERS
        ):
            raise PublishError(
                "Deployment writer lease is active or invalid; operator recovery required"
            )
        raw = canonical({"format": 1, "owner": uuid4().hex})
        if not self.put(
            LEASE_KEY, raw, LEASE_HEADERS, expected=None if old is None else old.etag
        ):
            raise PublishError("Deployment writer lease was acquired concurrently")
        acquired = self.get(LEASE_KEY)
        if acquired is None or acquired.raw != raw or acquired.headers != LEASE_HEADERS:
            raise PublishError("Deployment writer lease acquisition is uncertain")
        self._lease = acquired
        try:
            yield
        finally:
            try:
                self._verify_lease()
                if not self.put(
                    LEASE_KEY,
                    canonical({"format": 1, "owner": None}),
                    LEASE_HEADERS,
                    expected=acquired.etag,
                ):
                    raise PublishError("Deployment writer lease release failed")
            finally:
                self._lease = None


def _page(raw: bytes, prefix: str) -> tuple[set[str], str | None]:
    try:
        text = raw.decode("utf-8")
    except UnicodeError:
        raise PublishError("R2 inventory XML must be UTF-8") from None
    if "<!" in text:
        raise PublishError("R2 inventory XML declarations are forbidden")
    try:
        root = ET.fromstring(text)  # ruff: ignore[suspicious-xml-element-tree-usage] -- UTF-8 only; all DTD/entity declarations rejected above
    except ET.ParseError:
        raise PublishError("R2 inventory XML is invalid") from None
    ns = "{http://s3.amazonaws.com/doc/2006-03-01/}"
    if root.tag != ns + "ListBucketResult" or root.findtext(ns + "Prefix") != prefix:
        raise PublishError("R2 inventory prefix differs from request")
    rows = root.findall(ns + "Contents")
    result = {r.findtext(ns + "Key", "") for r in rows}
    if len(result) != len(rows) or any(not k.startswith(prefix) for k in result):
        raise PublishError("R2 inventory contains invalid keys")
    flag = root.findtext(ns + "IsTruncated")
    if flag not in {"true", "false"}:
        raise PublishError("R2 inventory lacks a truncation flag")
    token = root.findtext(ns + "NextContinuationToken")
    if flag == "true" and not token:
        raise PublishError("R2 inventory lacks a continuation token")
    return result, token if flag == "true" else None

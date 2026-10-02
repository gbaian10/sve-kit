"""Small conditional-object S3 boundary with SigV4 and no ambient credentials."""

import hashlib
import hmac
import os
import re
import time
from dataclasses import dataclass, field
from datetime import UTC, datetime
from http import HTTPStatus
from typing import TYPE_CHECKING
from urllib.parse import quote

import httpx

from sve_carddb.r2_upload.plan import UploadError

if TYPE_CHECKING:
    from collections.abc import Callable


@dataclass(frozen=True, repr=False)
class Credentials:
    """Local operator credentials are never included in reports or representations."""

    access_key: str
    secret_key: str

    @classmethod
    def environment(cls) -> Credentials:
        """Read only explicit R2 variables, never a profile or credential file."""
        access = os.environ.get("SVE_R2_ACCESS_KEY_ID", "")
        secret = os.environ.get("SVE_R2_SECRET_ACCESS_KEY", "")
        if not access or not secret:
            raise UploadError("Explicit local R2 credentials are required")
        return cls(access, secret)


def sign(
    request: httpx.Request,
    credentials: Credentials,
    now: datetime,
    *,
    region: str = "auto",
) -> None:
    """Sign the exact conditional headers and payload sent to the R2 S3 endpoint."""
    timestamp = now.astimezone(UTC).strftime("%Y%m%dT%H%M%SZ")
    day = timestamp[:8]
    hashed = hashlib.sha256(request.content).hexdigest()
    request.headers["x-amz-date"] = timestamp
    request.headers["x-amz-content-sha256"] = hashed
    headers = {
        key.lower(): " ".join(value.split())
        for key, value in request.headers.items()
        if key.lower() not in {"authorization", "content-length"}
    }
    names = ";".join(sorted(headers))
    canonical = "\n".join(
        (
            request.method,
            request.url.raw_path.decode("ascii"),
            "",
            "".join(key + ":" + headers[key] + "\n" for key in sorted(headers)),
            names,
            hashed,
        )
    )
    scope = day + "/" + region + "/s3/aws4_request"
    signing = "\n".join(
        (
            "AWS4-HMAC-SHA256",
            timestamp,
            scope,
            hashlib.sha256(canonical.encode()).hexdigest(),
        )
    )
    key = ("AWS4" + credentials.secret_key).encode()
    for part in (day, region, "s3", "aws4_request"):
        key = hmac.digest(key, part.encode(), "sha256")
    signature = hmac.new(key, signing.encode(), "sha256").hexdigest()
    request.headers["authorization"] = (
        "AWS4-HMAC-SHA256 Credential="
        + credentials.access_key
        + "/"
        + scope
        + ", SignedHeaders="
        + names
        + ", Signature="
        + signature
    )


@dataclass(frozen=True)
class Remote:
    """Verified raw response bytes and object metadata; never an error body."""

    raw: bytes
    headers: httpx.Headers


@dataclass
class S3:
    """Injected client permits synthetic tests without a real connection."""

    account_id: str
    bucket: str
    credentials: Credentials = field(repr=False)
    client: httpx.Client = field(repr=False)
    clock: Callable[[], datetime] = field(default=lambda: datetime.now(UTC), repr=False)
    wait: Callable[[float], None] = field(default=time.sleep, repr=False)

    def __post_init__(self) -> None:
        """Restrict requests to an explicit account and bucket."""
        if not re.fullmatch(r"[0-9a-f]{32}", self.account_id):
            raise UploadError(
                "R2 account ID must be 32 lowercase hexadecimal characters"
            )
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,61}[a-z0-9]", self.bucket):
            raise UploadError("Invalid R2 bucket name")

    def _request(
        self, method: str, key: str, raw: bytes, headers: dict[str, str]
    ) -> httpx.Request:
        if key.startswith("/") or ".." in key.split("/"):
            raise UploadError("Invalid object key")
        url = (
            "https://"
            + self.account_id
            + ".r2.cloudflarestorage.com/"
            + self.bucket
            + "/"
            + quote(key, safe="/")
        )
        request = httpx.Request(
            method, url, headers=headers | {"accept-encoding": "identity"}, content=raw
        )
        sign(request, self.credentials, self.clock())
        return request

    def get(self, key: str, *, limit: int) -> Remote | None:
        """Read bounded identity bytes; redirects and encoded responses fail closed."""
        delays = (0.5, 1.0)
        for attempt in range(3):
            try:
                return self._get_once(key, limit=limit)
            except (
                httpx.TimeoutException,
                httpx.NetworkError,
                httpx.RemoteProtocolError,
            ):
                if attempt == len(delays):
                    break
                self.wait(delays[attempt])
            except httpx.HTTPError:
                break
        raise UploadError("R2 transport failed") from None

    def _get_once(self, key: str, *, limit: int) -> Remote | None:
        response = self.client.send(
            self._request("GET", key, b"", {}), stream=True, follow_redirects=False
        )
        try:
            if response.status_code == HTTPStatus.NOT_FOUND:
                return None
            if response.status_code != HTTPStatus.OK:
                raise UploadError("R2 object read failed")
            if response.headers.get("content-encoding", "identity") != "identity":
                raise UploadError("R2 object has unexpected content encoding")
            chunks = bytearray()
            for chunk in response.iter_bytes():
                chunks.extend(chunk)
                if len(chunks) > limit:
                    raise UploadError("R2 object exceeds expected size")
            return Remote(bytes(chunks), response.headers)
        finally:
            response.close()

    def put(self, key: str, raw: bytes, headers: dict[str, str]) -> bool:
        """Return false only for a failed precondition; never retry a write implicitly."""
        try:
            response = self.client.send(
                self._request("PUT", key, raw, headers),
                stream=True,
                follow_redirects=False,
            )
            try:
                if response.status_code == HTTPStatus.PRECONDITION_FAILED:
                    return False
                if response.status_code not in {
                    HTTPStatus.OK,
                    HTTPStatus.CREATED,
                    HTTPStatus.NO_CONTENT,
                }:
                    raise UploadError("R2 conditional object write failed")
                return True
            finally:
                response.close()
        except httpx.HTTPError:
            raise UploadError("R2 transport failed") from None

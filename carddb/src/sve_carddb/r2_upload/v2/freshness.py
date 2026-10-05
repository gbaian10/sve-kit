"""Unsigned, ordinary full-query CDN GET without redirects, bypass or purge."""

from dataclasses import dataclass
from http import HTTPStatus
from urllib.parse import urlsplit

import httpx

from sve_carddb.r2_upload.boundary import UploadError
from sve_carddb.r2_upload.v2.adapter import MAX_OBJECT
from sve_carddb.r2_upload.v2.export import IMAGE_KEY


def body(response: httpx.Response, limit: int) -> bytes:
    """Preserve stored compressed siblings instead of HTTP-decoding their bytes."""
    result = bytearray()
    for chunk in response.iter_raw():
        result.extend(chunk)
        if len(result) > limit:
            raise UploadError("Remote response exceeds the configured byte limit")
    return bytes(result)


def cdn_root(value: str) -> str:
    """Pin the operator-selected HTTPS root before network or credential access."""
    try:
        split = urlsplit(value)
        if split.query or split.fragment:
            raise UploadError("Explicit HTTPS CDN root required")
        if (
            split.scheme != "https"
            or not split.netloc
            or split.username
            or split.password
            or not value.endswith("/")
        ):
            raise UploadError("Explicit HTTPS CDN root required")
        _ = split.port
    except ValueError:
        raise UploadError("Explicit HTTPS CDN root required") from None
    return value


@dataclass(repr=False)
class CDNFreshness:
    root: str
    client: httpx.Client

    def __post_init__(self) -> None:
        """Pin the unsigned CDN client to the explicit HTTPS root."""
        cdn_root(self.root)

    def get(self, url: str) -> bytes | None:
        """The uploader compares returned full bytes against the exported image."""
        split = urlsplit(url)
        suffix = url.removeprefix(self.root)
        if (
            not url.startswith(self.root)
            or not IMAGE_KEY.fullmatch(suffix.split("?", 1)[0])
            or split.fragment
            or not split.query
        ):
            raise UploadError("CDN URL is outside the pinned query-bearing root")
        try:
            request = httpx.Request("GET", url, headers={"accept-encoding": "identity"})
            response = self.client.send(
                request, auth=None, stream=True, follow_redirects=False
            )
            try:
                if (
                    response.status_code != HTTPStatus.OK
                    or response.headers.get("content-encoding", "identity")
                    != "identity"
                ):
                    return None
                return body(response, MAX_OBJECT)
            finally:
                response.close()
        except httpx.HTTPError, ValueError:
            raise UploadError("CDN transport or protocol failed") from None

"""Unsigned, ordinary full-query CDN GET without redirects, bypass or purge."""

from dataclasses import dataclass
from http import HTTPStatus
from urllib.parse import urlsplit

import httpx

from sve_carddb.r2_upload.v2.adapter import MAX_OBJECT, body
from sve_carddb.snapshot.publish.plan import IMAGE_KEY
from sve_carddb.snapshot.publish.storage import PublishError


def cdn_root(value: str) -> str:
    """Pin the operator-selected HTTPS root before network or credential access."""
    try:
        split = urlsplit(value)
        if split.query or split.fragment:
            raise PublishError("Explicit HTTPS CDN root required")
        if (
            split.scheme != "https"
            or not split.netloc
            or split.username
            or split.password
            or not value.endswith("/")
        ):
            raise PublishError("Explicit HTTPS CDN root required")
        _ = split.port
    except ValueError:
        raise PublishError("Explicit HTTPS CDN root required") from None
    return value


@dataclass(repr=False)
class CDNFreshness:
    root: str
    client: httpx.Client

    def __post_init__(self) -> None:
        """Pin the unsigned CDN client to the explicit HTTPS root."""
        cdn_root(self.root)

    def get(self, url: str) -> bytes | None:
        """The publisher compares returned full bytes against the sealed SHA/size."""
        split = urlsplit(url)
        suffix = url.removeprefix(self.root)
        if (
            not url.startswith(self.root)
            or not IMAGE_KEY.fullmatch(suffix.split("?", 1)[0])
            or split.fragment
            or not split.query
        ):
            raise PublishError("CDN URL is outside the pinned query-bearing root")
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
            raise PublishError("CDN transport or protocol failed") from None

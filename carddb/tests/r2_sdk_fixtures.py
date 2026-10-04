"""Exercise real SDK signing/serialization/parsing over the existing fake services."""

import io
from contextlib import ExitStack
from typing import TYPE_CHECKING, override

import httpx
import pytest
from botocore.awsrequest import AWSPreparedRequest, AWSRequest, AWSResponse
from botocore.compat import HTTPHeaders
from botocore.exceptions import (
    BotoCoreError,
    ConnectionClosedError,
    EndpointConnectionError,
    ReadTimeoutError,
)
from botocore.httpsession import URLLib3Session

from sve_carddb.r2_upload import sdk
from sve_carddb.r2_upload.sdk import Credentials, sdk_client
from sve_carddb.r2_upload.v2.adapter import R2Store

if TYPE_CHECKING:
    from collections.abc import Iterator

    from types_boto3_s3 import S3Client

CLIENTS = ExitStack()


@pytest.fixture
def close_sdk_clients() -> Iterator[None]:
    try:
        yield
    finally:
        CLIENTS.close()


class Raw:
    def __init__(self, response: httpx.Response) -> None:
        self.response = response
        self.raw = io.BytesIO(
            response.content
            if response.is_stream_consumed
            else b"".join(response.iter_raw())
        )

    def read(self, amt: int = -1) -> bytes:
        return self.raw.read(amt)

    def stream(
        self, amt: int = 64 * 1024, *, decode_content: bool = False
    ) -> Iterator[bytes]:
        assert not decode_content
        while chunk := self.read(amt):
            yield chunk

    def close(self) -> None:
        self.response.close()


class MockHTTP(URLLib3Session):
    def __init__(self, client: httpx.Client) -> None:
        self.client = client

    @override
    def close(self) -> None:
        self.client.close()

    @override
    def send(self, request: AWSRequest | AWSPreparedRequest) -> AWSResponse:
        assert isinstance(request, AWSPreparedRequest)
        headers = {}
        for name in request.headers:
            value: object = request.headers[name]
            assert isinstance(value, str | bytes)
            headers[name] = value.decode() if isinstance(value, bytes) else value
        raw = request.body
        if hasattr(raw, "read"):
            assert isinstance(raw, io.BytesIO)
            raw = raw.read()
        assert isinstance(raw, bytes) or raw is None
        outgoing = httpx.Request(
            request.method, request.url, headers=headers, content=raw
        )
        try:
            response = self.client.send(outgoing, stream=True, follow_redirects=False)
        except httpx.ReadTimeout:
            raise ReadTimeoutError(endpoint_url=request.url) from None
        except httpx.ConnectError:
            raise EndpointConnectionError(endpoint_url=request.url) from None
        except httpx.RemoteProtocolError:
            raise ConnectionClosedError(endpoint_url=request.url) from None
        except httpx.HTTPError:
            raise BotoCoreError from None
        result_headers = HTTPHeaders()
        for name, value in response.headers.items():
            result_headers[name] = value
        return AWSResponse(
            request.url, response.status_code, result_headers, Raw(response)
        )


def mock_client(
    client: httpx.Client, *, account: str, bucket: str, credentials: Credentials
) -> S3Client:
    return CLIENTS.enter_context(
        sdk_client(account, bucket, credentials, http_session=MockHTTP(client))
    )


def inventory(raw: bytes, prefix: str) -> None:
    with httpx.Client(
        transport=httpx.MockTransport(lambda _r: httpx.Response(200, content=raw))
    ) as http:
        client = mock_client(
            http,
            account="d" * 32,
            bucket="synthetic-v2",
            credentials=Credentials("synthetic-access", "synthetic-secret"),
        )
        R2Store(
            "d" * 32,
            "synthetic-v2",
            Credentials("synthetic-access", "synthetic-secret"),
            client,
        ).keys(prefix)


def install_mock_sdk(
    monkeypatch: pytest.MonkeyPatch, transport: httpx.BaseTransport
) -> None:
    original = sdk.Transport

    def factory(account: str, bucket: str) -> sdk.Transport:
        return original(
            account,
            bucket,
            MockHTTP(
                httpx.Client(
                    transport=transport,
                    trust_env=False,
                    follow_redirects=False,
                    timeout=30,
                )
            ),
        )

    monkeypatch.setattr(sdk, "Transport", factory)

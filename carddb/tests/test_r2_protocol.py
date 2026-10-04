"""Published AWS vectors and bounded, GET-only transient retries."""

import httpx
import pytest

from sve_carddb.r2_upload.plan import UploadError
from sve_carddb.r2_upload.s3 import S3, Credentials

from .r2_sdk_fixtures import mock_client
from .r2_upload_fixtures import ACCOUNT, BUCKET, CREDENTIALS

pytestmark = pytest.mark.usefixtures("close_sdk_clients")


def test_r2_endpoint_is_the_explicit_https_account_bucket() -> None:
    requests: list[httpx.Request] = []

    def handle(request: httpx.Request) -> httpx.Response:
        requests.append(request)
        return httpx.Response(404)

    with httpx.Client(transport=httpx.MockTransport(handle)) as client:
        remote = S3(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            ),
        )
        assert remote.get("snapshots/preview/current.json", limit=4096) is None
    assert len(requests) == 1
    assert (
        str(requests[0].url)
        == f"https://{ACCOUNT}.r2.cloudflarestorage.com/{BUCKET}/snapshots/preview/current.json"
    )


@pytest.mark.parametrize(
    "error", [httpx.ReadTimeout, httpx.ConnectError, httpx.RemoteProtocolError]
)
@pytest.mark.parametrize("recover", [True, False])
def test_transient_get_retries_are_finite_and_backoff_is_bounded(
    error: type[httpx.RequestError], recover: bool
) -> None:
    calls = []
    waits: list[float] = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        if recover and len(calls) == 3:
            return httpx.Response(200, content=b"{}")
        raise error("synthetic transient", request=request)

    with httpx.Client(transport=httpx.MockTransport(handler)) as client:
        remote = S3(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            ),
            waits.append,
        )
        if recover:
            result = remote.get("key", limit=2)
            assert result is not None
            assert result.raw == b"{}"
        else:
            with pytest.raises(UploadError, match=r"^R2 transport failed$"):
                remote.get("key", limit=2)
    assert len(calls) == 3
    assert waits == [0.5, 1.0]
    assert all(request.method == "GET" for request in calls)


@pytest.mark.parametrize("status", [302, 403, 429, 500, 503])
def test_http_status_failures_are_not_transport_retries(status: int) -> None:
    calls: list[httpx.Request] = []
    waits: list[float] = []

    def handle(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(status)

    with httpx.Client(transport=httpx.MockTransport(handle)) as client:
        remote = S3(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            ),
            waits.append,
        )
        with pytest.raises(UploadError, match=r"^R2 object read failed$"):
            remote.get("key", limit=2)
    assert len(calls) == 1
    assert waits == []


def test_non_retryable_local_protocol_error_is_not_retried() -> None:
    calls = []
    waits: list[float] = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        raise httpx.LocalProtocolError("synthetic local error", request=request)

    with httpx.Client(transport=httpx.MockTransport(handler)) as client:
        remote = S3(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            ),
            waits.append,
        )
        with pytest.raises(UploadError, match=r"^R2 transport failed$"):
            remote.get("key", limit=2)
    assert len(calls) == 1
    assert waits == []


def test_put_with_lost_response_is_never_retried_or_made_unconditional() -> None:
    calls = []
    waits: list[float] = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        raise httpx.ReadTimeout("synthetic lost response", request=request)

    with httpx.Client(transport=httpx.MockTransport(handler)) as client:
        remote = S3(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            ),
            waits.append,
        )
        with pytest.raises(UploadError, match=r"^R2 transport failed$"):
            remote.put("key", b"{}", {"if-none-match": "*"})
    assert len(calls) == 1
    assert waits == []
    assert calls[0].headers["if-none-match"] == "*"


@pytest.mark.parametrize(
    "missing", ["SVE_R2_ACCESS_KEY_ID", "SVE_R2_SECRET_ACCESS_KEY"]
)
def test_ambient_aws_credentials_never_complete_a_partial_r2_pair(
    monkeypatch: pytest.MonkeyPatch, missing: str
) -> None:
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-explicit-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-explicit-secret")
    monkeypatch.delenv(missing)
    monkeypatch.setenv("AWS_ACCESS_KEY_ID", "synthetic-fallback-access")
    monkeypatch.setenv("AWS_SECRET_ACCESS_KEY", "synthetic-fallback-secret")
    with pytest.raises(
        UploadError, match=r"^Explicit local R2 credentials are required$"
    ):
        Credentials.environment()

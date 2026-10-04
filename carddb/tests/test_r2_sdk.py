"""Typed SDK, native loopback transport and hostile ambient configuration."""

import gzip
import logging
import os
import sys
from functools import partial
from pathlib import Path
from typing import TYPE_CHECKING, override
from urllib.parse import urlsplit

import httpx
import pytest
from botocore.awsrequest import AWSPreparedRequest, AWSRequest, AWSResponse
from botocore.exceptions import ReadTimeoutError
from botocore.httpsession import URLLib3Session
from botocore.stub import Stubber

from sve_carddb.r2_upload import sdk
from sve_carddb.r2_upload.boundary import UploadError
from sve_carddb.r2_upload.sdk import BoundaryError, Credentials, bounded, sdk_client
from sve_carddb.r2_upload.v2.adapter import LEASE_KEY, R2Store
from sve_carddb.snapshot.publish.storage import PublishError, Stored

from .r2_sdk_fixtures import MockHTTP, mock_client
from .r2_v2_fixtures import ACCOUNT, BUCKET, CREDENTIALS
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- register independent loopback server

if TYPE_CHECKING:
    from .r2_v2_fixtures import Loopback, ServerState

pytestmark = pytest.mark.usefixtures("close_sdk_clients")


class NativeLoopback(URLLib3Session):
    def __init__(self, root: str) -> None:
        super().__init__(verify=True, proxies={}, timeout=30)
        self.root = root
        self.lose_put = False

    @override
    def send(self, request: AWSRequest | AWSPreparedRequest) -> AWSResponse:
        assert isinstance(request, AWSPreparedRequest)
        original = urlsplit(request.url)
        request.headers["Host"] = original.netloc
        request.url = (
            self.root + original.path + ("?" + original.query if original.query else "")
        )
        response = super().send(request)
        if self.lose_put and request.method == "PUT":
            self.lose_put = False
            raw: object = response.raw
            assert isinstance(raw, sdk.RawBody)
            raw.close()
            raise ReadTimeoutError(endpoint_url=request.url)
        return response


def test_native_sdk_keeps_conditions_and_exact_compressed_bytes(
    server: tuple[ServerState, Loopback],
) -> None:
    state, loopback = server
    native = NativeLoopback(loopback.root)
    with sdk_client(ACCOUNT, BUCKET, CREDENTIALS, http_session=native) as client:
        remote = R2Store(ACCOUNT, BUCKET, CREDENTIALS, client)
        headers = {"content-type": "application/json", "cache-control": "no-store"}
        with remote.exclusive():
            assert remote.put("synthetic.json", b"{}", headers, expected=None)
            assert not remote.put("synthetic.json", b"changed", headers, expected=None)
            first = remote.get("synthetic.json")
            assert first is not None
            assert remote.put("synthetic.json", b"new", headers, expected=first.etag)
        raw = gzip.compress(b"synthetic compressed sibling", mtime=0)
        state.objects["synthetic.json.gz"] = Stored(
            raw,
            '"opaque-gzip"',
            {
                "content-encoding": "gzip",
                "x-amz-meta-sha256": "synthetic-extra-metadata",
            },
        )
        assert R2Store(ACCOUNT, BUCKET, CREDENTIALS, client).get(
            "synthetic.json.gz"
        ) == Stored(raw, '"opaque-gzip"', {"content-encoding": "gzip"})
        retries: object = vars(client.meta.config).get("retries")
        assert retries == {
            "total_max_attempts": 1,
            "mode": "standard",
        }


def test_native_sdk_lost_write_response_is_not_replayed(
    server: tuple[ServerState, Loopback],
) -> None:
    state, loopback = server
    native = NativeLoopback(loopback.root)
    native.lose_put = True
    with sdk_client(ACCOUNT, BUCKET, CREDENTIALS, http_session=native) as client:
        with pytest.raises(PublishError, match=r"^R2 transport or protocol failed$"):
            R2Store(ACCOUNT, BUCKET, CREDENTIALS, client).put(
                LEASE_KEY,
                b"{}",
                {"content-type": "application/json"},
                expected=None,
            )
    assert len(state.requests) == 1
    assert state.objects[LEASE_KEY].raw == b"{}"


def test_typed_stubber_checks_both_put_conditions_and_metadata() -> None:
    with sdk_client(ACCOUNT, BUCKET, CREDENTIALS) as client, Stubber(client) as stub:
        expected = {
            "Bucket": BUCKET,
            "Key": LEASE_KEY,
            "Body": b"{}",
            "ContentType": "application/json",
            "CacheControl": "no-store",
        }
        stub.add_response("put_object", {}, expected | {"IfNoneMatch": "*"})
        stub.add_client_error(
            "put_object",
            "PreconditionFailed",
            http_status_code=412,
            expected_params=expected | {"IfMatch": '"opaque"'},
        )
        remote = R2Store(ACCOUNT, BUCKET, CREDENTIALS, client)
        headers = {"content-type": "application/json", "cache-control": "no-store"}
        assert remote.put(LEASE_KEY, b"{}", headers, expected=None)
        assert not remote.put(LEASE_KEY, b"{}", headers, expected='"opaque"')
        stub.assert_no_pending_responses()


def test_aws_environment_and_files_cannot_reconfigure_the_sdk(
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    forbidden = tmp_path / "ambient-aws"
    forbidden.mkdir()
    config = forbidden / "config"
    config.write_text("[profile ambient]\ncredential_process = forbidden-command\n")
    for name, value in {
        "AWS_PROFILE": "ambient",
        "AWS_DEFAULT_PROFILE": "ambient",
        "AWS_CONFIG_FILE": str(config),
        "AWS_SHARED_CREDENTIALS_FILE": str(config),
        "AWS_DATA_PATH": str(forbidden),
        "AWS_CA_BUNDLE": str(config),
        "AWS_ENDPOINT_URL": "https://foreign.invalid",
        "AWS_ENDPOINT_URL_S3": "https://foreign.invalid",
        "AWS_ACCESS_KEY_ID": "ambient-access",
        "AWS_SECRET_ACCESS_KEY": "ambient-secret",
        "AWS_SESSION_TOKEN": "ambient-token",
        "AWS_DEFAULT_REGION": "foreign",
        "AWS_MAX_ATTEMPTS": "30",
        "HTTPS_PROXY": "https://foreign.invalid",
        "AWS_CONTAINER_CREDENTIALS_FULL_URI": "http://127.0.0.1:1/credentials",
    }.items():
        monkeypatch.setenv(name, value)
    active = True
    accesses: list[str] = []

    def guarded(event: str, args: tuple[object, ...]) -> None:
        if (
            active
            and event == "open"
            and args
            and isinstance(args[0], str | os.PathLike)
        ):
            path = Path(args[0])
            assert not path.is_relative_to(forbidden)
            assert not path.is_relative_to(Path.home() / ".aws")
            accesses.append(str(path))

    sys.addaudithook(guarded)
    requests: list[httpx.Request] = []

    def handle(request: httpx.Request) -> httpx.Response:
        requests.append(request)
        return httpx.Response(404)

    try:
        with httpx.Client(transport=httpx.MockTransport(handle)) as http:
            client = mock_client(
                http, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            )
            assert (
                R2Store(ACCOUNT, BUCKET, CREDENTIALS, client).get("synthetic") is None
            )
    finally:
        active = False
    assert requests[0].url.host == ACCOUNT + ".r2.cloudflarestorage.com"
    assert "Credential=synthetic-access/" in requests[0].headers["authorization"]
    assert "x-amz-security-token" not in requests[0].headers
    assert accesses


def test_sdk_debug_logging_cannot_print_auth_or_error_contents(
    caplog: pytest.LogCaptureFixture,
) -> None:
    with (
        caplog.at_level(logging.DEBUG),
        httpx.Client(
            transport=httpx.MockTransport(
                lambda _r: httpx.Response(
                    403, content=b"synthetic-secret Signature=PRIVATE"
                )
            )
        ) as http,
    ):
        with sdk_client(
            ACCOUNT, BUCKET, CREDENTIALS, http_session=MockHTTP(http)
        ) as client:
            with pytest.raises(PublishError, match=r"^R2 object read failed$"):
                R2Store(ACCOUNT, BUCKET, CREDENTIALS, client).get("synthetic")
    assert "synthetic-access" not in caplog.text
    assert "synthetic-secret" not in caplog.text
    assert "Signature=" not in caplog.text


def test_ambient_plugins_fail_before_client_creation(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setenv("BOTOCORE_EXPERIMENTAL__PLUGINS", "synthetic=forbidden")
    with (
        pytest.raises(UploadError, match=r"^Ambient SDK plugins are forbidden$"),
        sdk_client(ACCOUNT, BUCKET, Credentials("x", "y")),
    ):
        pytest.fail("plugin-enabled SDK opened")


def test_listing_is_bounded_before_sdk_xml_parsing(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.setattr(sdk, "MAX_LIST", 32)
    with httpx.Client(
        transport=httpx.MockTransport(lambda _r: httpx.Response(200, content=b"x" * 33))
    ) as http:
        client = mock_client(
            http, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
        )
        with pytest.raises(
            PublishError, match=r"^Remote response exceeds the configured byte limit$"
        ):
            R2Store(ACCOUNT, BUCKET, CREDENTIALS, client).keys("snapshots/blobs/")


class Counted:
    def __init__(self) -> None:
        self.requests: list[int] = []

    def read(self, amt: int = -1) -> bytes:
        self.requests.append(amt)
        return b"x" * amt

    def close(self) -> None:
        pass


def test_bounded_reader_requests_only_limit_plus_sentinel() -> None:
    raw = Counted()
    with pytest.raises(BoundaryError):
        bounded(raw, 4)
    assert raw.requests == [5]


@pytest.mark.parametrize("method", ["GET", "PUT"])
@pytest.mark.parametrize("code", [202, 206])
def test_sdk_does_not_accept_noncontractual_success_status(
    method: str, code: int
) -> None:
    calls = []

    def handler(request: httpx.Request) -> httpx.Response:
        calls.append(request)
        return httpx.Response(code)

    with httpx.Client(transport=httpx.MockTransport(handler)) as http:
        remote = R2Store(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(http, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS),
        )
        operation = (
            partial(remote.get, "synthetic")
            if method == "GET"
            else partial(remote.put, LEASE_KEY, b"x", {}, expected=None)
        )
        with pytest.raises(
            PublishError,
            match=(
                r"^R2 object read failed$"
                if method == "GET"
                else r"^R2 conditional PUT failed; no unconditional fallback$"
            ),
        ):
            operation()
    assert len(calls) == 1


def test_injected_client_cannot_silently_select_another_account() -> None:
    with sdk_client(ACCOUNT, BUCKET, CREDENTIALS) as client:
        with pytest.raises(
            PublishError, match=r"^S3 client differs from the explicit R2 account$"
        ):
            R2Store("e" * 32, BUCKET, CREDENTIALS, client)


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

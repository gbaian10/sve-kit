"""Typed boto3 boundary with explicit configuration and bounded wire responses."""

import io
import logging
import os
import re
from contextlib import contextmanager
from dataclasses import dataclass, field
from http import HTTPStatus
from typing import TYPE_CHECKING, Protocol, cast, runtime_checkable
from urllib.parse import urlsplit
from xml.etree import ElementTree as ET  # ruff: ignore[suspicious-xml-etree-import] -- bounded UTF-8 inputs reject declarations before the domain checks

from boto3.session import Session
from botocore.awsrequest import AWSPreparedRequest, AWSResponse
from botocore.compat import HTTPHeaders
from botocore.config import Config
from botocore.configprovider import ConfigValueStore, ConstantProvider
from botocore.exceptions import BotoCoreError, ClientError
from botocore.httpsession import URLLib3Session
from botocore.loaders import Loader
from botocore.session import Session as CoreSession

from sve_carddb.r2_upload.boundary import UploadError

if TYPE_CHECKING:
    from collections.abc import Callable, Iterator

    from botocore.model import OperationModel
    from types_boto3_s3 import S3Client
    from types_boto3_s3.type_defs import GetObjectOutputTypeDef, PutObjectRequestTypeDef

MAX_LIST = 2 * 1024 * 1024


@dataclass(frozen=True, repr=False)
class Credentials:
    """Local operator credentials never appear in reports or representations."""

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


def validate_target(account: str, bucket: str) -> None:
    """Reject endpoint injection before creating an SDK or reading credentials."""
    if not re.fullmatch(r"[0-9a-f]{32}", account):
        raise UploadError("R2 account ID must be 32 lowercase hexadecimal characters")
    if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,61}[a-z0-9]", bucket):
        raise UploadError("Invalid R2 bucket name")


def validate_key(key: str) -> None:
    """Reject path normalization outside the explicitly selected bucket."""
    if (
        key.startswith("/")
        or any(p in {".", ".."} for p in key.split("/"))
        or "\\" in key
        or "\0" in key
    ):
        raise UploadError("Invalid object key")


class BoundaryError(Exception):
    """Only fixed messages escape the SDK's untrusted protocol boundary."""


@runtime_checkable
class RawBody(Protocol):
    def read(self, amt: int = ...) -> bytes:
        """Read at most the requested number of undecoded bytes."""
        ...

    def close(self) -> None:
        """Release the HTTP response after any bounded read."""
        ...


def bounded(raw: RawBody, limit: int) -> bytes:
    """Stop before a remote body can exceed the contract plus one sentinel byte."""
    result = bytearray()
    while True:
        chunk = raw.read(min(64 * 1024, limit - len(result) + 1))
        if not chunk:
            return bytes(result)
        result.extend(chunk)
        if len(result) > limit:
            raise BoundaryError("Remote response exceeds the configured byte limit")


class Buffered:
    def __init__(self, raw: bytes) -> None:
        self.raw = io.BytesIO(raw)

    def stream(
        self, amt: int = 64 * 1024, *, decode_content: bool = False
    ) -> Iterator[bytes]:
        """Keep the SDK parser on the already bounded, undecoded body."""
        if decode_content:
            raise BoundaryError("HTTP decoding is forbidden at the R2 boundary")
        while chunk := self.raw.read(amt):
            yield chunk


def _inventory_xml(data: bytes) -> None:
    try:
        text = data.decode("utf-8")
    except UnicodeError:
        raise BoundaryError("R2 inventory XML must be UTF-8") from None
    if "<!" in text:
        raise BoundaryError("R2 inventory XML declarations are forbidden")
    try:
        root = ET.fromstring(text)  # ruff: ignore[suspicious-xml-element-tree-usage] -- declarations rejected above
    except ET.ParseError:
        raise BoundaryError("R2 inventory XML is invalid") from None
    namespace = "{http://s3.amazonaws.com/doc/2006-03-01/}"
    if root.tag != namespace + "ListBucketResult":
        raise BoundaryError("R2 inventory prefix differs from request")
    # The SDK's permissive boolean parser otherwise treats any text as false.
    if root.findtext(namespace + "IsTruncated") not in {"true", "false"}:
        raise BoundaryError("R2 inventory lacks a truncation flag")


@dataclass(repr=False)
class Transport:
    """The SDK owns signing and XML parsing; this guard owns deployment policy."""

    account: str
    bucket: str
    session: URLLib3Session = field(
        default_factory=lambda: URLLib3Session(verify=True, proxies={}, timeout=30)
    )
    operation: str = ""
    sent: bool = False

    def begin(self, model: OperationModel, **_kwargs: object) -> None:
        """Reject nested SDK discovery operations and reset the one-send gate."""
        if model.name not in {
            "GetObject",
            "PutObject",
            "ListObjectsV2",
            "DeleteObject",
        }:
            raise BoundaryError("Unexpected R2 SDK operation")
        self.operation = model.name
        self.sent = False

    def send(self, request: AWSPreparedRequest, **_kwargs: object) -> AWSResponse:
        """Bound parser inputs and refuse redirects, retries and other endpoints."""
        # Region redirects and special S3 retry handlers can bypass max_attempts.
        url = urlsplit(request.url)
        if (
            self.sent
            or url.scheme != "https"
            or url.netloc != self.account + ".r2.cloudflarestorage.com"
            or not (
                url.path == "/" + self.bucket
                or url.path.startswith("/" + self.bucket + "/")
            )
            or url.fragment
        ):
            raise BoundaryError("R2 SDK attempted a retry or endpoint change")
        self.sent = True
        request.headers["Accept-Encoding"] = "identity"
        request.stream_output = True
        response = self.session.send(request)
        if self.operation == "GetObject" and response.status_code == HTTPStatus.OK:
            return response
        raw: object = response.raw
        if not isinstance(raw, RawBody):
            raise BoundaryError("Invalid R2 response stream")
        accepted = {HTTPStatus.OK}
        if self.operation == "PutObject":
            accepted |= {HTTPStatus.CREATED, HTTPStatus.NO_CONTENT}
        elif self.operation == "DeleteObject":
            accepted.add(HTTPStatus.NO_CONTENT)
        if response.status_code not in accepted:
            raw.close()
            # No SDK error parser or redirect handler sees remote error contents.
            raise ClientError(
                {
                    "Error": {
                        "Code": str(response.status_code),
                        "Message": "R2 request failed",
                    },
                    "ResponseMetadata": {
                        "HTTPStatusCode": response.status_code,
                        "HTTPHeaders": {},
                        "HostId": "",
                        "RequestId": "",
                        "RetryAttempts": 0,
                    },
                },
                self.operation,
            )
        try:
            data = bounded(raw, MAX_LIST)
        finally:
            raw.close()
        if self.operation == "ListObjectsV2" and response.status_code == HTTPStatus.OK:
            _inventory_xml(data)
        headers = HTTPHeaders()
        for name in response.headers:
            value: object = response.headers[name]
            if not isinstance(value, str):
                raise BoundaryError("Invalid R2 response header")
            headers[name] = value
        return AWSResponse(request.url, response.status_code, headers, Buffered(data))


class ResponseEvents(Protocol):
    # botocore-stubs incorrectly requires None from before-send response handlers.
    def register(self, event_name: str, handler: Callable[..., AWSResponse]) -> None:
        """Allow the documented SDK response-returning before-send callback."""
        ...


@contextmanager
def _open_client(
    account: str,
    bucket: str,
    credentials: Credentials,
    *,
    http_session: URLLib3Session | None = None,
) -> Iterator[S3Client]:
    """Open a typed S3 client without ambient AWS files, profiles or proxies."""
    validate_target(account, bucket)
    if os.environ.get("BOTOCORE_EXPERIMENTAL__PLUGINS", "DISABLED") not in {
        "",
        "DISABLED",
    }:
        raise UploadError("Ambient SDK plugins are forbidden")
    core = CoreSession()
    core.register_component(
        "data_loader",
        Loader(
            extra_search_paths=[Loader.BUILTIN_DATA_PATH],
            include_default_search_paths=False,
        ),
    )
    # Constant providers exclude all AWS environment, profiles, config and metadata.
    store = ConfigValueStore()
    for name, definition in core.SESSION_VARIABLES.items():
        default: object = definition[2]
        store.set_config_provider(name, ConstantProvider(default))
    store.set_config_variable("config_file", os.devnull)
    store.set_config_variable("credentials_file", os.devnull)
    store.set_config_variable("profile", None)
    store.set_config_variable("data_path", None)
    core.register_component("config_store", store)
    core.set_credentials(credentials.access_key, credentials.secret_key)
    session = Session(botocore_session=core)
    client = session.client(
        "s3",
        endpoint_url=f"https://{account}.r2.cloudflarestorage.com",
        region_name="auto",
        aws_access_key_id=credentials.access_key,
        aws_secret_access_key=credentials.secret_key,
        verify=True,
        config=Config(
            signature_version="s3v4",
            proxies={},
            connect_timeout=30,
            read_timeout=30,
            retries={"total_max_attempts": 1, "mode": "standard"},
            s3={"addressing_style": "path", "payload_signing_enabled": True},
            request_checksum_calculation="when_required",
            response_checksum_validation="when_required",
        ),
    )
    transport = (
        Transport(account, bucket)
        if http_session is None
        else Transport(account, bucket, http_session)
    )
    client.meta.events.register("before-call.s3", transport.begin)
    cast("ResponseEvents", client.meta.events).register(
        "before-send.s3", transport.send
    )
    try:
        yield client
    finally:
        client.close()
        transport.session.close()


@contextmanager
def _quiet_sdk() -> Iterator[None]:
    roots = ("botocore", "boto3", "urllib3")
    for root in roots:
        logging.getLogger(root)
    loggers = [
        value
        for name, value in logging.Logger.manager.loggerDict.copy().items()
        if isinstance(value, logging.Logger)
        and any(name == root or name.startswith(root + ".") for root in roots)
    ]
    saved = [(logger, logger.level, logger.disabled) for logger in loggers]
    for logger in loggers:
        logger.setLevel(logging.CRITICAL + 1)
        logger.disabled = True
    try:
        yield
    finally:
        for logger, level, disabled in saved:
            logger.setLevel(level)
            logger.disabled = disabled


@contextmanager
def sdk_client(
    account: str,
    bucket: str,
    credentials: Credentials,
    *,
    http_session: URLLib3Session | None = None,
) -> Iterator[S3Client]:
    """Keep SDK debug logs and ambient AWS configuration outside the boundary."""
    with _quiet_sdk():
        try:
            with _open_client(
                account, bucket, credentials, http_session=http_session
            ) as client:
                yield client
        except BotoCoreError:
            raise UploadError("R2 SDK initialization or transport failed") from None


def status(error: ClientError) -> int:
    """Never expose SDK error messages or arbitrary response dictionaries."""
    response: object = error.response
    if isinstance(response, dict):
        metadata: object = response.get("ResponseMetadata")
        if isinstance(metadata, dict):
            value: object = metadata.get("HTTPStatusCode")
            if type(value) is int:
                return value
    return 0


def put_parameters(
    bucket: str, key: str, raw: bytes, headers: dict[str, str]
) -> PutObjectRequestTypeDef:
    """Only supported metadata and one conditional write cross into the SDK."""
    validate_key(key)
    if set(headers) - {
        "content-type",
        "cache-control",
        "content-encoding",
        "if-match",
        "if-none-match",
        "x-amz-meta-sha256",
    }:
        raise BoundaryError("Unsupported object metadata")
    params: PutObjectRequestTypeDef = {"Bucket": bucket, "Key": key, "Body": raw}
    if "x-amz-meta-sha256" in headers:
        params["Metadata"] = {"sha256": headers["x-amz-meta-sha256"]}
    if "content-type" in headers:
        params["ContentType"] = headers["content-type"]
    if "cache-control" in headers:
        params["CacheControl"] = headers["cache-control"]
    if "content-encoding" in headers:
        params["ContentEncoding"] = headers["content-encoding"]
    if (
        "if-none-match" in headers
        and "if-match" not in headers
        and headers["if-none-match"] == "*"
    ):
        params["IfNoneMatch"] = "*"
    elif (
        "if-match" in headers
        and "if-none-match" not in headers
        and headers["if-match"] not in {"", "*"}
        and not headers["if-match"].startswith("W/")
    ):
        params["IfMatch"] = headers["if-match"]
    else:
        raise BoundaryError("Conditional PUT requires an opaque object ETag")
    return params


def object_headers(response: GetObjectOutputTypeDef) -> dict[str, str]:
    """Export only the metadata accepted by the publication contract."""
    result = {}
    if "ContentType" in response:
        result["content-type"] = response["ContentType"]
    if "CacheControl" in response:
        result["cache-control"] = response["CacheControl"]
    if "ContentEncoding" in response:
        result["content-encoding"] = response["ContentEncoding"]
    if "ETag" in response:
        result["etag"] = response["ETag"]
    return result

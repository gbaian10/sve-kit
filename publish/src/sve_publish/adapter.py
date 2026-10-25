"""R2 S3 boundary: exact reads, conditional writes and public-only listing."""

import re
import sys
from contextlib import contextmanager
from dataclasses import dataclass
from http import HTTPStatus
from time import sleep as sleep_for
from typing import TYPE_CHECKING

from botocore.exceptions import (
    BotoCoreError,
    ClientError,
    ConnectionClosedError,
    ConnectTimeoutError,
    EndpointConnectionError,
    ReadTimeoutError,
)
from botocore.parsers import ResponseParserError
from sve_carddb.export.read_api import IMAGE_KEY, JSON_KEY, ExportError

from sve_publish.headers import IMAGE_HEADERS
from sve_publish.sdk import (
    BoundaryError,
    Credentials,
    bounded,
    object_headers,
    put_parameters,
    status,
    validate_key,
)

if TYPE_CHECKING:
    from collections.abc import Callable, Generator

    from types_boto3_s3 import S3Client
    from types_boto3_s3.type_defs import (
        ListObjectsV2OutputTypeDef,
        ListObjectsV2RequestTypeDef,
    )

PUBLIC_PREFIXES = frozenset(
    {"snapshots/blobs/", "snapshots/manifests/"}
    | {"images/" + s + "/" for s in ("card_s", "card_l", "art_s")}
)
SNAPSHOT_PREFIXES = frozenset(p for p in PUBLIC_PREFIXES if p.startswith("snapshots/"))
IMAGE_PREFIXES = PUBLIC_PREFIXES - SNAPSHOT_PREFIXES
MAX_OBJECT = 128 * 1024 * 1024
# R2 processes roughly 60 ms per key, so each batch must finish within the 30-second read timeout.
MAX_DELETE_BATCH = 100
IMAGE_RETRY_DELAYS = (1, 4)
HTTP_STATUS_LIMIT = 600
IMAGE_TRANSPORT_ERRORS = (
    EndpointConnectionError,
    ConnectionClosedError,
    ReadTimeoutError,
    ConnectTimeoutError,
)


def _retryable_image_status(code: int) -> bool:
    return code in {HTTPStatus.REQUEST_TIMEOUT, HTTPStatus.TOO_MANY_REQUESTS} or (
        HTTPStatus.INTERNAL_SERVER_ERROR <= code < HTTP_STATUS_LIMIT
    )


def _image_put_failure(error: ClientError | BotoCoreError) -> tuple[bool, str]:
    if isinstance(error, ClientError):
        code = status(error)
        return _retryable_image_status(code), str(code)
    return isinstance(error, IMAGE_TRANSPORT_ERRORS), type(error).__name__


@dataclass(frozen=True, repr=False)
class Stored:
    """An ETag is an opaque precondition, never a content digest."""

    raw: bytes
    etag: str
    headers: dict[str, str]


@dataclass(repr=False)
class R2Store:
    """Single bucket boto3 boundary for one explicit R2 deployment."""

    account_id: str
    bucket: str
    credentials: Credentials
    client: S3Client

    def __post_init__(self) -> None:
        """Restrict the publication adapter to one explicit R2 deployment."""
        if not re.fullmatch(r"[0-9a-f]{32}", self.account_id):
            raise ExportError("Invalid explicit R2 account ID")
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,61}[a-z0-9]", self.bucket):
            raise ExportError("Invalid explicit R2 bucket")
        if (
            self.client.meta.endpoint_url
            != f"https://{self.account_id}.r2.cloudflarestorage.com"
        ):
            raise ExportError("S3 client differs from the explicit R2 account")

    @staticmethod
    @contextmanager
    def _operation(key: str, *, preserve_errors: bool = False) -> Generator[None]:
        try:
            validate_key(key)
        except ExportError:
            raise ExportError("Invalid S3 object key") from None
        try:
            yield
        except BoundaryError as error:
            if preserve_errors:
                raise
            raise ExportError(str(error)) from None
        except BotoCoreError:
            if preserve_errors:
                raise
            raise ExportError("R2 transport or protocol failed") from None
        except ResponseParserError:
            raise ExportError("R2 inventory XML is invalid") from None

    def get(self, key: str) -> Stored | None:
        """Read exact raw bytes, opaque ETag and contractual content metadata."""
        with self._operation(key):
            try:
                response = self.client.get_object(Bucket=self.bucket, Key=key)
            except ClientError as error:
                if status(error) == HTTPStatus.NOT_FOUND:
                    return None
                raise ExportError("R2 object read failed") from None
            body = response["Body"]
            try:
                etag = response.get("ETag", "")
                if not etag or etag.startswith("W/"):
                    raise ExportError("R2 object requires a strong opaque ETag")
                metadata = object_headers(response)
                metadata.pop("etag", None)
                return Stored(bounded(body, MAX_OBJECT), etag, metadata)
            finally:
                body.close()

    def put(
        self, key: str, raw: bytes, headers: dict[str, str], *, expected: str | None
    ) -> bool:
        """Never retry writes or downgrade an unsupported condition to a plain PUT."""
        if set(headers) - {"content-type", "cache-control", "content-encoding"}:
            raise ExportError("Unsupported object metadata")
        if expected is not None and (
            not expected or expected == "*" or expected.startswith("W/")
        ):
            raise ExportError("Conditional PUT requires an opaque object ETag")
        condition = (
            {"if-none-match": "*"} if expected is None else {"if-match": expected}
        )
        with self._operation(key):
            try:
                self.client.put_object(
                    **put_parameters(self.bucket, key, raw, headers | condition)
                )
            except ClientError as error:
                if status(error) == HTTPStatus.PRECONDITION_FAILED:
                    return False
                raise ExportError(
                    "R2 conditional PUT failed; no unconditional fallback"
                ) from None
            return True

    def put_image(
        self,
        key: str,
        raw: bytes,
        *,
        stopped: Callable[[], bool] | None = None,
        progress: Callable[[str], None] | None = None,
        sleep: Callable[[float], None] | None = None,
    ) -> None:
        """Overwrite only image keys after the inventory stopped advertising them."""
        if not IMAGE_KEY.fullmatch(key):
            raise ExportError("Image PUT requires a public WebP key")
        last_reason = "retry stopped"
        for attempt in range(1, len(IMAGE_RETRY_DELAYS) + 2):
            if attempt > 1 and stopped is not None and stopped():
                raise ExportError(
                    f"R2 image PUT failed ({last_reason}); rerun the same image input"
                )
            try:
                with self._operation(key, preserve_errors=True):
                    self.client.put_object(
                        **put_parameters(
                            self.bucket, key, raw, IMAGE_HEADERS, image=True
                        )
                    )
            except BoundaryError:
                raise ExportError(
                    "R2 image PUT failed (BoundaryError); rerun the same image input"
                ) from None
            except (ClientError, BotoCoreError) as error:
                retryable, reason = _image_put_failure(error)
                last_reason = reason
                if not retryable or attempt > len(IMAGE_RETRY_DELAYS):
                    raise ExportError(
                        f"R2 image PUT failed ({reason}); rerun the same image input"
                    ) from None
                if stopped is not None and stopped():
                    raise ExportError(
                        f"R2 image PUT failed ({reason}); rerun the same image input"
                    ) from None
                line = f"Retrying image PUT {key}, attempt {attempt + 1}/3 ({reason})"
                if progress is None:
                    sys.stderr.write(line + "\n")
                else:
                    progress(line)
                (sleep or sleep_for)(IMAGE_RETRY_DELAYS[attempt - 1])
            else:
                return

    def delete_many(self, keys: list[str]) -> None:
        """Delete one public-only batch; validate every key before sending it."""
        if not 1 <= len(keys) <= MAX_DELETE_BATCH:
            raise ExportError(f"Deletion batch requires 1 to {MAX_DELETE_BATCH} keys")
        if any(
            not any(key.startswith(p) for p in PUBLIC_PREFIXES)
            or not (JSON_KEY.fullmatch(key) or IMAGE_KEY.fullmatch(key))
            for key in keys
        ):
            raise ExportError("Deletion key is outside the public namespaces")
        with self._operation(""):
            try:
                response = self.client.delete_objects(
                    Bucket=self.bucket,
                    Delete={"Objects": [{"Key": key} for key in keys], "Quiet": True},
                )
            except ClientError:
                raise ExportError("R2 DELETE failed; inspect before retry") from None
            if errors := response.get("Errors"):
                raise ExportError(
                    f"R2 DELETE failed for {len(errors)} objects; inspect and rerun GC"
                )

    def keys(self, prefix: str) -> tuple[str, ...]:
        """Bounded SDK ListObjectsV2 pagination only in public namespaces."""
        if prefix not in PUBLIC_PREFIXES:
            raise ExportError("List prefix is not explicitly public")
        keys: set[str] = set()
        seen: set[str] = set()
        token = None
        while True:
            params: ListObjectsV2RequestTypeDef = {
                "Bucket": self.bucket,
                "Prefix": prefix,
                "MaxKeys": 1000,
            }
            if token is not None:
                params["ContinuationToken"] = token
            with self._operation(""):
                try:
                    response = self.client.list_objects_v2(**params)
                except ClientError:
                    raise ExportError("R2 public inventory read failed") from None
            page, token = _page(response, prefix)
            if keys.intersection(page):
                raise ExportError("R2 inventory contains repeated keys")
            keys.update(page)
            if token is None:
                return tuple(sorted(keys))
            if token in seen:
                raise ExportError("R2 inventory pagination repeats a token")
            seen.add(token)


def _page(
    response: ListObjectsV2OutputTypeDef, prefix: str
) -> tuple[set[str], str | None]:
    if response.get("Prefix") != prefix:
        raise ExportError("R2 inventory prefix differs from request")
    rows = response.get("Contents", [])
    result = {r.get("Key", "") for r in rows}
    if len(result) != len(rows) or any(not k.startswith(prefix) for k in result):
        raise ExportError("R2 inventory contains invalid keys")
    if "IsTruncated" not in response:
        raise ExportError("R2 inventory lacks a truncation flag")
    token = response.get("NextContinuationToken")
    if response["IsTruncated"] and not token:
        raise ExportError("R2 inventory lacks a continuation token")
    return result, token if response["IsTruncated"] else None

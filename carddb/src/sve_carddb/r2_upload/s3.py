"""Conditional preview objects through the typed boto3 S3 boundary."""

import time
from dataclasses import dataclass, field
from http import HTTPStatus
from typing import TYPE_CHECKING

from botocore.exceptions import (
    BotoCoreError,
    ClientError,
    ConnectionClosedError,
    ConnectTimeoutError,
    EndpointConnectionError,
    IncompleteReadError,
    ReadTimeoutError,
    ResponseStreamingError,
)

from sve_carddb.r2_upload.plan import UploadError
from sve_carddb.r2_upload.sdk import (
    BoundaryError,
    bounded,
    object_headers,
    put_parameters,
    status,
    validate_key,
    validate_target,
)
from sve_carddb.r2_upload.sdk import Credentials as Credentials  # ruff: ignore[useless-import-alias] -- preserve the existing credentials import boundary

if TYPE_CHECKING:
    from collections.abc import Callable

    from types_boto3_s3 import S3Client


@dataclass(frozen=True)
class Remote:
    raw: bytes
    headers: dict[str, str]


@dataclass(repr=False)
class S3:
    account_id: str
    bucket: str
    credentials: Credentials
    client: S3Client
    wait: Callable[[float], None] = field(default=time.sleep)

    def __post_init__(self) -> None:
        """Pin the preview client to the explicit account and bucket."""
        validate_target(self.account_id, self.bucket)
        if (
            self.client.meta.endpoint_url
            != f"https://{self.account_id}.r2.cloudflarestorage.com"
        ):
            raise UploadError("S3 client differs from the explicit R2 account")

    def get(self, key: str, *, limit: int) -> Remote | None:
        """Only transient reads retry, with a fixed finite backoff."""
        validate_key(key)
        delays = (0.5, 1.0)
        for attempt in range(3):
            try:
                return self._get_once(key, limit=limit)
            except (
                ReadTimeoutError,
                ConnectTimeoutError,
                EndpointConnectionError,
                ConnectionClosedError,
                IncompleteReadError,
                ResponseStreamingError,
            ):
                if attempt == len(delays):
                    break
                self.wait(delays[attempt])
            except BotoCoreError, BoundaryError:
                break
        raise UploadError("R2 transport failed") from None

    def _get_once(self, key: str, *, limit: int) -> Remote | None:
        try:
            response = self.client.get_object(Bucket=self.bucket, Key=key)
        except ClientError as error:
            if status(error) == HTTPStatus.NOT_FOUND:
                return None
            raise UploadError("R2 object read failed") from None
        body = response["Body"]
        try:
            if response.get("ContentEncoding", "identity") != "identity":
                raise UploadError("R2 object has unexpected content encoding")
            try:
                raw = bounded(body, limit)
            except BoundaryError:
                raise UploadError("R2 object exceeds expected size") from None
            return Remote(raw, object_headers(response))
        finally:
            body.close()

    def put(self, key: str, raw: bytes, headers: dict[str, str]) -> bool:
        """A 412 alone means conflict; uncertain writes never retry."""
        validate_key(key)
        try:
            self.client.put_object(**put_parameters(self.bucket, key, raw, headers))
        except ClientError as error:
            if status(error) == HTTPStatus.PRECONDITION_FAILED:
                return False
            raise UploadError("R2 conditional object write failed") from None
        except BotoCoreError, BoundaryError:
            raise UploadError("R2 transport failed") from None
        return True

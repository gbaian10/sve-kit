"""R2 S3 boundary: exact reads, conditional writes and public-only listing."""

import re
from contextlib import contextmanager
from dataclasses import dataclass
from http import HTTPStatus
from typing import TYPE_CHECKING

from botocore.exceptions import BotoCoreError, ClientError
from botocore.parsers import ResponseParserError

from sve_carddb.r2_upload.sdk import (
    BoundaryError,
    Credentials,
    bounded,
    object_headers,
    put_parameters,
    status,
    validate_key,
)
from sve_carddb.snapshot.read_api import IMAGE_KEY, JSON_KEY
from sve_carddb.snapshot.read_api import ExportError as UploadError

if TYPE_CHECKING:
    from collections.abc import Iterator

    from types_boto3_s3 import S3Client
    from types_boto3_s3.type_defs import (
        ListObjectsV2OutputTypeDef,
        ListObjectsV2RequestTypeDef,
    )

PUBLIC_PREFIXES = frozenset(
    {"snapshots/blobs/", "snapshots/manifests/"}
    | {"images/" + s + "/" for s in ("card_s", "card_m", "card_l", "art_s", "art_m")}
)
MAX_OBJECT = 128 * 1024 * 1024


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
            raise UploadError("Invalid explicit R2 account ID")
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,61}[a-z0-9]", self.bucket):
            raise UploadError("Invalid explicit R2 bucket")
        if (
            self.client.meta.endpoint_url
            != f"https://{self.account_id}.r2.cloudflarestorage.com"
        ):
            raise UploadError("S3 client differs from the explicit R2 account")

    @staticmethod
    @contextmanager
    def _operation(key: str) -> Iterator[None]:
        try:
            validate_key(key)
        except UploadError:
            raise UploadError("Invalid S3 object key") from None
        try:
            yield
        except BoundaryError as error:
            raise UploadError(str(error)) from None
        except BotoCoreError:
            raise UploadError("R2 transport or protocol failed") from None
        except ResponseParserError:
            raise UploadError("R2 inventory XML is invalid") from None

    def get(self, key: str) -> Stored | None:
        """Read exact raw bytes, opaque ETag and contractual content metadata."""
        with self._operation(key):
            try:
                response = self.client.get_object(Bucket=self.bucket, Key=key)
            except ClientError as error:
                if status(error) == HTTPStatus.NOT_FOUND:
                    return None
                raise UploadError("R2 object read failed") from None
            body = response["Body"]
            try:
                etag = response.get("ETag", "")
                if not etag or etag.startswith("W/"):
                    raise UploadError("R2 object requires a strong opaque ETag")
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
            raise UploadError("Unsupported object metadata")
        if expected is not None and (
            not expected or expected == "*" or expected.startswith("W/")
        ):
            raise UploadError("Conditional PUT requires an opaque object ETag")
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
                raise UploadError(
                    "R2 conditional PUT failed; no unconditional fallback"
                ) from None
            return True

    def delete(self, key: str) -> None:
        """GC alone deletes; R2 does not promise If-Match on DeleteObject."""
        if not any(key.startswith(p) for p in PUBLIC_PREFIXES) or not (
            JSON_KEY.fullmatch(key) or IMAGE_KEY.fullmatch(key)
        ):
            raise UploadError("Deletion key is outside the public namespaces")
        with self._operation(key):
            try:
                self.client.delete_object(Bucket=self.bucket, Key=key)
            except ClientError:
                raise UploadError("R2 DELETE failed; inspect before retry") from None

    def keys(self, prefix: str) -> tuple[str, ...]:
        """Bounded SDK ListObjectsV2 pagination only in public namespaces."""
        if prefix not in PUBLIC_PREFIXES:
            raise UploadError("List prefix is not explicitly public")
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
                    raise UploadError("R2 public inventory read failed") from None
            page, token = _page(response, prefix)
            if keys.intersection(page):
                raise UploadError("R2 inventory contains repeated keys")
            keys.update(page)
            if token is None:
                return tuple(sorted(keys))
            if token in seen:
                raise UploadError("R2 inventory pagination repeats a token")
            seen.add(token)


def _page(
    response: ListObjectsV2OutputTypeDef, prefix: str
) -> tuple[set[str], str | None]:
    if response.get("Prefix") != prefix:
        raise UploadError("R2 inventory prefix differs from request")
    rows = response.get("Contents", [])
    result = {r.get("Key", "") for r in rows}
    if len(result) != len(rows) or any(not k.startswith(prefix) for k in result):
        raise UploadError("R2 inventory contains invalid keys")
    if "IsTruncated" not in response:
        raise UploadError("R2 inventory lacks a truncation flag")
    token = response.get("NextContinuationToken")
    if response["IsTruncated"] and not token:
        raise UploadError("R2 inventory lacks a continuation token")
    return result, token if response["IsTruncated"] else None

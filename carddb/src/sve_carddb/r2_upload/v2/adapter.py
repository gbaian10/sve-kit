"""R2 S3 conditional boundary with a non-expiring deployment-wide CAS lease."""

import re
from contextlib import contextmanager
from dataclasses import dataclass, field
from http import HTTPStatus
from typing import TYPE_CHECKING
from uuid import uuid4

from botocore.exceptions import BotoCoreError, ClientError
from botocore.parsers import ResponseParserError

from sve_carddb.r2_upload.plan import UploadError
from sve_carddb.r2_upload.sdk import (
    BoundaryError,
    Credentials,
    bounded,
    object_headers,
    put_parameters,
    status,
    validate_key,
)
from sve_carddb.snapshot.publish.plan import IMAGE_KEY, INDEX, JSON_KEY
from sve_carddb.snapshot.publish.storage import PublishError, Stored
from sve_carddb.snapshot.values import canonical, digest, integer

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue
    from types_boto3_s3 import S3Client
    from types_boto3_s3.type_defs import (
        ListObjectsV2OutputTypeDef,
        ListObjectsV2RequestTypeDef,
    )

LEASE_KEY = "coordination/snapshot-v2-writer.json"
LEASE_HEADERS = {"content-type": "application/json", "cache-control": "no-store"}
PUBLIC_PREFIXES = frozenset(
    {"snapshots/blobs/", "snapshots/manifests/"}
    | {"images/" + s + "/" for s in ("card_s", "card_m", "card_l", "art_s", "art_m")}
)
MAX_OBJECT = 128 * 1024 * 1024


@dataclass(repr=False)
class R2Store:
    """Single bucket boto3 boundary with the deployment-wide CAS lease."""

    account_id: str
    bucket: str
    credentials: Credentials
    client: S3Client
    _lease: Stored | None = field(default=None, init=False)

    def __post_init__(self) -> None:
        """Restrict the publication adapter to one explicit R2 deployment."""
        if not re.fullmatch(r"[0-9a-f]{32}", self.account_id):
            raise PublishError("Invalid explicit R2 account ID")
        if not re.fullmatch(r"[a-z0-9][a-z0-9-]{1,61}[a-z0-9]", self.bucket):
            raise PublishError("Invalid explicit R2 bucket")
        if (
            self.client.meta.endpoint_url
            != f"https://{self.account_id}.r2.cloudflarestorage.com"
        ):
            raise PublishError("S3 client differs from the explicit R2 account")

    @staticmethod
    @contextmanager
    def _operation(key: str) -> Iterator[None]:
        try:
            validate_key(key)
        except UploadError:
            raise PublishError("Invalid S3 object key") from None
        try:
            yield
        except BoundaryError as error:
            raise PublishError(str(error)) from None
        except BotoCoreError:
            raise PublishError("R2 transport or protocol failed") from None
        except ResponseParserError:
            raise PublishError("R2 inventory XML is invalid") from None

    def get(self, key: str) -> Stored | None:
        """Read exact raw bytes, opaque ETag and contractual content metadata."""
        with self._operation(key):
            try:
                response = self.client.get_object(Bucket=self.bucket, Key=key)
            except ClientError as error:
                if status(error) == HTTPStatus.NOT_FOUND:
                    return None
                raise PublishError("R2 object read failed") from None
            body = response["Body"]
            try:
                etag = response.get("ETag", "")
                if not etag or etag.startswith("W/"):
                    raise PublishError("R2 object requires a strong opaque ETag")
                metadata = object_headers(response)
                metadata.pop("etag", None)
                return Stored(bounded(body, MAX_OBJECT), etag, metadata)
            finally:
                body.close()

    def put(
        self, key: str, raw: bytes, headers: dict[str, str], *, expected: str | None
    ) -> bool:
        """Never retry writes or downgrade an unsupported condition to a plain PUT."""
        if key != LEASE_KEY:
            self.verify_lease()
        if set(headers) - {"content-type", "cache-control", "content-encoding"}:
            raise PublishError("Unsupported object metadata")
        if expected is not None and (
            not expected or expected == "*" or expected.startswith("W/")
        ):
            raise PublishError("Conditional PUT requires an opaque object ETag")
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
                raise PublishError(
                    "R2 conditional PUT failed; no unconditional fallback"
                ) from None
            return True

    @staticmethod
    def delete(key: str, *, expected: str) -> bool:
        """R2's compatibility table does not promise atomic If-Match on DELETE."""
        if not expected or not any(key.startswith(p) for p in PUBLIC_PREFIXES):
            raise PublishError("Invalid conditional deletion request")
        raise PublishError(
            "R2 conditional DELETE is unverified; use separately approved GC"
        )

    def delete_approved(
        self, key: str, *, index: Stored, candidate: dict[str, JsonValue]
    ) -> None:
        """Explicit GC alone may delete without If-Match under a verified lease.

        Atomicity is the maintainer-approved single-writer assumption, not an
        S3 conditional-delete guarantee. Recheck the full index immediately last.
        """
        if (
            candidate.get("key") != key
            or not any(key.startswith(p) for p in PUBLIC_PREFIXES)
            or not (JSON_KEY.fullmatch(key) or IMAGE_KEY.fullmatch(key))
        ):
            raise PublishError("GC deletion key is outside the approved public scope")
        self.verify_lease()
        value = self.get(key)
        if (
            value is None
            or value.etag != candidate.get("etag")
            or len(value.raw) != integer(candidate.get("bytes"))
            or digest(value.raw) != candidate.get("sha256")
        ):
            raise PublishError("GC candidate changed before deletion")
        if self.get(INDEX) != index:
            raise PublishError("GC current/previous index changed before deletion")
        self.verify_lease()
        with self._operation(key):
            try:
                self.client.delete_object(Bucket=self.bucket, Key=key)
            except ClientError:
                raise PublishError(
                    "R2 approved DELETE failed; inspect before retry"
                ) from None

    def keys(self, prefix: str) -> tuple[str, ...]:
        """Bounded SDK ListObjectsV2 pagination only in public namespaces."""
        if prefix not in PUBLIC_PREFIXES:
            raise PublishError("List prefix is not explicitly public")
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
                    raise PublishError("R2 public inventory read failed") from None
            page, token = _page(response, prefix)
            if keys.intersection(page):
                raise PublishError("R2 inventory contains repeated keys")
            keys.update(page)
            if token is None:
                return tuple(sorted(keys))
            if token in seen:
                raise PublishError("R2 inventory pagination repeats a token")
            seen.add(token)

    def verify_lease(self) -> None:
        """Require the currently held deployment-wide CAS owner before mutations."""
        if self._lease is None or self.get(LEASE_KEY) != self._lease:
            raise PublishError("Deployment writer lease changed; stop and review")

    @contextmanager
    def exclusive(self) -> Iterator[None]:
        """All machines share a CAS lock; crashes never authorize timed takeover.

        A stuck active record needs separate operator recovery after proving all
        writers stopped. Unlock uses conditional PUT, never unsupported DELETE.
        """
        if self._lease is not None:
            raise PublishError(
                "Deployment writer lease is already held by this adapter"
            )
        old = self.get(LEASE_KEY)
        if old is not None and (
            old.raw != canonical({"format": 1, "owner": None})
            or old.headers != LEASE_HEADERS
        ):
            raise PublishError(
                "Deployment writer lease is active or invalid; operator recovery required"
            )
        raw = canonical({"format": 1, "owner": uuid4().hex})
        if not self.put(
            LEASE_KEY, raw, LEASE_HEADERS, expected=None if old is None else old.etag
        ):
            raise PublishError("Deployment writer lease was acquired concurrently")
        acquired = self.get(LEASE_KEY)
        if acquired is None or acquired.raw != raw or acquired.headers != LEASE_HEADERS:
            raise PublishError("Deployment writer lease acquisition is uncertain")
        self._lease = acquired
        try:
            yield
        finally:
            try:
                self.verify_lease()
                if not self.put(
                    LEASE_KEY,
                    canonical({"format": 1, "owner": None}),
                    LEASE_HEADERS,
                    expected=acquired.etag,
                ):
                    raise PublishError("Deployment writer lease release failed")
            finally:
                self._lease = None


def _page(
    response: ListObjectsV2OutputTypeDef, prefix: str
) -> tuple[set[str], str | None]:
    if response.get("Prefix") != prefix:
        raise PublishError("R2 inventory prefix differs from request")
    rows = response.get("Contents", [])
    result = {r.get("Key", "") for r in rows}
    if len(result) != len(rows) or any(not k.startswith(prefix) for k in result):
        raise PublishError("R2 inventory contains invalid keys")
    if "IsTruncated" not in response:
        raise PublishError("R2 inventory lacks a truncation flag")
    token = response.get("NextContinuationToken")
    if response["IsTruncated"] and not token:
        raise PublishError("R2 inventory lacks a continuation token")
    return result, token if response["IsTruncated"] else None

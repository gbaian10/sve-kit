"""Small, independent counterexamples for metadata and inventory boundaries."""

from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.export.read_api import ExportError as UploadError
from sve_carddb.r2_upload.v2.adapter import R2Store
from sve_carddb.r2_upload.v2.freshness import CDNFreshness

from .r2_sdk_fixtures import inventory, mock_client
from .r2_v2_fixtures import ACCOUNT, BUCKET, CREDENTIALS
from .r2_v2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated loopback server
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

if TYPE_CHECKING:
    from .r2_v2_fixtures import Loopback, ServerState

PREFIX = "snapshots/blobs/"
KEY = PREFIX + "a" * 64 + ".json"


def test_duplicate_rows_in_one_inventory_page_are_rejected() -> None:
    raw = (
        '<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">'
        "<Prefix>snapshots/blobs/</Prefix>"
        "<Contents><Key>snapshots/blobs/x</Key></Contents>"
        "<Contents><Key>snapshots/blobs/x</Key></Contents>"
        "<IsTruncated>false</IsTruncated></ListBucketResult>"
    )
    with pytest.raises(UploadError, match=r"^R2 inventory contains invalid keys$"):
        inventory(raw.encode(), PREFIX)


def test_duplicate_keys_across_inventory_pages_are_rejected() -> None:
    calls = 0

    def handle(_request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        raw = (
            '<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/">'
            "<Prefix>snapshots/blobs/</Prefix>"
            "<Contents><Key>snapshots/blobs/x</Key></Contents>"
            f"<IsTruncated>{'true' if calls == 1 else 'false'}</IsTruncated>"
            "<NextContinuationToken>second</NextContinuationToken></ListBucketResult>"
        )
        return httpx.Response(200, stream=httpx.ByteStream(raw.encode()))

    with httpx.Client(transport=httpx.MockTransport(handle)) as client:
        store = R2Store(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            ),
        )
        with pytest.raises(UploadError, match=r"^R2 inventory contains repeated keys$"):
            store.keys(PREFIX)
    assert calls == 2


def test_public_put_refuses_noncontractual_metadata(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _transport = remote
    with pytest.raises(UploadError, match=r"^Unsupported object metadata$"):
        store.put(
            KEY,
            b"synthetic",
            {"content-type": "application/json", "x-amz-meta-extra": "unknown"},
            expected=None,
        )
    assert KEY not in state.objects
    assert not any(op[1] == KEY for op in state.operations)


def test_cdn_encoded_bytes_are_not_an_identity_image() -> None:
    def handle(_request: httpx.Request) -> httpx.Response:
        return httpx.Response(
            200,
            headers={"content-encoding": "gzip"},
            stream=httpx.ByteStream(b"synthetic encoded bytes"),
        )

    with httpx.Client(transport=httpx.MockTransport(handle)) as client:
        assert (
            CDNFreshness("https://cdn.invalid/", client).get(
                "https://cdn.invalid/images/card_s/1.webp?v=1"
            )
            is None
        )

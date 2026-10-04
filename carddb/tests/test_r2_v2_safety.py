"""Small, independent counterexamples for metadata and inventory boundaries."""

from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.r2_upload.v2.adapter import LEASE_HEADERS, LEASE_KEY, R2Store, _page
from sve_carddb.r2_upload.v2.freshness import CDNFreshness
from sve_carddb.snapshot.publish.storage import PublishError, Stored
from sve_carddb.snapshot.values import canonical

from .r2_v2_fixtures import ACCOUNT, BUCKET, CREDENTIALS, NOW
from .r2_v2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated loopback server
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote

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
    with pytest.raises(PublishError, match=r"^R2 inventory contains invalid keys$"):
        _page(raw.encode(), PREFIX)


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
        store = R2Store(ACCOUNT, BUCKET, CREDENTIALS, client, lambda: NOW)
        with pytest.raises(
            PublishError, match=r"^R2 inventory contains repeated keys$"
        ):
            store.keys(PREFIX)
    assert calls == 2


def test_public_put_refuses_noncontractual_metadata(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _transport = remote
    with (
        store.exclusive(),
        pytest.raises(PublishError, match=r"^Unsupported object metadata$"),
    ):
        store.put(
            KEY,
            b"synthetic",
            {"content-type": "application/json", "x-amz-meta-extra": "unknown"},
            expected=None,
        )
    assert KEY not in state.objects
    assert not any(op[1] == KEY for op in state.operations)


def test_idle_lease_with_wrong_metadata_cannot_be_claimed(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _transport = remote
    old = Stored(
        canonical({"format": 1, "owner": None}),
        '"idle"',
        LEASE_HEADERS | {"cache-control": "public"},
    )
    state.objects[LEASE_KEY] = old
    with (
        pytest.raises(
            PublishError,
            match=r"^Deployment writer lease is active or invalid; operator recovery required$",
        ),
        store.exclusive(),
    ):
        pytest.fail("invalid idle metadata admitted a writer")
    assert state.objects[LEASE_KEY] == old
    assert state.operations == []


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

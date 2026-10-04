"""No external network: localhost S3 tests and redacted failure counterexamples."""

import gzip
from typing import TYPE_CHECKING

import httpx
import pytest

from sve_carddb.r2_upload.v2 import adapter
from sve_carddb.r2_upload.v2.adapter import LEASE_HEADERS, LEASE_KEY, R2Store, _page
from sve_carddb.r2_upload.v2.freshness import CDNFreshness, cdn_root
from sve_carddb.snapshot.publish.storage import PublishError, Stored
from sve_carddb.snapshot.values import canonical

from .r2_v2_fixtures import ACCOUNT, BUCKET, CREDENTIALS, NOW
from .r2_v2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- register shared pytest fixture
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- register shared pytest fixture

if TYPE_CHECKING:
    from .r2_v2_fixtures import Loopback, ServerState

META = {"content-type": "application/json", "cache-control": "no-store"}
KEY = "snapshots/blobs/" + "a" * 64 + ".json"


@pytest.mark.parametrize(
    ("account", "bucket", "message"),
    [
        (ACCOUNT + ".evil.example/x?", BUCKET, "Invalid explicit R2 account ID"),
        ("evil." + ACCOUNT, BUCKET, "Invalid explicit R2 account ID"),
        (ACCOUNT, BUCKET + ".evil.example/x?", "Invalid explicit R2 bucket"),
        (ACCOUNT, "https://evil.example/" + BUCKET, "Invalid explicit R2 bucket"),
    ],
)
def test_endpoint_rejects_prefix_or_suffix_injection_without_http(
    remote: tuple[R2Store, ServerState, Loopback],
    account: str,
    bucket: str,
    message: str,
) -> None:
    store, state, _transport = remote
    with pytest.raises(PublishError, match="^" + message + "$"):
        R2Store(account, bucket, CREDENTIALS, store.client)
    assert state.requests == []


@pytest.mark.parametrize(
    "key", ["snapshots/./blobs/x", "snapshots/../blobs/x", "./x", "../x"]
)
def test_dot_segments_in_keys_are_rejected_before_signed_http(
    remote: tuple[R2Store, ServerState, Loopback],
    key: str,
) -> None:
    store, state, _transport = remote
    with pytest.raises(PublishError, match=r"^Invalid S3 object key$"):
        store.get(key)
    assert state.requests == []


def test_conditional_create_overwrite_and_stale_etag_are_atomic(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    with store.exclusive():
        assert store.get(KEY) is None
        assert store.put(KEY, b"old", META, expected=None)
        first = store.get(KEY)
        assert first is not None
        assert first.raw == b"old"
        assert not store.put(KEY, b"intruder", META, expected=None)
        assert store.get(KEY) == first
        assert store.put(KEY, b"new", META, expected=first.etag)
        newer = store.get(KEY)
        assert newer is not None
        assert newer.raw == b"new"
        assert newer.etag != first.etag
        assert not store.put(KEY, b"stale", META, expected=first.etag)
        assert store.get(KEY) == newer
    assert state.objects[LEASE_KEY].raw == canonical({"format": 1, "owner": None})
    assert [op[2] for op in state.operations if op[1] == KEY] == [
        "*",
        "*",
        first.etag,
        first.etag,
    ]


def test_compressed_sibling_get_preserves_wire_bytes(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    raw = gzip.compress(b"synthetic raw JSON", mtime=0)
    state.objects[KEY + ".gz"] = Stored(
        raw, '"compressed"', META | {"content-encoding": "gzip"}
    )
    actual = store.get(KEY + ".gz")
    assert actual is not None
    assert actual.raw == raw
    assert actual.headers == META | {"content-encoding": "gzip"}


def test_list_pagination_signs_exact_query_and_preserves_key_names(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    expected = tuple(
        "snapshots/blobs/" + k
        for k in ("a.json", "b&c.json", "d+e.json", "f.json", "g.json")
    )
    state.objects.update({k: Stored(b"x", '"x"', META) for k in expected})
    state.objects["private/not-for-list"] = Stored(b"private", '"p"', META)
    assert store.keys("snapshots/blobs/") == expected
    assert len(state.requests) == 3
    assert all("prefix=snapshots%2Fblobs%2F" in path for _, path, _ in state.requests)
    assert "continuation-token=2" in state.requests[1][1]
    assert "continuation-token=4" in state.requests[2][1]


def test_cross_adapter_lease_excludes_second_writer_and_is_reusable(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    first, state, _ = remote
    second = R2Store(ACCOUNT, BUCKET, CREDENTIALS, first.client, lambda: NOW)
    with (
        first.exclusive(),
        pytest.raises(
            PublishError,
            match=r"^Deployment writer lease is active or invalid; operator recovery required$",
        ),
    ):
        with second.exclusive():
            pytest.fail("concurrent lease acquired")
    with second.exclusive():
        assert state.objects[LEASE_KEY].raw != canonical({"format": 1, "owner": None})
    assert state.objects[LEASE_KEY].raw == canonical({"format": 1, "owner": None})


def test_stale_acquisition_cannot_overwrite_a_live_owner(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    assert store.put(
        LEASE_KEY, canonical({"format": 1, "owner": None}), LEASE_HEADERS, expected=None
    )
    idle = store.get(LEASE_KEY)
    assert idle is not None
    with store.exclusive():
        owner = state.objects[LEASE_KEY]
        assert not store.put(
            LEASE_KEY, b"stale owner", LEASE_HEADERS, expected=idle.etag
        )
        assert state.objects[LEASE_KEY] == owner


def test_lost_lease_put_response_never_retries_or_guesses_unlock(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, transport = remote
    transport.lose_next_put = True
    with pytest.raises(PublishError, match=r"^R2 transport or protocol failed$"):
        with store.exclusive():
            pytest.fail("uncertain owner may not proceed")
    assert len(state.operations) == 1
    assert state.objects[LEASE_KEY].raw != canonical({"format": 1, "owner": None})
    with pytest.raises(
        PublishError,
        match=r"^Deployment writer lease is active or invalid; operator recovery required$",
    ):
        with store.exclusive():
            pytest.fail("no automatic takeover")


def test_public_put_requires_intact_writer_lease(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    with pytest.raises(
        PublishError, match=r"^Deployment writer lease changed; stop and review$"
    ):
        store.put(KEY, b"x", META, expected=None)
    assert state.operations == []

    def external_change() -> None:
        with store.exclusive():
            state.objects[LEASE_KEY] = Stored(
                b"foreign owner", '"foreign"', LEASE_HEADERS
            )
            store.put(KEY, b"x", META, expected=None)

    with pytest.raises(
        PublishError, match=r"^Deployment writer lease changed; stop and review$"
    ):
        external_change()
    assert KEY not in state.objects
    assert state.objects[LEASE_KEY].raw == b"foreign owner"


@pytest.mark.parametrize("status", [301, 403, 409, 500, 501])
def test_unsupported_or_failed_conditions_have_no_fallback_and_no_secret_output(
    remote: tuple[R2Store, ServerState, Loopback], status: int
) -> None:
    store, state, _ = remote
    with store.exclusive():
        state.fail_put = status
        try:
            with pytest.raises(
                PublishError,
                match=r"^R2 conditional PUT failed; no unconditional fallback$",
            ) as exc:
                store.put(KEY, b"candidate", META, expected=None)
            assert "synthetic-secret" not in str(exc.value)
            assert "synthetic-access" not in str(exc.value)
            assert "Signature=" not in str(exc.value)
        finally:
            state.fail_put = None
    assert KEY not in state.objects


def test_conditional_delete_is_disabled_without_even_a_read_or_delete(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    with pytest.raises(
        PublishError,
        match=r"^R2 conditional DELETE is unverified; use separately approved GC$",
    ):
        store.delete(KEY, expected='"x"')
    assert state.requests == []


@pytest.mark.parametrize(
    "prefix", ["", "raw/", "images/", "snapshots/", "coordination/", "../"]
)
def test_inventory_cannot_expand_its_public_scope(
    remote: tuple[R2Store, ServerState, Loopback], prefix: str
) -> None:
    store, state, _ = remote
    with pytest.raises(PublishError, match=r"^List prefix is not explicitly public$"):
        store.keys(prefix)
    assert state.requests == []


@pytest.mark.parametrize(
    ("xml", "message"),
    [
        (
            b'<!DOCTYPE ListBucketResult [<!ENTITY s "snapshots/blobs/x">]><ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Prefix>snapshots/blobs/</Prefix><Contents><Key>&s;</Key></Contents><IsTruncated>false</IsTruncated></ListBucketResult>',
            "R2 inventory XML declarations are forbidden",
        ),
        (
            '<?xml version="1.0" encoding="UTF-16"?><x/>'.encode("utf-16"),
            "R2 inventory XML must be UTF-8",
        ),
        (b"<x>", "R2 inventory XML is invalid"),
        (b"<x/>", "R2 inventory prefix differs from request"),
    ],
    ids=["entity", "utf16", "malformed", "wrong-root"],
)
def test_inventory_xml_refuses_entities_unknown_root_and_malformed_bytes(
    xml: bytes, message: str
) -> None:
    with pytest.raises(PublishError, match=r"^" + message + "$"):
        _page(xml, "snapshots/blobs/")


@pytest.mark.parametrize(
    "root",
    [
        "http://cdn.invalid/",
        "https://user:pass@cdn.invalid/",
        "https://cdn.invalid/?key=secret",
        "https://cdn.invalid/#fragment",
        "https://cdn.invalid",
        "https://cdn.invalid:bad/",
    ],
)
def test_cdn_root_rejects_secret_bearing_or_ambiguous_urls(root: str) -> None:
    with pytest.raises(PublishError, match=r"^Explicit HTTPS CDN root required$"):
        cdn_root(root)


def test_cdn_full_query_is_preserved_without_auth_bypass_or_redirect(
    server: tuple[ServerState, Loopback],
) -> None:
    state, transport = server
    state.cdn = True
    state.objects["images/card_s/1.webp"] = Stored(
        b"image bytes", '"e"', {"content-type": "image/webp"}
    )
    with httpx.Client(
        transport=transport,
        trust_env=False,
        auth=("synthetic-access", "synthetic-secret"),
    ) as client:
        cdn = CDNFreshness("https://cdn.invalid/", client)
        for token in (1, 2):
            assert (
                cdn.get(f"https://cdn.invalid/images/card_s/1.webp?v={token}")
                == b"image bytes"
            )
        state.status = 302
        assert cdn.get("https://cdn.invalid/images/card_s/1.webp?v=3") is None
    assert [path for _, path, _ in state.requests] == [
        f"/images/card_s/1.webp?v={v}" for v in (1, 2, 3)
    ]
    assert all(
        headers["accept-encoding"] == "identity" for _, _, headers in state.requests
    )
    assert all(
        not ({"authorization", "cache-control", "pragma"} & headers.keys())
        for _, _, headers in state.requests
    )


@pytest.mark.parametrize(
    "url",
    [
        "https://foreign.invalid/images/card_s/1.webp?v=1",
        "https://cdn.invalid/images/card_s/1.webp",
        "https://cdn.invalid/images/card_s/1.webp?v=1#x",
        "images/card_s/1.webp?v=1",
    ],
)
def test_cdn_refuses_unpinned_or_queryless_urls(
    server: tuple[ServerState, Loopback], url: str
) -> None:
    state, transport = server
    with (
        httpx.Client(transport=transport, trust_env=False) as client,
        pytest.raises(
            PublishError, match=r"^CDN URL is outside the pinned query-bearing root$"
        ),
    ):
        CDNFreshness("https://cdn.invalid/", client).get(url)
    assert state.requests == []


@pytest.mark.parametrize("kind", ["missing-etag", "weak-etag", "overflow"])
def test_object_reads_require_strong_etag_and_bounded_bytes(
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
    kind: str,
) -> None:

    store, state, _ = remote
    etag = (
        ""
        if kind == "missing-etag"
        else 'W/"weak"'
        if kind == "weak-etag"
        else '"strong"'
    )
    state.objects[KEY] = Stored(b"12345", etag, META)
    if kind == "overflow":
        monkeypatch.setattr(adapter, "MAX_OBJECT", 4)
    with pytest.raises(
        PublishError,
        match=r"^Remote response exceeds the configured byte limit$"
        if kind == "overflow"
        else r"^R2 object requires a strong opaque ETag$",
    ):
        store.get(KEY)


@pytest.mark.parametrize("expected", ["", "*", 'W/"weak"'])
def test_put_refuses_non_object_preconditions_before_http(
    remote: tuple[R2Store, ServerState, Loopback], expected: str
) -> None:
    store, state, _ = remote
    with store.exclusive():
        count = len(state.operations)
        with pytest.raises(
            PublishError, match=r"^Conditional PUT requires an opaque object ETag$"
        ):
            store.put(KEY, b"x", META, expected=expected)
        assert len(state.operations) == count


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("prefix", "R2 inventory prefix differs from request"),
        ("key", "R2 inventory contains invalid keys"),
        ("flag", "R2 inventory lacks a truncation flag"),
        ("token", "R2 inventory lacks a continuation token"),
    ],
)
def test_inventory_rejects_out_of_scope_and_incomplete_pages(
    change: str, message: str
) -> None:
    prefix = "snapshots/blobs/"
    raw = (
        '<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Prefix>'
        + ("private/" if change == "prefix" else prefix)
        + "</Prefix><Contents><Key>"
        + ("private/x" if change == "key" else prefix + "x")
        + "</Key></Contents><IsTruncated>"
        + ("" if change == "flag" else "true" if change == "token" else "false")
        + "</IsTruncated></ListBucketResult>"
    )
    with pytest.raises(PublishError, match="^" + message + "$"):
        _page(raw.encode(), prefix)


def test_list_repeating_token_stops_without_infinite_requests() -> None:
    calls = 0

    def handle(_request: httpx.Request) -> httpx.Response:
        nonlocal calls
        calls += 1
        if calls > 2:
            return httpx.Response(
                500, stream=httpx.ByteStream(b"bounded synthetic failure")
            )
        raw = (
            '<ListBucketResult xmlns="http://s3.amazonaws.com/doc/2006-03-01/"><Prefix>snapshots/blobs/</Prefix><Contents><Key>snapshots/blobs/'
            + str(calls)
            + "</Key></Contents><IsTruncated>true</IsTruncated><NextContinuationToken>same-token</NextContinuationToken></ListBucketResult>"
        )
        return httpx.Response(200, stream=httpx.ByteStream(raw.encode()))

    with httpx.Client(transport=httpx.MockTransport(handle)) as client:
        store = R2Store(ACCOUNT, BUCKET, CREDENTIALS, client, lambda: NOW)
        with pytest.raises(
            PublishError, match=r"^R2 inventory pagination repeats a token$"
        ):
            store.keys("snapshots/blobs/")
    assert calls == 2


def test_racing_lease_creation_stops_on_412_before_the_writer_enters(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    state.race_lease = True
    with pytest.raises(
        PublishError, match=r"^Deployment writer lease was acquired concurrently$"
    ):
        with store.exclusive():
            pytest.fail("racing lease may not enter")
    assert state.objects[LEASE_KEY].raw == b"foreign active owner"
    assert all(op[1] == LEASE_KEY for op in state.operations)


def test_acquisition_readback_must_match_claimed_owner(
    remote: tuple[R2Store, ServerState, Loopback], monkeypatch: pytest.MonkeyPatch
) -> None:
    store, state, _ = remote
    real_get = store.get
    calls = 0

    def get(key: str) -> Stored | None:
        nonlocal calls
        calls += 1
        if calls == 2:
            return Stored(b"foreign bytes", '"foreign"', LEASE_HEADERS)
        return real_get(key)

    monkeypatch.setattr(store, "get", get)
    with pytest.raises(
        PublishError, match=r"^Deployment writer lease acquisition is uncertain$"
    ):
        with store.exclusive():
            pytest.fail("unverified owner may not enter")
    assert state.objects[LEASE_KEY].raw != canonical({"format": 1, "owner": None})

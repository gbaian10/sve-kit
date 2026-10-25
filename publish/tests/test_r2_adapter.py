"""No external network: localhost S3 tests and redacted failure counterexamples."""

from threading import Event
from typing import TYPE_CHECKING

import brotli
import httpx
import pytest
from sve_carddb.export.read_api import ExportError

from sve_publish import adapter
from sve_publish.adapter import MAX_DELETE_BATCH, R2Store, Stored
from sve_publish.parallel import UploadObject, UploadPool
from sve_publish.sdk import BoundaryError

from .r2_fixtures import ACCOUNT, BUCKET, CREDENTIALS
from .r2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- register shared pytest fixture
from .r2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- register shared pytest fixture
from .r2_sdk_fixtures import inventory, mock_client

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

if TYPE_CHECKING:
    from .r2_fixtures import Loopback, ServerState

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
    with pytest.raises(ExportError, match="^" + message + "$"):
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
    with pytest.raises(ExportError, match=r"^Invalid S3 object key$"):
        store.get(key)
    assert state.requests == []


def test_conditional_create_overwrite_and_stale_etag_are_atomic(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
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
    raw = brotli.compress(b"synthetic raw JSON")
    state.objects[KEY + ".br"] = Stored(
        raw, '"compressed"', META | {"content-encoding": "br"}
    )
    actual = store.get(KEY + ".br")
    assert actual is not None
    assert actual.raw == raw
    assert actual.headers == META | {"content-encoding": "br"}


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


@pytest.mark.parametrize("status", [301, 403, 409, 500, 501])
def test_unsupported_or_failed_conditions_have_no_fallback_and_no_secret_output(
    remote: tuple[R2Store, ServerState, Loopback], status: int
) -> None:
    store, state, _ = remote
    state.fail_put = status
    try:
        with pytest.raises(
            ExportError,
            match=r"^R2 conditional PUT failed; no unconditional fallback$",
        ) as exc:
            store.put(KEY, b"candidate", META, expected=None)
        assert "synthetic-secret" not in str(exc.value)
        assert "synthetic-access" not in str(exc.value)
        assert "Signature=" not in str(exc.value)
    finally:
        state.fail_put = None
    assert KEY not in state.objects


def test_image_put_retries_temporary_failure_and_reports_progress(
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, _ = remote
    key = "images/card_l/63603.webp"
    state.put_statuses = [503]
    lines: list[str] = []
    delays: list[float] = []
    pool = UploadPool(
        store,
        1,
        1,
        lines.append,
    )
    # Inject sleep at the adapter boundary so the test never waits.
    monkeypatch.setattr(adapter, "sleep_for", delays.append)
    pool.sync([UploadObject(key, lambda: b"synthetic image", {}, image=True)])
    assert [request[0] for request in state.requests] == ["PUT", "PUT"]
    assert state.objects[key].raw == b"synthetic image"
    assert delays == [1]
    assert any(
        "Retrying image PUT" in line and key in line and "503" in line for line in lines
    )


def test_image_put_stops_after_three_server_errors_with_status(
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, _ = remote
    key = "images/card_l/63603.webp"
    state.put_statuses = [503, 503, 503]
    delays: list[float] = []
    monkeypatch.setattr(adapter, "sleep_for", delays.append)
    with pytest.raises(ExportError, match=r"503.*" + key):
        UploadPool(store, 1, 1).sync(
            [UploadObject(key, lambda: b"synthetic image", {}, image=True)]
        )
    assert [request[0] for request in state.requests] == ["PUT"] * 3
    assert delays == [1, 4]


def test_image_put_does_not_retry_forbidden_or_boundary_error(
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, _ = remote
    key = "images/card_l/63603.webp"
    state.fail_put = 403
    with pytest.raises(ExportError, match=r"403.*" + key):
        UploadPool(store, 1, 1).sync(
            [UploadObject(key, lambda: b"synthetic image", {}, image=True)]
        )
    assert len(state.requests) == 1

    state.fail_put = None
    state.requests.clear()
    calls = 0

    def reject_put(**_kwargs: object) -> None:
        nonlocal calls
        calls += 1
        raise BoundaryError("synthetic boundary rejection")

    monkeypatch.setattr(store.client, "put_object", reject_put)
    with pytest.raises(ExportError, match=r"BoundaryError.*" + key):
        UploadPool(store, 1, 1).sync(
            [UploadObject(key, lambda: b"synthetic image", {}, image=True)]
        )
    assert calls == 1
    assert state.requests == []


def test_conditional_json_put_still_sends_once_on_server_error(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    state.fail_put = 503
    with pytest.raises(ExportError, match="conditional PUT failed"):
        store.put(KEY, b"synthetic json", META, expected=None)
    assert len(state.requests) == 1


def test_stopped_pool_does_not_start_an_image_retry(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    key = "images/card_l/63603.webp"
    state.put_statuses = [503]
    stopped = Event()

    def stop_during_backoff(_delay: float) -> None:
        stopped.set()

    with pytest.raises(ExportError, match="503"):
        store.put_image(
            key, b"synthetic image", stopped=stopped, sleep=stop_during_backoff
        )
    assert len(state.requests) == 1


@pytest.mark.parametrize(
    "prefix", ["", "raw/", "images/", "snapshots/", "coordination/", "../"]
)
def test_inventory_cannot_expand_its_public_scope(
    remote: tuple[R2Store, ServerState, Loopback], prefix: str
) -> None:
    store, state, _ = remote
    with pytest.raises(ExportError, match=r"^List prefix is not explicitly public$"):
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
    with pytest.raises(ExportError, match=r"^" + message + "$"):
        inventory(xml, "snapshots/blobs/")


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
        ExportError,
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
    count = len(state.operations)
    with pytest.raises(
        ExportError, match=r"^Conditional PUT requires an opaque object ETag$"
    ):
        store.put(KEY, b"x", META, expected=expected)
    assert len(state.operations) == count


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("prefix", "R2 inventory prefix differs from request"),
        ("key", "R2 inventory contains invalid keys"),
        ("flag", "R2 inventory lacks a truncation flag"),
        ("empty-flag", "R2 inventory lacks a truncation flag"),
        ("invalid-flag", "R2 inventory lacks a truncation flag"),
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
        + ("true" if change == "token" else "false")
        + "</IsTruncated></ListBucketResult>"
    )
    if change in {"flag", "empty-flag", "invalid-flag"}:
        raw = raw.replace(
            "<IsTruncated>false</IsTruncated>",
            ""
            if change == "flag"
            else "<IsTruncated></IsTruncated>"
            if change == "empty-flag"
            else "<IsTruncated>invalid</IsTruncated>",
        )
    with pytest.raises(ExportError, match="^" + message + "$"):
        inventory(raw.encode(), prefix)


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
        store = R2Store(
            ACCOUNT,
            BUCKET,
            CREDENTIALS,
            mock_client(
                client, account=ACCOUNT, bucket=BUCKET, credentials=CREDENTIALS
            ),
        )
        with pytest.raises(
            ExportError, match=r"^R2 inventory pagination repeats a token$"
        ):
            store.keys("snapshots/blobs/")
    assert calls == 2


@pytest.mark.parametrize(
    "key", ["private/x.json", "snapshots/versions/index.json", "images/card_s/0.webp"]
)
def test_delete_refuses_non_public_keys_before_http(
    remote: tuple[R2Store, ServerState, Loopback], key: str
) -> None:
    store, state, _ = remote
    with pytest.raises(
        ExportError, match=r"^Deletion key is outside the public namespaces$"
    ):
        store.delete_many([KEY, key])
    assert state.requests == []


def test_delete_removes_one_public_object_without_condition(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    state.objects[KEY] = Stored(b"x", '"x"', META)
    store.delete_many([KEY])
    assert KEY not in state.objects
    assert state.operations == [("DELETE", KEY, None)]


@pytest.mark.parametrize("count", [0, MAX_DELETE_BATCH + 1])
def test_delete_refuses_invalid_batch_size_before_http(
    remote: tuple[R2Store, ServerState, Loopback], count: int
) -> None:
    store, state, _ = remote
    with pytest.raises(ExportError, match=f"requires 1 to {MAX_DELETE_BATCH} keys"):
        store.delete_many([KEY] * count)
    assert state.requests == []


def test_delete_reports_partial_errors_and_rerun_removes_remaining_objects(
    remote: tuple[R2Store, ServerState, Loopback],
) -> None:
    store, state, _ = remote
    other = "images/card_s/999.webp"
    state.objects.update({key: Stored(b"x", '"x"', META) for key in (KEY, other)})
    state.delete_errors.add(other)
    with pytest.raises(
        ExportError, match=r"^R2 DELETE failed for 1 objects; inspect and rerun GC$"
    ):
        store.delete_many([KEY, other])
    assert set(state.objects) == {other}
    state.delete_errors.clear()
    store.delete_many([other])
    assert not state.objects

"""Public closure, create-only S3 writes and atomic pointer failure counterexamples."""

import json
from dataclasses import replace
from functools import partial
from typing import TYPE_CHECKING

import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.r2_upload.plan import POINTER, UploadError, pointer_value, read_member
from sve_carddb.r2_upload.s3 import S3, Credentials, sign
from sve_carddb.r2_upload.upload import upload
from sve_carddb.snapshot.values import canonical

from .r2_upload_fixtures import (
    ACCOUNT,
    BUCKET,
    CREDENTIALS,
    NOW,
    Store,
    verify_signature,
)
from .r2_upload_fixtures import local as local  # ruff: ignore[useless-import-alias] -- register shared test fixtures
from .r2_upload_fixtures import previous as previous  # ruff: ignore[useless-import-alias] -- genuinely complete old published version
from .r2_upload_fixtures import public_plan as public_plan  # ruff: ignore[useless-import-alias] -- register the validated immutable module base
from .r2_upload_fixtures import public_template as public_template  # ruff: ignore[useless-import-alias] -- register shared test fixtures
from .test_snapshot_preview_images import images as images  # ruff: ignore[useless-import-alias] -- register the synthetic image template

if TYPE_CHECKING:
    from sve_carddb.r2_upload.plan import Plan


def seed(store: Store, local: Plan) -> None:
    for member in local.members:
        store.save(
            member.key,
            read_member(local.root, member.key),
            {
                "content-type": member.content_type,
                "cache-control": member.cache_control,
            },
        )


def test_first_upload_publishes_complete_members_then_pointer_and_rerun_skips(
    local: Plan,
) -> None:
    store = Store()
    first = upload(local, store.remote())
    assert first["uploaded_files"] == len(local.members)
    assert first["uploaded_bytes"] == sum(m.size for m in local.members)
    assert first["skipped_files"] == 0
    put_keys = [key for method, key in store.calls if method == "PUT"]
    assert put_keys == [m.key for m in local.members]
    assert put_keys[-1] == POINTER
    for m in local.members:
        assert store.objects[m.key][0] == read_member(local.root, m.key)
    before = len(store.calls)
    again = upload(local, store.remote())
    assert again["uploaded_files"] == 0
    assert again["skipped_files"] == len(local.members)
    assert again["skipped_bytes"] == first["candidate_bytes"]
    assert all(method == "GET" for method, _ in store.calls[before:])


@pytest.mark.parametrize(
    "damage", ["bytes", "content-type", "cache-control", "content-encoding", "oversize"]
)
def test_existing_objects_are_verified_and_never_overwritten(
    local: Plan, damage: str
) -> None:
    store = Store()
    member = local.members[0]
    raw = read_member(local.root, member.key)
    headers = {
        "content-type": member.content_type,
        "cache-control": member.cache_control,
    }
    if damage == "bytes":
        raw = bytes(len(raw))
    elif damage == "oversize":
        raw += b"x"
    else:
        headers[damage] = "synthetic wrong value"
    store.save(member.key, raw, headers)
    messages = {
        "bytes": "Existing R2 object differs from the local public member",
        "oversize": "R2 object exceeds expected size",
        "content-encoding": "R2 object has unexpected content encoding",
    }
    with pytest.raises(
        UploadError,
        match="^"
        + messages.get(
            damage, "Existing R2 object metadata differs from the upload contract"
        )
        + "$",
    ):
        upload(local, store.remote())
    assert store.objects[member.key][0] == raw
    assert not any(method == "PUT" for method, _ in store.calls)
    assert POINTER not in store.objects


@pytest.mark.parametrize("same", [True, False])
def test_conditional_create_race_verifies_the_winner(local: Plan, same: bool) -> None:
    store = Store()
    key = local.members[0].key

    def race(request: httpx.Request, requested: str) -> httpx.Response | None:
        if request.method == "PUT" and requested == key:
            store.save(
                key,
                request.content if same else bytes(len(request.content)),
                {
                    "content-type": request.headers["content-type"],
                    "cache-control": request.headers["cache-control"],
                },
            )
            store.intervene = None
            return httpx.Response(412)
        return None

    store.intervene = race
    if same:
        result = upload(local, store.remote())
        assert result["skipped_files"] == 1
        assert result["uploaded_files"] == len(local.members) - 1
    else:
        with pytest.raises(
            UploadError,
            match=r"^Existing R2 object differs from the local public member$",
        ):
            upload(local, store.remote())
        assert POINTER not in store.objects


@pytest.mark.parametrize("phase", [0, 1, 2, 3])
def test_interruption_keeps_old_pointer_and_resume_completes(
    local: Plan, previous: Plan, phase: int
) -> None:
    store = Store()
    seed(store, previous)
    old = store.objects[POINTER][0]
    target = next(
        m.key
        for m in local.members
        if m.phase == phase and (m.key not in store.objects or m.key == POINTER)
    )

    def interrupt(request: httpx.Request, key: str) -> httpx.Response | None:
        if request.method == "PUT" and key == target:
            raise KeyboardInterrupt
        return None

    store.intervene = interrupt
    with pytest.raises(KeyboardInterrupt):
        upload(local, store.remote())
    assert store.objects[POINTER][0] == old
    before = {
        m.key for m in local.members if m.key != POINTER and m.key in store.objects
    }
    store.intervene = None
    result = upload(local, store.remote())
    assert result["skipped_files"] == len(before)
    assert store.objects[POINTER][0] == read_member(local.root, POINTER)
    assert all(m.key in store.objects for m in local.members)


def test_pointer_compare_and_swap_never_overwrites_a_concurrent_publisher(
    local: Plan,
    previous: Plan,
) -> None:
    store = Store()
    seed(store, previous)
    old = store.objects[POINTER][0]

    def change(request: httpx.Request, key: str) -> httpx.Response | None:
        if request.method == "PUT" and key == POINTER:
            store.save(
                POINTER,
                old,
                {"content-type": "application/json", "cache-control": "no-store"},
            )
            store.intervene = None
        return None

    store.intervene = change
    with pytest.raises(
        UploadError, match=r"^Preview pointer changed concurrently; rerun after review$"
    ):
        upload(local, store.remote())
    assert store.objects[POINTER][0] == old


def test_pointer_without_etag_stops_before_any_member_put(local: Plan) -> None:
    store = Store()
    seed(store, local)
    store.objects[POINTER][1].pop("etag")
    with pytest.raises(UploadError, match=r"^Existing preview pointer has no ETag$"):
        upload(local, store.remote())
    assert store.calls == [("GET", POINTER)]


@pytest.mark.parametrize("mutation", ["forged-plan", "before", "during"])
def test_local_changes_never_publish_pointer(local: Plan, mutation: str) -> None:
    store = Store()
    if mutation == "forged-plan":
        local = replace(local, members=local.members[1:])
        message = "Local public inventory changed"
    elif mutation == "before":
        (local.root / POINTER).write_bytes(b"corrupt")
        message = "Public preview validation failed"
    else:

        def change(request: httpx.Request, key: str) -> httpx.Response | None:
            if key == local.members[-2].key and request.method == "GET":
                (local.root / POINTER).write_bytes(b"corrupt")
                store.intervene = None
            return None

        store.intervene = change
        message = "Public preview validation failed"
    with pytest.raises(UploadError, match="^" + message + "$"):
        upload(local, store.remote())
    assert not any(method == "PUT" and key == POINTER for method, key in store.calls)
    if mutation != "during":
        assert store.calls == []


@pytest.mark.parametrize("status", [302, 403, 500])
def test_remote_failures_and_redirects_are_redacted_and_never_followed(
    local: Plan, status: int
) -> None:
    store = Store(
        intervene=lambda _request, _key: httpx.Response(
            status,
            content=b"synthetic secret server body",
            headers={"location": "https://external.invalid/"},
        )
    )
    with pytest.raises(UploadError, match=r"^R2 object read failed$"):
        upload(local, store.remote())
    assert store.calls == [("GET", POINTER)]


def test_transport_failure_is_redacted(local: Plan) -> None:
    def fail(request: httpx.Request, _key: str) -> httpx.Response | None:
        raise httpx.ReadTimeout("synthetic secret timeout", request=request)

    store = Store(intervene=fail)
    with pytest.raises(UploadError, match=r"^R2 transport failed$"):
        upload(local, store.remote())


@pytest.mark.parametrize(
    ("account", "bucket", "message"),
    [
        ("wrong", BUCKET, "R2 account ID must be 32 lowercase hexadecimal characters"),
        (ACCOUNT, "bad/path", "Invalid R2 bucket name"),
    ],
)
def test_endpoint_is_an_explicit_account_and_bucket(
    account: str, bucket: str, message: str
) -> None:
    with httpx.Client(
        transport=httpx.MockTransport(lambda _r: httpx.Response(404))
    ) as client:
        with pytest.raises(UploadError, match="^" + message + "$"):
            S3(account, bucket, CREDENTIALS, client)


def test_missing_credentials_and_representations_do_not_leak(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    monkeypatch.delenv("SVE_R2_ACCESS_KEY_ID", raising=False)
    monkeypatch.delenv("SVE_R2_SECRET_ACCESS_KEY", raising=False)
    with pytest.raises(
        UploadError, match=r"^Explicit local R2 credentials are required$"
    ):
        Credentials.environment()
    assert "synthetic-access" not in repr(CREDENTIALS)
    assert "synthetic-secret" not in repr(CREDENTIALS)


def test_pointer_and_member_helpers_do_not_accept_arbitrary_paths(local: Plan) -> None:
    with pytest.raises(UploadError, match=r"^Invalid preview pointer$"):
        pointer_value(
            canonical(
                {
                    "manifest_path": "snapshots/wrong.json",
                    "manifest_sha256": "sha256:" + "a" * 64,
                }
            )
        )
    with pytest.raises(UploadError, match=r"^Invalid public member key$"):
        read_member(local.root, "/private/input")
    remote = Store().remote()
    with pytest.raises(UploadError, match=r"^Invalid object key$"):
        remote.get("../private", limit=1)


def test_sigv4_is_deterministic_for_fixed_time_and_covers_payload() -> None:
    request = httpx.Request(
        "PUT",
        "https://"
        + ACCOUNT
        + ".r2.cloudflarestorage.com/"
        + BUCKET
        + "/snapshots/example.json",
        headers={
            "if-none-match": "*",
            "content-type": "application/json",
            "cache-control": "no-store",
        },
        content=b"{}",
    )
    sign(request, CREDENTIALS, NOW)
    verify_signature(request)


def test_cli_execute_wires_scoped_s3_and_disables_ambient_transport(
    local: Plan, monkeypatch: pytest.MonkeyPatch
) -> None:
    store = Store()
    original = httpx.Client

    def factory(**kwargs: object) -> httpx.Client:
        assert kwargs == {"trust_env": False, "follow_redirects": False, "timeout": 30}
        return original(transport=httpx.MockTransport(store.handle))

    monkeypatch.setattr(httpx, "Client", factory)
    monkeypatch.setattr(
        "sve_carddb.r2_upload.commands.S3", partial(S3, clock=lambda: NOW)
    )
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", CREDENTIALS.access_key)
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", CREDENTIALS.secret_key)
    result = CliRunner().invoke(
        app,
        [
            "r2",
            "upload-preview",
            "--preview-dir",
            str(local.root),
            "--execute",
            "--confirm-maintainer-authorization",
            "--account-id",
            ACCOUNT,
            "--bucket",
            BUCKET,
        ],
    )
    assert result.exit_code == 0, result.output
    report = json.loads(result.output)
    assert report["mode"] == "execute"
    assert report["uploaded_files"] == report["candidate_files"]
    assert report["uploaded_bytes"] == report["candidate_bytes"]
    assert CREDENTIALS.secret_key not in result.output
    assert CREDENTIALS.access_key not in result.output
    assert list(store.objects)[-1] == POINTER


@pytest.mark.parametrize("status", [302, 403, 500])
def test_failed_conditional_write_stops_without_publishing_or_following(
    local: Plan, status: int
) -> None:
    store = Store(
        intervene=lambda request, _key: (
            httpx.Response(
                status,
                content=b"synthetic private error",
                headers={"location": "https://external.invalid/"},
            )
            if request.method == "PUT"
            else None
        )
    )
    with pytest.raises(UploadError, match=r"^R2 conditional object write failed$"):
        upload(local, store.remote())
    assert POINTER not in store.objects
    assert store.calls[-1] == ("PUT", local.members[0].key)


def test_pointer_timeout_after_server_commit_still_points_at_complete_members(
    local: Plan,
) -> None:
    store = Store()

    def timeout(request: httpx.Request, key: str) -> httpx.Response | None:
        if request.method == "PUT" and key == POINTER:
            assert all(
                m.key in store.objects for m in local.members if m.key != POINTER
            )
            store.save(
                POINTER,
                request.content,
                {"content-type": "application/json", "cache-control": "no-store"},
            )
            raise httpx.ReadTimeout("synthetic lost response", request=request)
        return None

    store.intervene = timeout
    with pytest.raises(UploadError, match=r"^R2 transport failed$"):
        upload(local, store.remote())
    assert store.objects[POINTER][0] == read_member(local.root, POINTER)
    store.intervene = None
    assert upload(local, store.remote())["uploaded_files"] == 0


def test_successful_put_response_without_verified_object_cannot_publish(
    local: Plan,
) -> None:
    store = Store(
        intervene=lambda request, _key: (
            httpx.Response(200) if request.method == "PUT" else None
        )
    )
    with pytest.raises(
        UploadError, match=r"^Existing R2 object differs from the local public member$"
    ):
        upload(local, store.remote())
    assert POINTER not in store.objects
    assert store.calls[-1] == ("GET", local.members[0].key)

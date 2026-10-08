"""Upload of export-offline roots against the localhost S3/CDN server."""

import gzip
import json
from copy import deepcopy
from typing import TYPE_CHECKING

import httpx
import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.json import canonical, object_value, parse, string
from sve_carddb.r2_upload.sdk import Credentials
from sve_carddb.r2_upload.v2.adapter import Stored
from sve_carddb.r2_upload.v2.freshness import CDNFreshness
from sve_carddb.r2_upload.v2.headers import member_headers
from sve_carddb.r2_upload.v2.publish import next_index, upload
from sve_carddb.snapshot.export.compression import python_brotli
from sve_carddb.snapshot.read_api import INDEX, POINTER, load_export
from sve_carddb.snapshot.read_api import ExportError as UploadError

from .r2_sdk_fixtures import install_mock_sdk
from .r2_v2_export_fixtures import art_changed as art_changed  # ruff: ignore[useless-import-alias] -- module-scoped crop-change corpus
from .r2_v2_export_fixtures import export
from .r2_v2_export_fixtures import images as images  # ruff: ignore[useless-import-alias] -- module-scoped synthetic corpus
from .r2_v2_export_fixtures import roots as roots  # ruff: ignore[useless-import-alias] -- per-test public and private roots
from .r2_v2_fixtures import ACCOUNT, BUCKET
from .r2_v2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated loopback server
from .r2_v2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

if TYPE_CHECKING:
    from collections.abc import Iterator

    from pydantic import JsonValue

    from sve_carddb.r2_upload.v2.adapter import R2Store
    from sve_carddb.snapshot.preview import Roots

    from .r2_v2_fixtures import Loopback, ServerState
    from .test_snapshot_preview_images import PublicImages

CDN = "https://cdn.invalid/"


@pytest.fixture
def cdn(remote: tuple[R2Store, ServerState, Loopback]) -> Iterator[CDNFreshness]:
    with httpx.Client(transport=remote[2], trust_env=False) as client:
        yield CDNFreshness(CDN, client)


def puts(state: ServerState, start: int = 0) -> list[str]:
    return [key for op, key, _ in state.operations[start:] if op == "PUT"]


def current(state: ServerState) -> dict[str, JsonValue]:
    return object_value(object_value(parse(state.objects[INDEX].raw))["current"])


def test_first_upload_selects_only_the_public_closure_and_writes_index_last(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    cdn: CDNFreshness,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    stale = {
        p.relative_to(roots.preview).as_posix()
        for p in roots.preview.rglob("*")
        if p.is_file()
    }
    text = deepcopy(images.projection)
    text.tables["printing"][0]["rarity_raw"] = "Synthetic second export"
    export(images, roots, step=1, projection=text)
    loaded = load_export(roots.preview)
    result = upload(store, loaded, cdn)
    assert result["index"] == "updated"
    assert result["cdn_verification"] == "verified"
    expected = (
        {m.key for m in loaded.members} | {i.key for i in loaded.images} | {INDEX}
    )
    assert set(state.objects) == expected
    assert stale - expected - {POINTER}
    private = {
        p.relative_to(roots.private).as_posix()
        for p in roots.private.rglob("*")
        if p.is_file()
    }
    assert "media-state.json" in private
    assert not private & set(state.objects)
    assert not any((roots.preview / name).exists() for name in private)
    assert puts(state)[-1] == INDEX
    index = object_value(parse(state.objects[INDEX].raw))
    assert index == {
        "index_format": 2,
        "revision": 1,
        "current": loaded.entry,
        "previous": None,
    }
    assert state.objects[INDEX].headers == {
        "content-type": "application/json",
        "cache-control": "no-store",
    }
    for member in loaded.members:
        assert state.objects[member.key].headers == member_headers(member)
    gz = next(m for m in loaded.members if m.key.endswith(".gz"))
    assert member_headers(gz)["content-encoding"] == "gzip"
    image = state.objects[loaded.images[0].key]
    assert image.headers == {
        "content-type": "image/webp",
        "cache-control": "public,max-age=86400,must-revalidate",
    }


def test_unchanged_rerun_makes_zero_puts(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    cdn: CDNFreshness,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    upload(store, load_export(roots.preview), cdn)
    before = len(state.operations)
    index = state.objects[INDEX]
    result = upload(store, load_export(roots.preview), None)
    assert puts(state, before) == []
    assert state.objects[INDEX] == index
    assert result["written_files"] == 0
    assert result["index"] == "unchanged"


@pytest.mark.parametrize("failed", ["image", "manifest"])
def test_partial_failure_keeps_old_index_and_rerun_uploads_the_rest(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    failed: str,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    key = (
        loaded.images[2].key
        if failed == "image"
        else string(loaded.entry["manifest_path"])
    )
    state.fail_key = key
    with pytest.raises(UploadError, match="no unconditional fallback"):
        upload(store, loaded, None)
    assert INDEX not in state.objects
    done = set(state.objects)
    assert done
    state.fail_key = None
    before = len(state.operations)
    upload(store, loaded, None)
    rerun = puts(state, before)
    assert key in rerun
    assert not done & set(rerun)
    assert rerun[-1] == INDEX


def test_changed_art_gets_new_version_and_unchanged_card_keeps_it(
    images: PublicImages,
    art_changed: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    cdn: CDNFreshness,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    first = load_export(roots.preview)
    upload(store, first, cdn)
    export(art_changed, roots, step=1, library=art_changed.library)
    second = load_export(roots.preview)
    before = len(state.operations)
    upload(store, second, cdn)
    written = [k for k in puts(state, before) if k.startswith("images/")]
    assert written
    assert all(k.startswith("images/art_") for k in written)
    old = {i.key: i.url for i in first.images}
    for item in second.images:
        if item.key.startswith("images/card_"):
            assert item.url == old[item.key]
        else:
            assert item.url != old[item.key]
    index = object_value(parse(state.objects[INDEX].raw))
    assert index["revision"] == 2
    assert index["current"] == second.entry
    assert index["previous"] == first.entry


def test_failed_cdn_check_keeps_index_and_rerun_switches_it(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    cdn: CDNFreshness,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    first = load_export(roots.preview)
    upload(store, first, cdn)
    text = deepcopy(images.projection)
    text.tables["printing"][0]["rarity_raw"] = "Synthetic second export"
    export(images, roots, step=1, projection=text)
    second = load_export(roots.preview)
    state.cdn_status = 404
    with pytest.raises(UploadError, match=r"^CDN full-URL verification failed$"):
        upload(store, second, cdn)
    assert current(state) == first.entry
    state.cdn_status = None
    upload(store, second, cdn)
    assert current(state) == second.entry


def test_conflicting_immutable_json_is_never_overwritten(
    images: PublicImages, roots: Roots, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    member = loaded.members[0]
    state.objects[member.key] = Stored(b"other", '"other"', member_headers(member))
    with pytest.raises(
        UploadError, match=r"^Immutable JSON object differs from the export$"
    ):
        upload(store, loaded, None)
    assert state.objects[member.key].raw == b"other"
    assert INDEX not in state.objects


def test_brotli_siblings_are_uploaded_as_exported(
    images: PublicImages, roots: Roots, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, _ = remote
    export(images, roots, step=0, brotli=python_brotli())
    loaded = load_export(roots.preview)
    brotli = [m for m in loaded.members if m.key.endswith(".br")]
    assert any(m.key == string(loaded.entry["manifest_path"]) + ".br" for m in brotli)
    upload(store, loaded, None)
    for member in brotli:
        assert (
            state.objects[member.key].raw == (roots.preview / member.key).read_bytes()
        )
        assert state.objects[member.key].headers["content-encoding"] == "br"


def test_index_refuses_reused_or_older_versions() -> None:
    entry: dict[str, JsonValue] = {
        "data_version": "preview-20261004T010203Z-0001",
        "published_at": "2026-10-04T01:02:03Z",
        "manifest_path": "snapshots/manifests/" + "a" * 64 + ".json",
    }
    index: dict[str, JsonValue] = {
        "index_format": 2,
        "revision": 4,
        "current": entry,
        "previous": None,
    }
    assert next_index(index, entry) is None
    with pytest.raises(UploadError, match=r"^Data version is already published"):
        next_index(index, entry | {"manifest_path": "other"})
    newer = entry | {"data_version": "preview-20261005T010203Z-0001"}
    with pytest.raises(UploadError, match=r"^Data version is already published"):
        next_index(index | {"current": newer, "previous": entry}, entry | {"x": 1})
    older = entry | {
        "data_version": "preview-20261003T010203Z-0001",
        "published_at": "2026-10-03T01:02:03Z",
    }
    with pytest.raises(UploadError, match=r"^Export is older than the remote current$"):
        next_index(index, older)


@pytest.mark.parametrize("target", ["blob", "image", "pointer"])
def test_tampered_export_is_rejected_before_any_request(
    images: PublicImages, roots: Roots, target: str
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    if target == "blob":
        path = roots.preview / loaded.members[0].key
    elif target == "image":
        path = roots.preview / loaded.images[0].key
    else:
        path = roots.preview / POINTER
    raw = path.read_bytes()
    path.write_bytes(raw[:-1] if target == "image" else raw[:-1] + b"!")
    with pytest.raises(UploadError):
        load_export(roots.preview)


def test_interrupted_image_overwrite_leaves_no_pointer_to_upload(
    images: PublicImages,
    art_changed: PublicImages,
    roots: Roots,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    import sve_carddb.snapshot.preview as writer  # ruff: ignore[import-outside-top-level] -- fault only the output writer boundary

    export(images, roots, step=0)
    original = writer._write

    def interrupted(
        roots: Roots, path: str, raw: bytes, *, immutable: bool, private: bool = False
    ) -> None:
        if path.startswith("images/art_"):
            original(roots, path, raw, immutable=immutable, private=private)
            raise OSError("synthetic interruption")
        original(roots, path, raw, immutable=immutable, private=private)

    monkeypatch.setattr(writer, "_write", interrupted)
    with pytest.raises(OSError, match=r"^synthetic interruption$"):
        export(art_changed, roots, step=1, library=art_changed.library)
    assert not (roots.preview / POINTER).exists()
    with pytest.raises(UploadError, match=r"^Export validation failed$"):
        load_export(roots.preview)


def test_manifest_filename_must_match_its_content_hash(
    images: PublicImages, roots: Roots
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    source = string(loaded.entry["manifest_path"])
    wrong_path = "snapshots/manifests/" + "0" * 64 + ".json"
    (roots.preview / wrong_path).write_bytes((roots.preview / source).read_bytes())
    (roots.preview / POINTER).write_bytes(
        canonical(
            {
                "manifest_path": wrong_path,
                "manifest_sha256": loaded.entry["manifest_sha256"],
            }
        )
    )
    with pytest.raises(
        UploadError, match=r"^Manifest differs from the preview pointer$"
    ):
        load_export(roots.preview)


def _cli(
    monkeypatch: pytest.MonkeyPatch, transport: Loopback, *args: str
) -> tuple[int, str]:
    real_client = httpx.Client

    def factory(**_kwargs: object) -> httpx.Client:
        return real_client(transport=transport, trust_env=False, follow_redirects=False)

    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setattr(httpx, "Client", factory)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    result = CliRunner().invoke(
        app,
        ["r2", "upload-v2", *args, "--account-id", ACCOUNT, "--bucket", BUCKET],
    )
    return result.exit_code, result.output


def test_cli_execute_verifies_cdn_and_prints_no_secrets(
    images: PublicImages,
    roots: Roots,
    server: tuple[ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    state, transport = server
    export(images, roots, step=0)
    code, output = _cli(
        monkeypatch,
        transport,
        "--export-dir",
        str(roots.preview),
        "--cdn-base-url",
        CDN,
        "--execute",
    )
    assert code == 0, output
    result = json.loads(output)
    assert result["mode"] == "execute"
    assert result["cdn_verification"] == "verified"
    assert "synthetic-secret" not in output
    assert "Signature=" not in output
    assert any(h.get("host") == "cdn.invalid" for _m, _p, h in state.requests)
    assert INDEX in state.objects


def test_skip_cdn_still_reads_every_object_back_from_origin(
    images: PublicImages,
    roots: Roots,
    server: tuple[ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    state, transport = server
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    code, output = _cli(
        monkeypatch,
        transport,
        "--export-dir",
        str(roots.preview),
        "--execute",
        "--skip-cdn-verify",
    )
    assert code == 0, output
    assert json.loads(output)["cdn_verification"] == "skipped"
    assert not any(h.get("host") == "cdn.invalid" for _m, _p, h in state.requests)
    read = {p for m, p, _ in state.requests if m == "GET"}
    for key in [m.key for m in loaded.members] + [i.key for i in loaded.images]:
        assert f"/{BUCKET}/{key}" in read


def test_execute_without_cdn_url_or_skip_is_refused_before_reading(
    roots: Roots, monkeypatch: pytest.MonkeyPatch
) -> None:
    def forbidden() -> Credentials:
        raise AssertionError("credentials read")

    monkeypatch.setattr(Credentials, "environment", forbidden)
    result = CliRunner().invoke(
        app,
        ["r2", "upload-v2", "--export-dir", str(roots.preview), "--execute"],
        env={"FORCE_COLOR": None, "NO_COLOR": "1", "TERM": "dumb"},
    )
    assert result.exit_code != 0
    assert "--skip-cdn-verify" in result.output


def test_dry_run_reads_no_credentials_and_opens_no_client(
    images: PublicImages, roots: Roots, monkeypatch: pytest.MonkeyPatch
) -> None:
    export(images, roots, step=0)

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("dry-run touched credentials or the network")

    monkeypatch.setattr(Credentials, "environment", forbidden)
    monkeypatch.setattr(httpx, "Client", forbidden)
    monkeypatch.setattr("sve_carddb.r2_upload.sdk.Session", forbidden)
    before = {p: p.read_bytes() for p in roots.preview.rglob("*") if p.is_file()}
    result = CliRunner().invoke(
        app,
        [
            "r2",
            "upload-v2",
            "--export-dir",
            str(roots.preview),
            "--cdn-base-url",
            CDN,
        ],
    )
    assert result.exit_code == 0, result.output
    output = json.loads(result.output)
    loaded = load_export(roots.preview)
    assert output == {
        "data_version": loaded.entry["data_version"],
        "image_bytes": sum(i.bytes for i in loaded.images),
        "image_files": len(loaded.images),
        "json_bytes": sum(len(m.raw) for m in loaded.members),
        "json_files": len(loaded.members),
        "manifest_sha256": loaded.entry["manifest_sha256"],
        "mode": "dry_run",
        "remote": "not_checked",
    }
    assert {p: p.read_bytes() for p in before} == before


@pytest.mark.parametrize(
    ("current_fraction", "candidate_fraction", "accepted"),
    [
        ("", ".1", True),
        (".1", "", False),
        (".1000", ".1", True),
        (".10000000002", ".10000000001", False),
    ],
)
def test_index_compares_fractional_seconds_chronologically(
    images: PublicImages,
    roots: Roots,
    current_fraction: str,
    candidate_fraction: str,
    *,
    accepted: bool,
) -> None:
    export(images, roots, step=0)
    first = load_export(roots.preview).entry
    first |= {"published_at": "2026-10-04T01:02:03" + current_fraction + "Z"}
    index = next_index(None, first)
    candidate = first | {
        "data_version": "preview-20261004T010203Z-0002",
        "published_at": "2026-10-04T01:02:03" + candidate_fraction + "Z",
    }
    if accepted:
        result = next_index(index, candidate)
        assert result is not None
        assert result["current"] == candidate
        assert result["previous"] == first
    else:
        with pytest.raises(
            UploadError, match=r"^Export is older than the remote current$"
        ):
            next_index(index, candidate)


@pytest.mark.parametrize("retry_original", [False, True])
def test_pointer_failure_after_state_commit_preserves_safe_image_versions(
    images: PublicImages,
    art_changed: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
    *,
    retry_original: bool,
) -> None:
    import sve_carddb.snapshot.preview as writer  # ruff: ignore[import-outside-top-level] -- interrupt only the final pointer switch

    store, state, _ = remote
    export(images, roots, step=0)
    upload(store, load_export(roots.preview), None)
    original = writer._write

    def interrupted(
        roots: Roots, path: str, raw: bytes, *, immutable: bool, private: bool = False
    ) -> None:
        if path == POINTER:
            raise OSError("synthetic pointer interruption")
        original(roots, path, raw, immutable=immutable, private=private)

    with monkeypatch.context() as patch:
        patch.setattr(writer, "_write", interrupted)
        with pytest.raises(OSError, match=r"^synthetic pointer interruption$"):
            export(art_changed, roots, step=1)
    assert not (roots.preview / POINTER).exists()
    saved = object_value(parse((roots.private / "media-state.json").read_bytes()))
    assert saved["high_water"] == 2
    assert object_value(saved["committed"])["revision"] == 2
    retry = images if retry_original else art_changed
    export(retry, roots, step=2)
    loaded = load_export(roots.preview)
    for image in loaded.images:
        expected = (
            (3 if retry_original else 2) if image.key.startswith("images/art_") else 1
        )
        assert image.url.endswith("?v=" + str(expected))
    upload(store, loaded, None)
    assert current(state) == loaded.entry


def test_lost_index_put_response_reruns_without_another_revision(
    images: PublicImages, roots: Roots, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, transport = remote
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    transport.lose_key = INDEX
    with pytest.raises(UploadError, match=r"^R2 transport or protocol failed$"):
        upload(store, loaded, None)
    committed = state.objects[INDEX]
    before = len(state.operations)
    result = upload(store, loaded, None)
    assert puts(state, before) == []
    assert state.objects[INDEX] == committed
    assert result["index_revision"] == 1


def test_index_cas_race_preserves_the_concurrent_index(
    images: PublicImages,
    roots: Roots,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    store, state, _ = remote
    export(images, roots, step=0)
    upload(store, load_export(roots.preview), None)
    original_index = state.objects[INDEX]
    export(images, roots, step=1)
    original = store.put

    def raced(
        key: str, raw: bytes, headers: dict[str, str], *, expected: str | None
    ) -> bool:
        if key == INDEX:
            state.objects[INDEX] = Stored(
                original_index.raw, '"concurrent"', original_index.headers
            )
        return original(key, raw, headers, expected=expected)

    monkeypatch.setattr(store, "put", raced)
    with pytest.raises(UploadError, match=r"^Version index conditional write failed$"):
        upload(store, load_export(roots.preview), None)
    assert state.objects[INDEX].etag == '"concurrent"'
    assert state.objects[INDEX].raw == original_index.raw


@pytest.mark.parametrize("encoding", ["truncated", "different", "valid"])
def test_gzip_sibling_checks_decoded_bytes_without_recompression(
    images: PublicImages, roots: Roots, encoding: str
) -> None:
    export(images, roots, step=0)
    loaded = load_export(roots.preview)
    path = string(loaded.entry["manifest_path"]) + ".gz"
    raw = canonical(loaded.entry)
    if encoding == "valid":
        raw = (roots.preview / string(loaded.entry["manifest_path"])).read_bytes()
    encoded = gzip.compress(raw, compresslevel=1, mtime=123)
    if encoding == "truncated":
        encoded = encoded[:-4]
    (roots.preview / path).write_bytes(encoded)
    if encoding == "valid":
        member = next(m for m in load_export(roots.preview).members if m.key == path)
        assert member.raw == encoded
    else:
        with pytest.raises(UploadError, match=r"^Export gzip sibling"):
            load_export(roots.preview)

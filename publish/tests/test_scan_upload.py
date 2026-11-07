"""Scanner release validation and upload against the localhost S3 server."""

import hashlib
import json
from typing import TYPE_CHECKING, Any

import pytest
from sve_carddb.export.read_api import ExportError
from typer.testing import CliRunner

from sve_publish.adapter import Stored
from sve_publish.cli import app
from sve_publish.headers import INDEX_HEADERS, JSON_HEADERS, SCAN_BINARY_HEADERS
from sve_publish.scan import MANIFEST, load_release, report, upload_scan

from .r2_fixtures import ACCOUNT, BUCKET
from .r2_fixtures import remote as remote  # ruff: ignore[useless-import-alias] -- isolated loopback server
from .r2_fixtures import server as server  # ruff: ignore[useless-import-alias] -- dependency of remote
from .r2_sdk_fixtures import install_mock_sdk

if TYPE_CHECKING:
    from collections.abc import Callable
    from pathlib import Path

    from sve_publish.adapter import R2Store

    from .r2_fixtures import Loopback, ServerState

pytestmark = pytest.mark.usefixtures("close_sdk_clients")

DIM = 256
COUNT = 2

type Manifest = dict[str, Any]


def _put(root: Path, folder: str, suffix: str, raw: bytes) -> dict[str, object]:
    digest = hashlib.sha256(raw).hexdigest()
    file = f"{folder}/{digest}{suffix}"
    (root / folder).mkdir(exist_ok=True)
    (root / file).write_bytes(raw)
    return {"file": file, "bytes": len(raw), "sha256": digest}


def write_release(root: Path, *, rec_model: bytes = b"rec-1 model") -> Manifest:
    """A tiny release laid out like `sve_scan release`, with synthetic bytes."""
    root.mkdir(parents=True, exist_ok=True)
    manifest: Manifest = {
        "format": 1,
        "det": {
            "version": 1,
            **_put(root, "models", ".onnx", b"det-1 model"),
            "input": 256,
            "stride": 8,
            "grow": 1.0,
        },
        "rec": {
            "version": 1,
            **_put(root, "models", ".onnx", rec_model),
            "input": [312, 224],
            "dim": DIM,
            "scoring": {"hubWeight": 0.75, "accept": 0.5, "margin": 0.03},
        },
        "index": {},
    }
    for edition in ("jp", "en"):
        tag = (edition + rec_model.decode()).encode()

        def part(name: str, size: int, tag: bytes = tag) -> bytes:
            return hashlib.shake_256(name.encode() + tag).digest(size)

        manifest["index"][edition] = {
            "rec": 1,
            "data_version": f"2026.11.07-{edition}",
            "count": COUNT,
            "vectors": _put(root, "index", ".i8", part("vectors", COUNT * DIM)),
            "scale": _put(root, "index", ".scale", part("scale", COUNT * 4)),
            "hub": _put(root, "index", ".hub", part("hub", COUNT * 4)),
            "cards": _put(root, "index", ".json", json.dumps([tag.decode()]).encode()),
        }
    write_manifest(root, manifest)
    return manifest


def write_manifest(root: Path, manifest: Manifest) -> None:
    (root / "manifest.json").write_bytes(json.dumps(manifest, indent=2).encode())


def test_dry_run_reports_the_release_offline(tmp_path: Path) -> None:
    write_release(tmp_path / "release")
    result = CliRunner().invoke(app, ["scan", "upload", str(tmp_path / "release")])
    assert result.exit_code == 0, result.output
    summary = json.loads(result.stdout)
    assert summary == {
        "mode": "dry_run",
        "format": 1,
        "det": "det-1",
        "rec": "rec-1",
        "index": {
            "en": {"count": COUNT, "data_version": "2026.11.07-en"},
            "jp": {"count": COUNT, "data_version": "2026.11.07-jp"},
        },
        "files": 10,
        "bytes": summary["bytes"],
        "manifest_sha256": hashlib.sha256(
            (tmp_path / "release" / "manifest.json").read_bytes()
        ).hexdigest(),
        "remote": "not_checked",
    }


def test_relative_release_dir_is_refused() -> None:
    result = CliRunner().invoke(app, ["scan", "upload", "release"])
    assert result.exit_code == 2
    assert "RELEASE_DIR must be an absolute path" in result.output


def _set(path: str, value: object) -> Callable[[Manifest], None]:
    *parents, last = path.split(".")

    def change(manifest: Manifest) -> None:
        row = manifest
        for name in parents:
            row = row[name]
        row[last] = value

    return change


def _drop_index_file(manifest: Manifest, root: Path) -> None:
    (root / manifest["index"]["jp"]["hub"]["file"]).unlink()


def _rewrite_rec(manifest: Manifest, root: Path) -> None:
    (root / manifest["rec"]["file"]).write_bytes(b"rec-1 modex")


def _rename_det(manifest: Manifest, root: Path) -> None:
    other = "0" * 64
    (root / manifest["det"]["file"]).rename(root / f"models/{other}.onnx")
    manifest["det"]["file"] = f"models/{other}.onnx"
    manifest["det"]["sha256"] = other


@pytest.mark.parametrize(
    ("change", "message"),
    [
        (_set("format", 2), "format must be 1"),
        (_set("format", True), "format must be 1"),
        (_set("det.version", 0), "det.version must be a positive integer"),
        (_set("det.stride", 7), "multiple of det.stride"),
        (_set("det.grow", 0), "det.grow must be positive"),
        (_set("det.grow", "1"), "det.grow must be a finite number"),
        (_set("rec.input", [312]), r"rec.input must be \[height, width\]"),
        (_set("rec.dim", 128), "index.en sizes do not match"),
        (_set("rec.scoring.margin", None), "rec.scoring.margin must be a finite"),
        (_set("rec.scoring", []), "rec.scoring must be an object"),
        (_set("rec.sha256", "A" * 64), "rec.sha256 is not a SHA-256"),
        (_set("rec.bytes", 5), "rec size differs"),
        (_set("rec.file", "models/x.onnx"), r"rec.file must be models/<sha256>\.onnx"),
        (_set("index.jp.rec", 2), "index.jp was built for another recogniser"),
        (_set("index.jp.data_version", ""), "index.jp.data_version is missing"),
        (_set("index.jp.count", 3), "index.jp sizes do not match"),
        (_set("index.cn", {}), "unknown editions"),
        (_set("index", None), "index must be an object"),
    ],
)
def test_manifest_contract_violations_stop_before_upload(
    tmp_path: Path, change: Callable[[Manifest], None], message: str
) -> None:
    manifest = write_release(tmp_path)
    change(manifest)
    write_manifest(tmp_path, manifest)
    with pytest.raises(ExportError, match=message):
        load_release(tmp_path)


@pytest.mark.parametrize(
    ("damage", "message"),
    [
        (_drop_index_file, "index.jp.hub file is missing"),
        (_rewrite_rec, "rec SHA-256 differs"),
        (_rename_det, "det SHA-256 differs"),
    ],
)
def test_files_must_exist_and_match_their_hash_names(
    tmp_path: Path, damage: Callable[[Manifest, Path], None], message: str
) -> None:
    manifest = write_release(tmp_path)
    damage(manifest, tmp_path)
    write_manifest(tmp_path, manifest)
    with pytest.raises(ExportError, match=message):
        load_release(tmp_path)


@pytest.mark.parametrize(
    ("raw", "message"),
    [(b"\xff", "not JSON"), (b"[]", "root must be an object"), (None, "manifest")],
)
def test_unreadable_manifest_is_refused(
    tmp_path: Path, raw: bytes | None, message: str
) -> None:
    write_release(tmp_path)
    if raw is None:
        (tmp_path / "manifest.json").unlink()
    else:
        (tmp_path / "manifest.json").write_bytes(raw)
    with pytest.raises(ExportError, match=message):
        load_release(tmp_path)
    result = CliRunner().invoke(app, ["scan", "upload", str(tmp_path)])
    assert result.exit_code == 2


def puts(state: ServerState, start: int = 0) -> list[str]:
    return [key for op, key, _ in state.operations[start:] if op == "PUT"]


def test_upload_sends_hashed_files_first_and_manifest_last(
    tmp_path: Path, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, _ = remote
    write_release(tmp_path)
    release = load_release(tmp_path)
    result = upload_scan(store, release, jobs=3)
    assert result == {
        "mode": "execute",
        "written_files": 10,
        "unchanged_files": 0,
        "manifest": "updated",
        "cdn_verification": "not_checked",
    }
    written = puts(state)
    assert written[-1] == MANIFEST
    assert sorted(written[:-1]) == sorted(item.key for item in release.files)
    assert state.objects[MANIFEST] == Stored(
        release.manifest, state.objects[MANIFEST].etag, INDEX_HEADERS
    )
    for item in release.files:
        stored = state.objects[item.key]
        assert stored.raw == (tmp_path / item.file).read_bytes()
        expected = JSON_HEADERS if item.file.endswith(".json") else SCAN_BINARY_HEADERS
        assert stored.headers == expected

    start = len(state.operations)
    again = upload_scan(store, release)
    assert (again["written_files"], again["unchanged_files"]) == (0, 10)
    assert again["manifest"] == "unchanged"
    assert puts(state, start) == []


def test_new_model_uploads_only_new_files_and_keeps_old_ones(
    tmp_path: Path, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, _ = remote
    write_release(tmp_path / "v1")
    first = load_release(tmp_path / "v1")
    upload_scan(store, first)
    write_release(tmp_path / "v2", rec_model=b"rec-1 retrained")
    second = load_release(tmp_path / "v2")
    start = len(state.operations)
    result = upload_scan(store, second)
    new = {item.key for item in second.files} - {item.key for item in first.files}
    # The det model is shared; the rec model and its eight index parts are new.
    assert len(new) == 9
    assert result["written_files"] == len(new)
    assert set(puts(state, start)[:-1]) == new
    assert [c for op, k, c in state.operations[start:] if k == MANIFEST and op == "PUT"]
    assert state.objects[MANIFEST].raw == second.manifest
    assert all(item.key in state.objects for item in first.files)


def test_failed_file_leaves_the_old_manifest(
    tmp_path: Path, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, _ = remote
    write_release(tmp_path)
    release = load_release(tmp_path)
    state.fail_key = release.files[1].key
    with pytest.raises(ExportError):
        upload_scan(store, release, jobs=1)
    assert MANIFEST not in state.objects
    state.fail_key = None
    assert upload_scan(store, release)["manifest"] == "updated"


def test_file_changed_after_validation_is_not_published(
    tmp_path: Path, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, _ = remote
    manifest = write_release(tmp_path)
    release = load_release(tmp_path)
    (tmp_path / manifest["det"]["file"]).write_bytes(b"det-1 modex")
    with pytest.raises(ExportError, match="Scan file changed after validation"):
        upload_scan(store, release)
    assert MANIFEST not in state.objects


def test_manifest_changed_by_another_writer_stops(
    tmp_path: Path, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, state, _ = remote
    write_release(tmp_path)
    release = load_release(tmp_path)

    def race(_line: str) -> None:
        state.objects[MANIFEST] = Stored(b"{}", '"other"', INDEX_HEADERS)

    with pytest.raises(ExportError, match="changed during upload"):
        upload_scan(store, release, progress=race)
    assert state.objects[MANIFEST].raw == b"{}"


def test_rejected_conditional_manifest_write_stops(
    tmp_path: Path, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, _, transport = remote
    write_release(tmp_path)
    release = load_release(tmp_path)
    transport.reject_put_key = MANIFEST
    transport.reject_put_number = 1
    with pytest.raises(ExportError, match="conditional write failed"):
        upload_scan(store, release)


def test_upload_requires_a_worker(
    tmp_path: Path, remote: tuple[R2Store, ServerState, Loopback]
) -> None:
    store, _, _ = remote
    write_release(tmp_path)
    with pytest.raises(ExportError, match="at least 1"):
        upload_scan(store, load_release(tmp_path), jobs=0)


def test_cli_execute_uploads_through_the_sdk(
    tmp_path: Path,
    remote: tuple[R2Store, ServerState, Loopback],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    _, state, transport = remote
    write_release(tmp_path)
    install_mock_sdk(monkeypatch, transport)
    monkeypatch.setenv("SVE_R2_ACCESS_KEY_ID", "synthetic-access")
    monkeypatch.setenv("SVE_R2_SECRET_ACCESS_KEY", "synthetic-secret")
    arguments = ["scan", "upload", str(tmp_path), "--account-id", ACCOUNT]
    result = CliRunner().invoke(app, [*arguments, "--bucket", BUCKET, "--execute"])
    assert result.exit_code == 0, result.output
    summary = json.loads(result.stdout)
    assert summary["mode"] == "execute"
    assert summary["remote"] == "checked"
    assert summary["written_files"] == 10
    assert summary["det"] == "det-1"
    assert "Upload: 10/10 complete, 10 written" in result.stderr
    assert "synthetic-secret" not in result.output
    assert state.objects[MANIFEST].raw == (tmp_path / "manifest.json").read_bytes()


def test_report_counts_shared_files_once(tmp_path: Path) -> None:
    manifest = write_release(tmp_path)
    manifest["index"]["en"]["cards"] = manifest["index"]["jp"]["cards"]
    write_manifest(tmp_path, manifest)
    assert report(load_release(tmp_path))["files"] == 9


@pytest.mark.parametrize("constant", ["NaN", "Infinity", "-Infinity"])
def test_non_standard_constants_are_refused_anywhere(
    tmp_path: Path, constant: str
) -> None:
    # Unknown fields are not otherwise checked, but the site's JSON.parse rejects these.
    write_release(tmp_path)
    path = tmp_path / "manifest.json"
    raw = path.read_bytes()
    path.write_bytes(raw.replace(b"{", b'{"extra": ' + constant.encode() + b",", 1))
    with pytest.raises(ExportError, match="not JSON"):
        load_release(tmp_path)

"""Synthetic preview isolation, public identity and unknown-coverage counterexamples."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

import sve_carddb.snapshot.preview as writer_module
from sve_carddb.build_db import create_database
from sve_carddb.cli import app
from sve_carddb.snapshot.export import Ownership, export_snapshot
from sve_carddb.snapshot.export.compression import python_brotli
from sve_carddb.snapshot.media import prepare_media
from sve_carddb.snapshot.preview import (
    Roots,
    _write,
    require_unknown_coverage,
    write_preview,
)
from sve_carddb.snapshot.project import Projection, project
from sve_carddb.snapshot.project.records import art_records, initial
from sve_carddb.snapshot.project.source import Source
from sve_carddb.snapshot.publication import require_formal, require_preview
from sve_carddb.snapshot.reader import read_snapshot
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    object_value,
    parse,
    string,
)

from .snapshot_project_fixtures import SETTINGS, populate, schema
from .test_snapshot_export import BATCH
from .test_snapshot_export import exported as exported  # ruff: ignore[useless-import-alias] -- register shared module fixture
from .test_snapshot_project import projected

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.snapshot.export import Snapshot
    from sve_carddb.snapshot.media import MediaPlan


@pytest.fixture(scope="module")
def logical() -> tuple[Projection, Ownership]:
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
        projection = projected(db)
        projection = replace(
            projection,
            tables=projection.tables
            | {
                "printing_image": [
                    row | {"availability": "unfetched", "publication_state": "pending"}
                    for row in projection.tables["printing_image"]
                ],
                "image_variant": [],
            },
        )
        return prepare_media(
            projection, None, revision=1
        ).projection, Ownership.from_database(db, projection)


@pytest.fixture(params=["formal_version", "en_region"])
def invalid_preview(request: pytest.FixtureRequest, exported: Snapshot) -> Snapshot:
    change: dict[str, JsonValue] = (
        {"data_version": "20261002T010203Z-0001"}
        if request.param == "formal_version"
        else {"regions": ["jp", "en"]}
    )
    return replace(exported, manifest=exported.manifest | change)


def test_writer_rejects_non_preview_manifest(
    invalid_preview: Snapshot, tmp_path: Path
) -> None:
    with pytest.raises(ValueError, match="Preview requires"):
        write_preview(
            invalid_preview,
            Roots(tmp_path / "preview", tmp_path / "private"),
            {},
        )
    assert list(tmp_path.iterdir()) == []


@pytest.mark.parametrize("version", ["20261002T010203Z-0001", "20261002T010203Z-0002"])
def test_preview_cannot_accept_formal_version(exported: Snapshot, version: str) -> None:
    manifest = exported.manifest | {"data_version": version}
    with pytest.raises(ValueError, match="preview- data version"):
        require_preview(manifest)


def test_formal_publish_refuses_preview(exported: Snapshot, tmp_path: Path) -> None:
    with pytest.raises(ValueError, match="Formal publish refuses preview"):
        require_formal(exported.manifest)
    path = tmp_path / "manifest.json"
    path.write_bytes(canonical(exported.manifest))
    result = CliRunner().invoke(app, ["snapshot", "publish", str(path)])
    assert result.exit_code != 0
    assert isinstance(result.exception, ValueError)
    assert str(result.exception) == "Formal publish refuses preview artifacts"
    require_formal(exported.manifest | {"data_version": "20261002T010203Z-0001"})


def test_preview_scope_is_exactly_jp(exported: Snapshot) -> None:
    for regions in (["en"], ["jp", "en"]):
        broken = exported.manifest.copy()
        broken["regions"] = list[JsonValue](regions)
        with pytest.raises(ValueError, match="exactly the JP"):
            require_preview(broken)


def test_symlinked_destinations_cannot_escape_preview(tmp_path: Path) -> None:
    outside = tmp_path / "outside"
    outside.mkdir()
    preview = tmp_path / "preview"
    preview.mkdir()
    (preview / "snapshots").symlink_to(outside, target_is_directory=True)
    roots = Roots(preview, tmp_path / "private")
    with pytest.raises(ValueError, match="escapes"):
        _write(roots, "snapshots/preview/current.json", b"{}", immutable=False)
    with pytest.raises(ValueError, match="escapes"):
        roots.destination("../outside/index.json")
    assert list(outside.iterdir()) == []


def test_writer_reads_back_an_independent_join(
    exported: Snapshot, logical: tuple[Projection, Ownership], tmp_path: Path
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "private")
    report = write_preview(
        exported,
        roots,
        {"input_sha256": "sha256:" + "a" * 64},
        media_plan=preview_plan(exported),
    )
    pointer = object_value(
        parse((roots.preview / "snapshots/preview/current.json").read_bytes())
    )
    manifest_raw = (roots.preview / string(pointer["manifest_path"])).read_bytes()
    assert digest(manifest_raw) == pointer["manifest_sha256"]
    assert report["pointer"] == pointer
    assert (
        read_snapshot(
            parse(manifest_raw),
            {key: blob.raw for key, blob in exported.payloads.items()},
        )
        == logical[0].tables
    )
    assert not (roots.preview / "snapshots/versions").exists()
    assert not (roots.preview / "pages").exists()
    assert (
        write_preview(
            exported,
            roots,
            {"input_sha256": "sha256:" + "a" * 64},
            media_plan=preview_plan(exported),
        )
        == report
    )


def test_failed_artifact_write_keeps_old_preview_pointer(
    exported: Snapshot, tmp_path: Path
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "private")
    _write(roots, "snapshots/preview/current.json", b"old preview", immutable=False)
    first = object_value(array(exported.manifest["files"])[0])
    _write(roots, string(first["path"]), b"corrupt existing bytes", immutable=True)
    with pytest.raises(ValueError, match="Immutable"):
        write_preview(exported, roots, {}, media_plan=preview_plan(exported))
    assert (
        roots.preview / "snapshots/preview/current.json"
    ).read_bytes() == b"old preview"


@pytest.mark.parametrize("with_brotli", [False, True])
def test_preview_pointer_follows_every_immutable_member(
    exported: Snapshot,
    logical: tuple[Projection, Ownership],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    with_brotli: bool,
) -> None:
    codec = python_brotli() if with_brotli else None
    if codec is not None:
        exported = export_snapshot(logical[0], logical[1], BATCH, brotli=codec)
    roots = Roots(tmp_path / "preview", tmp_path / "private")
    manifest_hash = digest(canonical(exported.manifest))[7:]
    raw_paths = {
        string(object_value(item)["path"]) for item in array(exported.manifest["files"])
    } | {
        string(object_value(exported.manifest["text_all"])["path"]),
        "snapshots/manifests/" + manifest_hash + ".json",
    }
    expected = raw_paths | {path + ".gz" for path in raw_paths}
    if codec is not None:
        expected |= {path + ".br" for path in raw_paths}
    sealed: set[str] = set()
    pointer_written = False

    def observed_write(
        roots: Roots, path: str, raw: bytes, *, immutable: bool, private: bool = False
    ) -> None:
        nonlocal pointer_written
        if not immutable:
            assert (path, private) == ("snapshots/preview/current.json", False)
            assert expected <= sealed
            assert all((roots.preview / member).is_file() for member in expected)
            assert (roots.private / "reports" / (manifest_hash + ".json")).is_file()
            assert (roots.private / "media-state.json").is_file()
            pointer_written = True
        else:
            assert not pointer_written
        _write(roots, path, raw, immutable=immutable, private=private)
        if immutable and not private:
            sealed.add(path)

    monkeypatch.setattr(writer_module, "_write", observed_write)
    write_preview(exported, roots, {}, brotli=codec, media_plan=preview_plan(exported))
    assert pointer_written
    public = {
        p.relative_to(roots.preview).as_posix()
        for p in roots.preview.rglob("*")
        if p.is_file()
    }
    assert public == expected | {"snapshots/preview/current.json"}


@pytest.mark.parametrize("late_member", ["snapshots/manifests/", "reports/"])
def test_late_immutable_failure_preserves_old_pointer(
    exported: Snapshot,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    late_member: str,
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "private")
    _write(roots, "snapshots/preview/current.json", b"old preview", immutable=False)

    def failing_write(
        roots: Roots, path: str, raw: bytes, *, immutable: bool, private: bool = False
    ) -> None:
        if path.startswith(late_member):
            raise OSError("Synthetic late immutable failure")
        _write(roots, path, raw, immutable=immutable, private=private)

    monkeypatch.setattr(writer_module, "_write", failing_write)
    with pytest.raises(OSError, match="late immutable"):
        write_preview(exported, roots, {}, media_plan=preview_plan(exported))
    assert (
        roots.preview / "snapshots/preview/current.json"
    ).read_bytes() == b"old preview"


@pytest.mark.parametrize("field", ["source_windows", "restriction_coverage"])
def test_missing_coverage_cannot_be_invented_complete(
    logical: tuple[Projection, Ownership], field: str
) -> None:
    projection = replace(
        logical[0],
        metadata=logical[0].metadata
        | {"source_windows": [], "restriction_coverage": []},
        tables=logical[0].tables
        | {name: [] for name in ("qa", "errata", "cr_version", "restriction")},
    )
    require_unknown_coverage(projection)
    broken = replace(
        projection, metadata=projection.metadata | {field: [{"state": "complete"}]}
    )
    with pytest.raises(ValueError, match="Uncovered"):
        require_unknown_coverage(broken)
    for table in ("qa", "errata", "cr_version", "restriction"):
        with pytest.raises(ValueError, match="Unrequested"):
            require_unknown_coverage(
                replace(
                    projection, tables=projection.tables | {table: [{"id": "invented"}]}
                )
            )


def test_roots_have_no_defaults() -> None:
    result = CliRunner().invoke(
        app,
        ["snapshot", "export-offline", "--inputs", __file__],
        env={
            "SVE_PREVIEW_DIR": "",
            "NO_COLOR": "1",
            "TERM": "dumb",
        },
    )
    assert result.exit_code != 0
    assert "--preview-dir" in result.output


def test_project_refuses_publication_printing_absent_from_build() -> None:
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
        with pytest.raises(ValueError, match="Publication printing is absent"):
            project(
                db,
                regions=("jp",),
                as_of="2026-10-02",
                settings=SETTINGS,
                publication_printings=frozenset({"missing-synthetic-printing"}),
            )


class EmptySources:
    def pages(self) -> tuple[()]:
        return ()


@pytest.mark.parametrize("format_version", ["1.0.0", "1.1.0", "1.2.0"])
def test_cli_rejects_retired_formats_before_inputs(
    format_version: str, tmp_path: Path
) -> None:
    path = tmp_path / "inputs.json"
    path.write_text("{}")
    preview = tmp_path / "preview"
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export-offline",
            "--inputs",
            str(path),
            "--bundle-dir",
            str(tmp_path / "bundle"),
            "--format-version",
            format_version,
            "--preview-dir",
            str(preview),
            "--private-dir",
            str(tmp_path / "private"),
        ],
    )
    assert result.exit_code != 0
    assert isinstance(result.exception, ValueError)
    assert str(result.exception) == "Unsupported snapshot format profile"
    assert not preview.exists()


def test_review_joins_follow_filtered_primary_keys() -> None:
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
            db.insert(
                "decision",
                dict(db.rows("decision")[0].values)
                | {"id": "pending", "state": "proposed"},
            )
            db.insert(
                "digital_link",
                dict(db.rows("digital_link")[0].values)
                | {"id": "a-excluded", "decision_id": "pending"},
            )
        source = Source(db)
        view = initial(source)
        view["digital_link"] = [
            row for row in view["digital_link"] if row["id"] == "digital-link"
        ]
        art_records(source, view)
        assert view["digital_link"][0]["review_level"] == "confirmed"
        assert view["art"][0]["review_level"] == "confirmed"
        assert view["digital_art_link"][0]["review_level"] == "confirmed"


def test_renamed_preview_is_not_a_formal_release(
    exported: Snapshot, tmp_path: Path
) -> None:
    path = tmp_path / "renamed.json"
    path.write_bytes(
        canonical(exported.manifest | {"data_version": "20261002T010203Z-0001"})
    )
    result = CliRunner().invoke(app, ["snapshot", "publish", str(path)])
    assert result.exit_code != 0
    assert "Formal release gates are not implemented yet (#34)" in result.output
    assert sorted(p.name for p in tmp_path.iterdir()) == ["renamed.json"]


def preview_plan(snapshot: Snapshot) -> MediaPlan:
    view = read_snapshot(
        snapshot.manifest, {key: blob.raw for key, blob in snapshot.payloads.items()}
    )
    config = object_value(parse(snapshot.payloads["config"].raw))
    return prepare_media(Projection(view, config, snapshot.manifest), None, revision=1)

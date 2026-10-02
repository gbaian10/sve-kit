"""Synthetic public WebP bytes, state gates and preview activation failures."""

import shutil
from copy import deepcopy
from dataclasses import dataclass, replace
from io import BytesIO
from pathlib import Path
from types import SimpleNamespace
from typing import TYPE_CHECKING

import pytest
from PIL import Image
from typer.testing import CliRunner

import sve_carddb.snapshot.preview as writer_module
import sve_carddb.snapshot.preview.commands as cli_module
from sve_carddb.build_db import create_database
from sve_carddb.cli import app
from sve_carddb.image_variants import SIZES, build_variants
from sve_carddb.snapshot.export import Ownership, export_snapshot
from sve_carddb.snapshot.preview import Roots, _write, write_preview
from sve_carddb.snapshot.preview.build import Built
from sve_carddb.snapshot.preview.images import image_blobs
from sve_carddb.snapshot.values import (
    array,
    canonical,
    digest,
    integer,
    object_value,
    parse,
    string,
)

from .snapshot_project_fixtures import populate, schema
from .test_image_variants import png, source
from .test_snapshot_export import BATCH
from .test_snapshot_preview import cli_recipe
from .test_snapshot_project import projected

if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Snapshot
    from sve_carddb.snapshot.project import Projection
    from sve_carddb.snapshot.project.source import Record


@dataclass(frozen=True)
class PublicImages:
    projection: Projection
    ownership: Ownership
    library: Path

    def snapshot(self, tables: dict[str, list[Record]] | None = None) -> Snapshot:
        projection = self.projection
        if tables is not None:
            projection = replace(projection, tables=tables)
        return export_snapshot(projection, self.ownership, BATCH)

    def tables(self) -> dict[str, list[Record]]:
        return deepcopy(self.projection.tables)


@pytest.fixture(scope="module")
def images(tmp_path_factory: pytest.TempPathFactory) -> PublicImages:
    root = tmp_path_factory.mktemp("public-image-library")
    result = build_variants(
        source(png(160, 224)), blob_root=root / "library", cache_root=root / "cache"
    )
    with create_database(schema()) as db:
        with db.transaction():
            populate(db)
        projection = projected(db)
        tables = projection.tables | {
            "image_asset": [
                row | {"origin": "official"} for row in projection.tables["image_asset"]
            ],
            "image_variant": [
                {
                    "image_id": "image",
                    "size_key": v.size_key,
                    "format": v.format,
                    "path": v.path,
                    "width": v.width,
                    "height": v.height,
                    "bytes": v.bytes,
                }
                for v in sorted(result.variants, key=lambda item: item.size_key)
            ],
        }
        projection = replace(projection, tables=tables)
        return PublicImages(
            projection, Ownership.from_database(db, projection), root / "library"
        )


def test_writer_publishes_only_listed_webps_and_consistent_art_contract(
    images: PublicImages, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    library = tmp_path / "library"
    shutil.copytree(images.library, library)
    (library / "original.png").write_bytes(png(8, 8))
    (library / "digital.webp").write_bytes(b"synthetic digital image")
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    roots.formal.mkdir()
    (roots.formal / "sentinel").write_bytes(b"formal unchanged")
    members = dict(image_blobs(images.projection.tables, library))
    sealed: set[str] = set()

    def observed(roots: Roots, path: str, raw: bytes, *, immutable: bool) -> None:
        if path.startswith("snapshots/"):
            assert set(members) <= sealed
        _write(roots, path, raw, immutable=immutable)
        sealed.add(path)

    monkeypatch.setattr(writer_module, "_write", observed)
    report = write_preview(images.snapshot(), roots, {}, image_source=library)
    assert report["images"] == {
        "unique_files": len(members),
        "unique_bytes": sum(map(len, members.values())),
    }
    assert {
        p.relative_to(roots.preview).as_posix(): p.read_bytes()
        for p in roots.preview.rglob("*.webp")
    } == members
    assert not list(roots.preview.rglob("*.png"))
    assert not (roots.preview / "digital.webp").exists()
    assert (roots.formal / "sentinel").read_bytes() == b"formal unchanged"
    assert sorted(p.name for p in roots.formal.iterdir()) == ["sentinel"]
    config = images.projection.config["image_sizes"]
    assert config == [
        {
            "key": size.key,
            "purpose": size.purpose,
            "max_width": size.max_width,
            "max_height": size.max_height,
        }
        for size in sorted(SIZES, key=lambda item: item.key)
    ]
    assert {row["size_key"] for row in images.projection.tables["image_variant"]} == {
        size.key for size in SIZES
    }
    for row in images.projection.tables["image_variant"]:
        if row["size_key"] in {"art_s", "art_m"}:
            assert integer(row["width"]) * 3 == integer(row["height"]) * 4
    again = Roots(tmp_path / "again", roots.formal)
    assert write_preview(images.snapshot(), again, {}, image_source=library) == report
    assert {
        p.relative_to(roots.preview): p.read_bytes()
        for p in roots.preview.rglob("*")
        if p.is_file()
    } == {
        p.relative_to(again.preview): p.read_bytes()
        for p in again.preview.rglob("*")
        if p.is_file()
    }


@pytest.mark.parametrize("state", ["unfetched", "missing", "pending", "withdrawn"])
def test_unavailable_or_unapproved_images_have_metadata_only(
    images: PublicImages, tmp_path: Path, state: str
) -> None:
    tables = images.tables()
    asset = tables["image_asset"][0]
    field = "availability" if state in {"unfetched", "missing"} else "publication_state"
    asset[field] = state
    if state == "withdrawn":
        asset["withdrawal_reason"] = "Synthetic withdrawal"
    tables["image_variant"] = []
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    report = write_preview(images.snapshot(tables), roots, {})
    assert report["images"] == {"unique_files": 0, "unique_bytes": 0}
    assert not (roots.preview / "images").exists()
    assert asset in tables["image_asset"]
    tables["image_variant"] = images.tables()["image_variant"]
    with pytest.raises(ValueError, match="Only approved available"):
        list(image_blobs(tables, images.library))


@pytest.mark.parametrize("missing", ["all", "art_s", "card_l"])
def test_writer_refuses_incomplete_available_image_closure(
    images: PublicImages, tmp_path: Path, missing: str
) -> None:
    tables = images.tables()
    tables["image_variant"] = [
        row
        for row in tables["image_variant"]
        if missing != "all" and row["size_key"] != missing
    ]
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    with pytest.raises(ValueError, match="all five sizes"):
        write_preview(images.snapshot(tables), roots, {}, image_source=images.library)
    assert not roots.preview.exists()


@pytest.mark.parametrize("state", ["missing-proof", "other-image-proof", "confirmed"])
def test_third_party_approval_requires_each_image_confirmed(
    images: PublicImages, tmp_path: Path, state: str
) -> None:
    tables = images.tables()
    tables["image_asset"][0]["origin"] = "third_party"
    confirmed = frozenset({"image" if state == "confirmed" else "other"})
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    if state == "confirmed":
        assert write_preview(
            images.snapshot(tables),
            roots,
            {},
            image_source=images.library,
            confirmed_images=confirmed,
        )["images"]
    else:
        with pytest.raises(ValueError, match="individual confirmed"):
            write_preview(
                images.snapshot(tables),
                roots,
                {},
                image_source=images.library,
                confirmed_images=confirmed,
            )
        assert not roots.preview.exists()


@pytest.mark.parametrize(
    "case",
    [
        "source",
        "format",
        "prefix",
        "path",
        "digital",
        "shared-metadata",
    ],
)
def test_public_asset_validation_counterexamples(
    images: PublicImages, case: str
) -> None:
    tables = images.tables()
    variant = tables["image_variant"][0]
    changes: dict[str, Record] = {
        "format": {"format": "png"},
        "prefix": {
            "path": "images/sha256/00/" + string(variant["path"]).split("/")[-1]
        },
        "path": {"path": "../source.png"},
    }
    if case in changes:
        variant.update(changes[case])
    elif case == "digital":
        tables["printing_image"] = []
    elif case == "shared-metadata":
        tables["image_variant"].append(
            variant | {"width": integer(variant["width"]) + 1}
        )
    message = {
        "source": "Preview images require an explicit asset source",
        "digital": "Only approved available printing images can be published",
        "shared-metadata": "Shared image blob metadata disagrees",
    }.get(case, "Preview image requires a content-addressed WebP path")
    with pytest.raises(ValueError, match=f"^{message}$"):
        list(image_blobs(tables, None if case == "source" else images.library))


@pytest.mark.parametrize("field", ["width", "height", "bytes"])
def test_valid_webp_must_match_manifest_dimensions_and_bytes(
    images: PublicImages, field: str
) -> None:
    tables = images.tables()
    variant = tables["image_variant"][0]
    path = string(variant["path"])
    raw = (images.library / path).read_bytes()
    assert digest(raw)[7:] == Path(path).stem
    assert len(raw) == integer(variant["bytes"])
    # Shared hashes must keep consistent metadata so the decoded/file-size guard is reached.
    for row in tables["image_variant"]:
        if row["path"] == path:
            row[field] = integer(row[field]) + 1
    message = (
        "Preview image hash or bytes mismatch"
        if field == "bytes"
        else "Preview image decoded format or dimensions mismatch"
    )
    with pytest.raises(ValueError, match=f"^{message}$"):
        list(image_blobs(tables, images.library))


@pytest.mark.parametrize("kind", ["relative", "symlink"])
def test_image_input_root_requires_absolute_non_symlink_path(
    images: PublicImages,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    kind: str,
) -> None:
    if kind == "relative":
        monkeypatch.chdir(images.library.parent)
        root = Path(images.library.name)
    else:
        root = tmp_path / "linked-library"
        root.symlink_to(images.library, target_is_directory=True)
    assert root.is_dir()
    with pytest.raises(
        ValueError, match=r"^Preview image input must be an absolute non-symlink root$"
    ):
        list(image_blobs(images.projection.tables, root))


@pytest.mark.parametrize("case", ["corrupt", "missing", "symlink", "png"])
def test_actual_image_bytes_must_match_webp_metadata(
    images: PublicImages, tmp_path: Path, case: str
) -> None:
    tables = images.tables()
    library = tmp_path / "library"
    shutil.copytree(images.library, library)
    variant = tables["image_variant"][0]
    blob = library / string(variant["path"])
    raw = blob.read_bytes()
    if case == "corrupt":
        blob.write_bytes(raw + b"corrupt")
    elif case == "missing":
        blob.unlink()
    elif case == "symlink":
        blob.unlink()
        blob.symlink_to(images.library / string(variant["path"]))
    else:
        raw = png(8, 8)
        hashed = digest(raw)[7:]
        variant.update(
            {
                "path": f"images/sha256/{hashed[:2]}/{hashed}.webp",
                "bytes": len(raw),
                "width": 8,
                "height": 8,
            }
        )
        blob = library / string(variant["path"])
        blob.parent.mkdir(parents=True, exist_ok=True)
        blob.write_bytes(raw)
    with pytest.raises((ValueError, FileNotFoundError)):
        list(image_blobs(tables, library))


def test_equal_length_webp_tamper_requires_content_hash(
    images: PublicImages, tmp_path: Path
) -> None:
    library = tmp_path / "library"
    shutil.copytree(images.library, library)
    variant = images.projection.tables["image_variant"][0]
    blob = library / string(variant["path"])
    raw = blob.read_bytes()
    changed = raw[:-1] + bytes([raw[-1] ^ 1])
    assert len(changed) == integer(variant["bytes"])
    assert digest(changed) != digest(raw)
    # The size/decoder guards must accept the tamper to isolate the hash requirement.
    with Image.open(BytesIO(changed)) as decoded:
        decoded.load()
        assert decoded.format == "WEBP"
        assert decoded.size == (integer(variant["width"]), integer(variant["height"]))
    blob.write_bytes(changed)
    with pytest.raises(ValueError, match=r"^Preview image hash or bytes mismatch$"):
        list(image_blobs(images.projection.tables, library))


@pytest.mark.parametrize("failure", ["image", "manifest", "late-tamper"])
def test_interruption_keeps_old_complete_preview(
    images: PublicImages, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, failure: str
) -> None:
    roots = Roots(tmp_path / "preview", tmp_path / "formal")
    tables = images.tables()
    tables["image_variant"] = []
    tables["image_asset"][0].update(
        {"availability": "unfetched", "publication_state": "pending"}
    )
    write_preview(images.snapshot(tables), roots, {})
    old = {p: p.read_bytes() for p in roots.preview.rglob("*") if p.is_file()}
    calls = 0

    def failing(roots: Roots, path: str, raw: bytes, *, immutable: bool) -> None:
        nonlocal calls
        if path.startswith("images/"):
            calls += 1
            if failure == "image" and calls == 2:
                raise OSError("synthetic image interruption")
        if failure == "manifest" and path.startswith("snapshots/manifests/"):
            raise OSError("synthetic manifest interruption")
        _write(roots, path, raw, immutable=immutable)
        if failure == "late-tamper" and path.startswith("reports/"):
            (
                roots.preview
                / string(images.projection.tables["image_variant"][0]["path"])
            ).write_bytes(b"synthetic late corruption")

    monkeypatch.setattr(writer_module, "_write", failing)
    with pytest.raises((OSError, ValueError)):
        write_preview(images.snapshot(), roots, {}, image_source=images.library)
    assert all(p.read_bytes() == raw for p, raw in old.items())
    assert not (roots.preview / "snapshots/versions").exists()
    assert not roots.formal.exists()


@pytest.mark.parametrize("relation", ["same", "input-child", "output-child", "formal"])
def test_image_source_roots_must_be_disjoint(
    images: PublicImages, tmp_path: Path, relation: str
) -> None:
    preview, formal = tmp_path / "preview", tmp_path / "formal"
    source = preview
    if relation == "input-child":
        source /= "library"
    elif relation == "output-child":
        preview /= "output"
    elif relation == "formal":
        source = formal
    with pytest.raises(ValueError, match="disjoint"):
        write_preview(
            images.snapshot(), Roots(preview, formal), {}, image_source=source
        )
    assert list(tmp_path.iterdir()) == []


@pytest.mark.parametrize("option", ["--image-assets-dir", "--image-cache-dir"])
def test_cli_requires_both_readonly_image_roots(tmp_path: Path, option: str) -> None:
    path = tmp_path / "inputs.json"
    path.write_bytes(canonical(cli_recipe(tmp_path)))
    library = tmp_path / "library"
    library.mkdir()
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export",
            "--inputs",
            str(path),
            "--preview-dir",
            str(tmp_path / "preview"),
            "--cdn-dir",
            str(tmp_path / "formal"),
            option,
            str(library),
        ],
        env={"NO_COLOR": "1", "TERM": "dumb"},
    )
    assert result.exit_code != 0
    assert "provided together" in result.output
    assert not (tmp_path / "preview").exists()


def test_cli_rejects_relative_preview_before_reading_recipe(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.chdir(tmp_path)
    path = tmp_path / "inputs.json"
    path.write_bytes(b"not a recipe")
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export",
            "--inputs",
            str(path),
            "--preview-dir",
            "relative-preview",
            "--cdn-dir",
            str(tmp_path / "formal"),
        ],
    )
    assert result.exit_code != 0
    assert isinstance(result.exception, ValueError)
    assert str(result.exception) == "Preview root must be an absolute path"
    assert list(tmp_path.iterdir()) == [path]


def test_writer_rejects_relative_preview_before_writes(
    images: PublicImages, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.chdir(tmp_path)
    with pytest.raises(ValueError, match=r"^Preview root must be an absolute path$"):
        write_preview(
            images.snapshot(),
            Roots(Path("relative-preview"), tmp_path / "formal"),
            {},
            image_source=images.library,
        )
    assert list(tmp_path.iterdir()) == []


def test_cli_passes_confirmed_images_to_writer(
    images: PublicImages, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "inputs.json"
    path.write_bytes(canonical(cli_recipe(tmp_path)))
    cache = tmp_path / "cache"
    cache.mkdir()
    tables = images.tables()
    tables["image_asset"][0]["origin"] = "third_party"
    built = Built(
        replace(images.projection, tables=tables),
        images.ownership,
        b"synthetic input",
        {},
        frozenset({"image"}),
    )
    # The CLI must bind its frozen loader's result and root to the actual builder.
    sentinel = object()
    monkeypatch.setattr(cli_module, "FrozenSources", lambda *_args: sentinel)
    image_build = SimpleNamespace(images=(), elapsed_seconds=0)
    crop_inputs = object()
    monkeypatch.setattr(
        cli_module, "load_image_crops", lambda *_args, **_kwargs: crop_inputs
    )

    def assets(source: object, _roots: object, **kwargs: object) -> object:
        assert source is sentinel
        assert kwargs == {"workers": 4, "reuse_only": True, "crops": crop_inputs}
        return image_build

    def build(_recipe: object, **kwargs: object) -> Built:
        assert kwargs == {"images": image_build, "image_root": images.library}
        return built

    monkeypatch.setattr(cli_module, "build_jp_assets", assets)
    monkeypatch.setattr(cli_module, "build", build)
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export",
            "--inputs",
            str(path),
            "--preview-dir",
            str(tmp_path / "preview"),
            "--cdn-dir",
            str(tmp_path / "formal"),
            "--image-assets-dir",
            str(images.library),
            "--image-cache-dir",
            str(cache),
        ],
    )
    assert result.exit_code == 0, result.exception
    report = object_value(parse(result.output.encode()))
    assert report["image_execution"] == {
        "reuse_milliseconds": 0,
        "cache_hits": 0,
        "new_encoding_milliseconds": 0,
    }
    pointer = object_value(report["pointer"])
    manifest_path = string(pointer["manifest_path"])
    manifest = object_value(parse((tmp_path / "preview" / manifest_path).read_bytes()))
    assert any(
        string(object_value(row)["key"]).startswith("images/")
        for row in array(manifest["files"])
    )

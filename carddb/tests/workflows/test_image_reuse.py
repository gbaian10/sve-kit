"""Small frozen inputs prove scoped conversion, complete reuse and rejection paths."""

from dataclasses import dataclass
from pathlib import Path
from types import MappingProxyType
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.json import canonical, digest, object_value, parse, string
from sve_carddb.export.image_index import (
    MEDIA_TABLES,
    ImageBinding,
    ImageIndex,
    ImagePin,
)
from sve_carddb.export.image_scope import select_printings
from sve_carddb.export.image_upload import ImageUpload, upload_description
from sve_carddb.export.media import MediaPlan, prepare_media
from sve_carddb.export.media_urls import image_path
from sve_carddb.export.preview import Roots
from sve_carddb.export.project import Projection
from sve_carddb.export.read_api import POINTER, load_image_scope, load_image_upload
from sve_carddb.images.assets import ImageBuild, PreviewRoots, build_regional_assets
from sve_carddb.images.crops import CropRecord, ImageCrops
from sve_carddb.images.variants import DEFAULT_RECIPE
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.parse.pages import official_en
from sve_carddb.parse.pages.official_jp import image_url
from sve_carddb.workflows import image_index as producer
from sve_carddb.workflows.image_index import write_images
from sve_carddb.workflows.image_reuse import (
    ImageRequest,
    ImageReuse,
    PreviousImages,
    ReuseEvidence,
)

from ..support.english_image_fixtures import FRONT
from ..support.english_image_fixtures import english_images as english_images  # ruff: ignore[useless-import-alias] -- reuse the sealed EN baseline
from ..support.image_crop_fixtures import record
from .test_snapshot_offline_images import prepared as prepared  # ruff: ignore[useless-import-alias] -- reuse the regional text fixture
from .test_snapshot_offline_images import regional_images as regional_images  # ruff: ignore[useless-import-alias] -- reuse the sealed regional image fixture

if TYPE_CHECKING:
    from sve_carddb.domains.card_extras import CardPage
    from sve_carddb.workflows.offline import Inputs

    from ..support.text_observation_fixtures import Case


def projection(images: ImageBuild) -> tuple[Projection, tuple[ImageBinding, ...]]:
    tables: dict[str, list[dict[str, JsonValue]]] = {
        name: [] for name in (*MEDIA_TABLES, "printing", "face")
    }
    bindings = []
    for position, item in enumerate(
        sorted(images.images, key=lambda image: image.source.url), 1
    ):
        printing, face = f"p:{position}", f"f:{position}"
        image_id = (
            "img:binding:"
            + digest(canonical({"source_id": item.source.id, "src": item.source.url}))[
                7:
            ]
        )
        bindings.append(
            ImageBinding(printing_id=printing, face_id=face, int_id=position, ordinal=0)
        )
        tables["printing"].append(
            {"id": printing, "int_id": position, "faces": [{"face_id": face}]}
        )
        tables["face"].append({"id": face, "ordinal": 0})
        tables["image_asset"].append(
            {
                "id": image_id,
                "source_src_raw": item.source.url,
                "source_url": item.source.url,
                "width": item.result.source_width,
                "height": item.result.source_height,
                "format": "png",
            }
        )
        tables["printing_image"].append(
            {
                "printing_id": printing,
                "face_id": face,
                "image_id": image_id,
                "publication_state": "approved",
                "availability": "available",
            }
        )
        tables["image_variant"].extend(
            {
                "image_id": image_id,
                "size_key": v.size_key,
                "format": "webp",
                "path": v.path,
                "width": v.width,
                "height": v.height,
                "bytes": v.bytes,
            }
            for v in item.result.variants
        )
    tables["image_asset"].sort(key=lambda row: str(row["id"]))
    tables["image_variant"].sort(
        key=lambda row: (str(row["image_id"]), str(row["size_key"]))
    )
    return Projection(tables, {}, {}), tuple(bindings)


def write_json(root: Path, raw: bytes) -> Path:
    root.mkdir(parents=True, exist_ok=True)
    path = root / (digest(raw)[7:] + ".json")
    path.write_bytes(raw)
    return path


@dataclass(frozen=True)
class Baseline:
    sources: FrozenSources
    crops: ImageCrops
    paths: PreviousImages
    requests: tuple[ImageRequest, ...]
    index: ImageIndex
    upload: ImageUpload
    plan: MediaPlan

    def reuse(self, paths: PreviousImages | None = None) -> ImageReuse:
        return ImageReuse(
            paths or self.paths,
            self.sources.root,
            self.sources.store_id,
            frozenset({"p:3"}),
        )


@pytest.fixture
def baseline(  # ruff: ignore[too-many-locals] -- one tiny producer baseline owns its source and handoff
    tmp_path: Path, image_archive_template: tuple[Path, str, str]
) -> Baseline:
    root, store_id, batch = image_archive_template
    sources = FrozenSources(root, store_id, batch, verify_raw=False)
    first = min(sources.current_versions, key=lambda item: item.url)
    crop = CropRecord.model_validate(
        record(sources.descriptor(first.source_version_id))
    )
    crops = ImageCrops(MappingProxyType({crop.key: crop}))
    library = PreviewRoots(tmp_path / "baseline-assets", tmp_path / "baseline-cache")
    built = build_regional_assets(sources, library, region="jp", crops=crops)
    old = ImageBuild(
        tuple(sorted(built.images, key=lambda item: item.source.url)[:2]), 0
    )
    view, bindings = projection(old)
    plan = prepare_media(view, library.preview, revision=1)
    index = ImageIndex(
        recipe=DEFAULT_RECIPE.version,
        store_id=store_id,
        pins=(ImagePin(region="jp", batch_id=batch),),
        evidence=(),
        bindings=bindings,
        tables={name: plan.projection.tables[name] for name in MEDIA_TABLES},
    )
    upload = upload_description(plan, digest(index.content()))
    output = Roots(tmp_path / "baseline-public", tmp_path / "baseline-private")
    write_images(plan, output, library.preview, None)
    evidence = ReuseEvidence(
        image_index_sha256=digest(index.content()),
        recipe=DEFAULT_RECIPE.definition(),
        crops=(crop,),
    )
    paths = PreviousImages(
        write_json(tmp_path / "index", index.content()),
        write_json(tmp_path / "upload", upload.content()),
        write_json(tmp_path / "evidence", evidence.content()),
        output.preview,
    )
    requests = tuple(
        ImageRequest(f"p:{position}", f"f:{position}", "jp", item.url, item.url)
        for position, item in enumerate(
            sorted(sources.current_versions, key=lambda item: item.url), 1
        )
    )
    return Baseline(sources, crops, paths, requests, index, upload, plan)


def test_empty_cache_converts_new_scope_and_copies_all_other_rows(
    baseline: Baseline,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    reuse = baseline.reuse()
    selected = baseline.requests[-1].url
    outside_pngs = {
        baseline.sources.root / entry.blob.path
        for entry in baseline.sources.entries.values()
        if baseline.sources.descriptor(entry.source_version_id).url != selected
    }
    selected_pngs = {
        baseline.sources.root / entry.blob.path
        for entry in baseline.sources.entries.values()
        if baseline.sources.descriptor(entry.source_version_id).url == selected
    }
    outside_pngs -= selected_pngs
    read_bytes = Path.read_bytes

    def guarded(path: Path) -> bytes:
        if path in outside_pngs:
            pytest.fail("Scoped conversion read an out-of-scope PNG")
        return read_bytes(path)

    monkeypatch.setattr(Path, "read_bytes", guarded)
    borrowed = reuse.images(baseline.requests, {"jp": baseline.sources}, baseline.crops)
    library = PreviewRoots(tmp_path / "new-assets", tmp_path / "empty-cache")
    converted = build_regional_assets(
        baseline.sources,
        library,
        region="jp",
        crops=baseline.crops,
        urls=frozenset({selected}),
    )
    build = ImageBuild((*borrowed, *converted.images), converted.elapsed_seconds)
    assert build.execution()["new_encodings"] == 1
    assert build.execution()["reused_images"] == 2
    view, bindings = projection(build)
    reuse.validate_state(baseline.plan.state)
    plan = prepare_media(
        view,
        library.preview,
        revision=2,
        previous=baseline.plan.state,
        verify_blobs=False,
    )
    index = baseline.index.model_copy(
        update={
            "bindings": bindings,
            "tables": {name: plan.projection.tables[name] for name in MEDIA_TABLES},
        }
    )
    reuse.validate(index)
    output = Roots(tmp_path / "new-public", tmp_path / "new-private")
    reuse.copy_images(output)
    write_images(plan, output, library.preview, baseline.plan.state)
    for item in baseline.upload.objects:
        assert (output.preview / item.key).read_bytes() == (
            baseline.paths.images / item.key
        ).read_bytes()
    assert len(list(output.preview.rglob("*.webp"))) == 9
    assert {
        row["card_version"]
        for row in index.tables["printing_image"]
        if row["printing_id"] == "p:3"
    } == {2}


@pytest.mark.parametrize("change", ["changed", "missing"])
def test_outside_webp_must_exist_and_match_every_hash(
    baseline: Baseline, change: str
) -> None:
    target = baseline.paths.images / baseline.upload.objects[0].key
    if change == "missing":
        target.unlink()
    else:
        target.write_bytes(b"corrupt")
    with pytest.raises(ValueError, match=r"include p:[12]: WebP"):
        baseline.reuse().images(
            baseline.requests, {"jp": baseline.sources}, baseline.crops
        )


def test_outside_frozen_source_change_names_the_printing(baseline: Baseline) -> None:
    request = baseline.requests[0]
    current = min(baseline.sources.current_versions, key=lambda item: item.url)
    source, descriptor = baseline.sources.metadata(
        current.source_version_id, parser_version=DEFAULT_RECIPE.version
    )
    changed = source.model_copy(update={"sha256": "sha256:" + "f" * 64})
    with pytest.raises(ValueError, match="include p:1: source sha256 changed"):
        baseline.reuse().borrow(
            request, (changed, descriptor), (source, descriptor), baseline.crops
        )


@pytest.mark.parametrize("change", ["added", "removed", "changed"])
def test_outside_crop_changes_are_rejected(baseline: Baseline, change: str) -> None:
    records = dict(baseline.crops.records)
    if change == "removed":
        records.clear()
    elif change == "changed":
        key, crop = next(iter(records.items()))
        records[key] = crop.model_copy(update={"top": 20})
    else:
        second = sorted(baseline.sources.current_versions, key=lambda item: item.url)[1]
        crop = CropRecord.model_validate(
            record(baseline.sources.descriptor(second.source_version_id))
        )
        records[crop.key] = crop
    with pytest.raises(ValueError, match=r"include p:[12]: crop override changed"):
        baseline.reuse().images(
            baseline.requests,
            {"jp": baseline.sources},
            ImageCrops(MappingProxyType(records)),
        )


def test_different_recipe_requires_a_full_rebuild(
    baseline: Baseline, tmp_path: Path
) -> None:
    changed = baseline.index.model_copy(update={"recipe": "sha256:" + "f" * 64})
    paths = PreviousImages(
        write_json(tmp_path / "changed", changed.content()),
        baseline.paths.upload,
        baseline.paths.evidence,
        baseline.paths.images,
    )
    with pytest.raises(ValueError, match="recipe changed; perform a full"):
        baseline.reuse(paths)


def test_unregistered_printing_is_never_silently_skipped() -> None:
    with pytest.raises(ValueError, match="unregistered printing: p:unknown"):
        select_printings({"p:1": "BP22"}, ("BP22",), ("p:unknown",))


def test_wrong_committed_versions_are_rejected(baseline: Baseline) -> None:
    with pytest.raises(ValueError, match="missing committed media state"):
        baseline.reuse().validate_state({"revision": 2, "members": {}})


def test_metadata_changes_outside_scope_name_the_printing(baseline: Baseline) -> None:
    changed = ImageIndex.model_validate_json(baseline.index.content())
    changed.tables["image_asset"][0]["source_url"] = (
        "https://example.invalid/changed.png"
    )
    with pytest.raises(ValueError, match=r"expand image scope: p:[12]"):
        baseline.reuse().validate(changed)


def test_borrowed_outputs_skip_identical_bytes_and_remove_pointer_before_repair(
    baseline: Baseline,
    tmp_path: Path,
) -> None:
    reuse = baseline.reuse()
    reuse.images(baseline.requests, {"jp": baseline.sources}, baseline.crops)
    roots = Roots(tmp_path / "output", tmp_path / "private")
    reuse.copy_images(roots)
    pointer = roots.destination(POINTER)
    pointer.parent.mkdir(parents=True)
    pointer.write_bytes(b"previous preview")
    target = roots.preview / baseline.upload.objects[0].key
    original = target.read_bytes()
    before = target.stat().st_mtime_ns
    reuse.copy_images(roots)
    assert target.stat().st_mtime_ns == before
    assert pointer.is_file()
    target.write_bytes(b"damaged output")
    reuse.copy_images(roots)
    assert target.read_bytes() == original
    assert not pointer.exists()


def test_scoped_cli_matches_the_full_build_and_rejects_unsafe_inputs(  # ruff: ignore[too-many-locals] -- one CLI baseline is reused by the success and rejection checks
    prepared: tuple[Case, Inputs, tuple[CardPage, ...]],
    regional_images: tuple[Inputs, ImageBuild, PreviewRoots],
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    case, _, _ = prepared
    recipe, _, roots = regional_images
    monkeypatch.setattr(
        producer, "registry_plan", lambda *_args, **_kwargs: case.identity
    )
    monkeypatch.setattr(
        producer, "FrozenTexts", lambda *_args, **_kwargs: case.provider
    )
    inputs = tmp_path / "inputs.json"
    inputs.write_text(recipe.model_dump_json(), encoding="utf-8")
    full_public, full_private = tmp_path / "full-public", tmp_path / "full-private"

    def arguments(
        public: Path, private: Path, assets_dir: Path, cache: Path
    ) -> list[str]:
        return [
            "images",
            "build-index",
            "--inputs",
            str(inputs),
            "--image-assets-dir",
            str(assets_dir),
            "--image-cache-dir",
            str(cache),
            "--image-output-dir",
            str(public),
            "--private-dir",
            str(private),
        ]

    runner = CliRunner()
    full = runner.invoke(
        app,
        [
            *arguments(full_public, full_private, roots.preview, roots.cache),
            "--initial",
        ],
    )
    assert full.exit_code == 0, repr(full.exception)
    summary = object_value(parse(full.stdout.encode()))
    index_path = Path(string(summary["image_index"]))
    index = ImageIndex.read(index_path)
    english = image_url(FRONT, official_en.card_url("synthetic"))
    urls = {
        string(row["id"]): string(row["source_url"])
        for row in index.tables["image_asset"]
    }
    images = {
        string(row["printing_id"]): urls[string(row["image_id"])]
        for row in index.tables["printing_image"]
    }
    selected = next(key for key, url in images.items() if url != english)
    previous = [
        "--previous-index",
        str(index_path),
        "--previous-image-upload",
        string(summary["image_upload"]),
        "--previous-image-dir",
        str(full_public),
        "--previous-reuse-evidence",
        string(summary["image_reuse"]),
    ]
    scoped_public = tmp_path / "scoped-public"
    scoped_private = tmp_path / "scoped-private"
    scoped_private.mkdir()
    (scoped_private / "media-state.json").write_bytes(
        (full_private / "media-state.json").read_bytes()
    )
    scoped = arguments(
        scoped_public, scoped_private, tmp_path / "new-assets", tmp_path / "new-cache"
    )
    for rejected, message in (
        ([*previous, "--scope", "UNKNOWN"], "Unknown image scope"),
        (["--printing-id", selected], "require all four --previous-"),
    ):
        result = runner.invoke(app, [*scoped, *rejected])
        assert result.exit_code != 0
        assert message in str(result.exception)
    assert not scoped_public.exists()

    result = runner.invoke(app, [*scoped, *previous, "--printing-id", selected])
    assert result.exit_code == 0, repr(result.exception)
    result_summary = object_value(parse(result.stdout.encode()))
    execution = object_value(result_summary["image_execution"])
    assert execution["new_encodings"] == 1
    assert execution["reused_images"] == 1
    assert Path(string(result_summary["image_index"])).read_bytes() == (
        index_path.read_bytes()
    )
    full_files = {path.relative_to(full_public) for path in full_public.rglob("*.webp")}
    assert full_files == {
        path.relative_to(scoped_public) for path in scoped_public.rglob("*.webp")
    }
    assert all(
        (scoped_public / path).read_bytes() == (full_public / path).read_bytes()
        for path in full_files
    )
    assert load_image_scope(
        Path(string(result_summary["image_scope"])),
        load_image_upload(Path(string(result_summary["image_upload"]))),
    ).select((), (selected,))

    outside = next(key for key, url in images.items() if url == english)
    binding = next(item for item in index.bindings if item.printing_id == outside)
    key = image_path(binding.int_id, binding.ordinal, "card_s")
    (full_public / key).write_bytes(b"corrupt")
    tampered = runner.invoke(
        app,
        [
            *arguments(
                tmp_path / "tamper-public",
                scoped_private,
                tmp_path / "other-assets",
                tmp_path / "other-cache",
            ),
            *previous,
            "--printing-id",
            selected,
        ],
    )
    assert tampered.exit_code != 0
    assert f"Expand image scope to include {outside}" in str(tampered.exception)

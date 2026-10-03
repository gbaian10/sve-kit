"""Explicit preview mode and fail-closed formal publication command boundary."""

import subprocess  # ruff: ignore[suspicious-subprocess-import] -- explicitly injected optional compressor boundary
from pathlib import Path
from typing import Annotated

import typer

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    ImageBuild,
    PreviewRoots,
    build_jp_assets,
    build_regional_assets,
)
from sve_carddb.image_crops import load_image_crops
from sve_carddb.snapshot.export import Brotli, export_snapshot
from sve_carddb.snapshot.offline import Inputs as OfflineInputs
from sve_carddb.snapshot.offline import build as build_offline
from sve_carddb.snapshot.preview import Roots, _write, write_preview
from sve_carddb.snapshot.preview.build import Inputs, build
from sve_carddb.snapshot.profiles import LEGACY, profile
from sve_carddb.snapshot.publication import require_formal, require_preview
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

app = typer.Typer(no_args_is_help=True, help="Export isolated offline previews.")


def command_brotli(command: Path) -> Brotli:
    """Use an explicitly supplied compressor, rather than an undeclared dependency."""
    executable = str(command.resolve(strict=True))
    version = subprocess.check_output([executable, "--version"], text=True).strip()  # ruff: ignore[subprocess-without-shell-equals-true] -- explicit caller-selected executable, no shell
    pin = version + " / " + digest(Path(executable).read_bytes())

    def compress(raw: bytes) -> bytes:
        return subprocess.check_output([executable, "-q", "11", "-c"], input=raw)  # ruff: ignore[subprocess-without-shell-equals-true] -- explicit caller-selected executable, no shell

    return Brotli(pin, compress)


def verify_inputs(roots: Roots, inputs: Inputs | OfflineInputs) -> None:
    """No output may modify the input repo or archived immutable evidence."""
    roots.verify()
    for protected in (inputs.repo, inputs.archive):
        root, source = roots.preview.resolve(), protected.resolve()
        if root.is_relative_to(source) or source.is_relative_to(root):
            raise ValueError(
                "Preview output must be disjoint from immutable input roots"
            )


@app.command("export")
def export_command(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- CLI selects fixed profile and explicit isolated input/output roots
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    preview_dir: Annotated[Path, typer.Option(envvar="SVE_PREVIEW_DIR")],
    cdn_dir: Annotated[Path, typer.Option(envvar="SVE_CDN_DIR")],
    brotli_command: Annotated[
        Path | None, typer.Option(exists=True, dir_okay=False)
    ] = None,
    image_assets_dir: Annotated[
        Path | None, typer.Option(exists=True, file_okay=False)
    ] = None,
    image_cache_dir: Annotated[
        Path | None, typer.Option(exists=True, file_okay=False)
    ] = None,
    format_version: Annotated[str, typer.Option()] = LEGACY,
) -> None:
    """Require explicit roots and pins; write no formal index, active state or cache."""
    profile(format_version)
    roots = Roots(preview_dir, cdn_dir)
    roots.verify()
    recipe = Inputs.model_validate_json(inputs.read_bytes())
    verify_inputs(roots, recipe)
    codec = None if brotli_command is None else command_brotli(brotli_command)
    if (image_assets_dir is None) != (image_cache_dir is None):
        raise typer.BadParameter(
            "Image asset and cache roots must be provided together"
        )
    image_execution: dict[str, int] | None = None
    if image_assets_dir is None or image_cache_dir is None:
        built = build(recipe)
    else:
        image_roots = PreviewRoots(image_assets_dir, cdn_dir, image_cache_dir)
        image_roots.validate((recipe.archive, recipe.repo, preview_dir))
        crops = load_image_crops(
            recipe.repo / "authored", authored_revision=recipe.revision
        )
        images = build_jp_assets(
            FrozenSources(recipe.archive, recipe.store_id, recipe.image_batch),
            image_roots,
            workers=4,
            reuse_only=True,
            crops=crops,
        )
        built = build(recipe, images=images, image_root=image_assets_dir)
        image_execution = {
            "reuse_milliseconds": round(images.elapsed_seconds * 1000),
            "cache_hits": sum(item.result.cache_hit for item in images.images),
            "new_encoding_milliseconds": 0,
        }
    snapshot = export_snapshot(
        built.projection,
        built.ownership,
        recipe.batch(),
        brotli=codec,
        format_version=format_version,
    )
    require_preview(snapshot.manifest)
    _write(
        roots,
        "private/inputs/" + digest(built.input_content)[7:] + ".json",
        built.input_content,
        immutable=True,
    )
    report = write_preview(
        snapshot,
        roots,
        built.report,
        brotli=codec,
        image_source=image_assets_dir,
        confirmed_images=built.confirmed_images,
    )
    if image_execution is not None:
        report["image_execution"] = dict(image_execution)
    typer.echo(canonical(report).decode())


@app.command("export-offline")
def export_offline_command(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- CLI binds explicit recipe, isolated outputs, paired image roots and optional compressor
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    preview_dir: Annotated[Path, typer.Option(envvar="SVE_PREVIEW_DIR")],
    cdn_dir: Annotated[Path, typer.Option(envvar="SVE_CDN_DIR")],
    bundle_dir: Annotated[Path, typer.Option()],
    brotli_command: Annotated[
        Path | None, typer.Option(exists=True, dir_okay=False)
    ] = None,
    image_assets_dir: Annotated[
        Path | None, typer.Option(exists=True, file_okay=False)
    ] = None,
    image_cache_dir: Annotated[
        Path | None, typer.Option(exists=True, file_okay=False)
    ] = None,
    format_version: Annotated[str, typer.Option()] = LEGACY,
) -> None:
    """Export both launch regions to an isolated preview plus a verified private DB bundle."""
    profile(format_version)
    roots = Roots(preview_dir, cdn_dir)
    recipe = OfflineInputs.model_validate_json(inputs.read_bytes())
    verify_inputs(roots, recipe)
    for protected in (recipe.repo, recipe.archive, cdn_dir, inputs):
        output, source = bundle_dir.resolve(), protected.resolve()
        if output.is_relative_to(source) or source.is_relative_to(output):
            raise ValueError(
                "Offline bundle must be disjoint from protected inputs and formal output"
            )
    if inputs.resolve().is_relative_to(preview_dir.resolve()):
        raise ValueError("Offline preview must be disjoint from recipe")
    codec = None if brotli_command is None else command_brotli(brotli_command)
    if (image_assets_dir is None) != (image_cache_dir is None):
        raise typer.BadParameter(
            "Image asset and cache roots must be provided together"
        )
    images = None
    if image_assets_dir is not None and image_cache_dir is not None:
        image_roots = PreviewRoots(image_assets_dir, cdn_dir, image_cache_dir)
        image_roots.validate(
            (recipe.archive, recipe.repo, preview_dir, bundle_dir, inputs)
        )
        crops = load_image_crops(
            recipe.repo / "authored", authored_revision=recipe.revision
        )
        regional = tuple(
            build_regional_assets(
                FrozenSources(recipe.archive, recipe.store_id, pin.image_batch),
                image_roots,
                region=pin.region,
                crops=crops,
                workers=2,
                reuse_only=True,
            )
            for pin in recipe.sources
        )
        images = ImageBuild(
            tuple(item for part in regional for item in part.images),
            sum(part.elapsed_seconds for part in regional),
        )
    built = build_offline(
        recipe, bundle_dir=bundle_dir, images=images, image_root=image_assets_dir
    )
    snapshot = export_snapshot(
        built.projection,
        built.ownership,
        recipe.batch(),
        brotli=codec,
        format_version=format_version,
    )
    report = write_preview(
        snapshot,
        roots,
        built.report,
        brotli=codec,
        regions=("en", "jp"),
        image_source=image_assets_dir,
        confirmed_images=built.confirmed_images,
    )
    typer.echo(canonical(report).decode())


@app.command("publish")
def publish_command(
    manifest: Annotated[Path, typer.Argument(exists=True, dir_okay=False)],
) -> None:
    """Reject preview first; complete formal release gates are tracked by #34."""
    require_formal(object_value(parse(manifest.read_bytes())))
    raise typer.BadParameter("Formal release gates are not implemented yet (#34)")

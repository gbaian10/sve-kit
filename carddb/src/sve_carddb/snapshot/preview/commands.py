"""Explicit preview mode and fail-closed formal publication command boundary."""

from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime annotations
from typing import TYPE_CHECKING, Annotated

import typer

from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    MAX_WORKERS,
    ImageBuild,
    PreviewRoots,
    build_regional_assets,
)
from sve_carddb.image_crops import load_image_crops
from sve_carddb.snapshot.export import export_snapshot
from sve_carddb.snapshot.export.compression import python_brotli
from sve_carddb.snapshot.media import prepare_media
from sve_carddb.snapshot.offline import Built, Inputs
from sve_carddb.snapshot.offline import build as build_offline
from sve_carddb.snapshot.preview import Roots, _write, write_preview
from sve_carddb.snapshot.preview.media_state import reserve
from sve_carddb.snapshot.profiles import MEDIA, profile
from sve_carddb.snapshot.publication import require_formal, require_preview
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Brotli

app = typer.Typer(no_args_is_help=True, help="Export isolated offline previews.")


def verify_inputs(roots: Roots, inputs: Inputs) -> None:
    """No output may modify the input repo or archived immutable evidence."""
    roots.verify()
    for protected in (inputs.repo, inputs.archive):
        source = protected.resolve()
        for output in (roots.preview, roots.private):
            root = output.resolve()
            if root.is_relative_to(source) or source.is_relative_to(root):
                raise ValueError(
                    "Preview output must be disjoint from immutable input roots"
                )


@app.command("export-offline")
def export_offline_command(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- CLI binds explicit recipe, isolated outputs, paired image roots, worker count and optional compressor
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    preview_dir: Annotated[Path, typer.Option(envvar="SVE_PREVIEW_DIR")],
    private_dir: Annotated[Path, typer.Option()],
    bundle_dir: Annotated[Path, typer.Option()],
    brotli: Annotated[bool, typer.Option("--brotli/--no-brotli")] = False,
    image_assets_dir: Annotated[
        Path | None, typer.Option(exists=True, file_okay=False)
    ] = None,
    image_cache_dir: Annotated[
        Path | None, typer.Option(exists=True, file_okay=False)
    ] = None,
    workers: Annotated[int, typer.Option(min=1, max=MAX_WORKERS)] = 2,
    format_version: Annotated[str, typer.Option()] = MEDIA,
) -> None:
    """Export both launch regions to an isolated preview plus a verified private DB bundle."""
    profile(format_version)
    roots = Roots(preview_dir, private_dir)
    recipe = Inputs.model_validate_json(inputs.read_bytes())
    verify_inputs(roots, recipe)
    for protected in (recipe.repo, recipe.archive, inputs, preview_dir):
        output, source = bundle_dir.resolve(), protected.resolve()
        if output.is_relative_to(source) or source.is_relative_to(output):
            raise ValueError("Offline bundle must be disjoint from protected inputs")
    if any(
        inputs.resolve().is_relative_to(root.resolve())
        for root in (preview_dir, private_dir)
    ):
        raise ValueError("Offline preview must be disjoint from recipe")
    codec = python_brotli() if brotli else None
    if (image_assets_dir is None) != (image_cache_dir is None):
        raise typer.BadParameter(
            "Image asset and cache roots must be provided together"
        )
    images = None
    if image_assets_dir is not None and image_cache_dir is not None:
        image_roots = PreviewRoots(image_assets_dir, image_cache_dir)
        image_roots.validate(
            (recipe.archive, recipe.repo, preview_dir, private_dir, bundle_dir, inputs)
        )
        crops = load_image_crops(recipe.repo / "authored")
        regional = tuple(
            build_regional_assets(
                FrozenSources(recipe.archive, recipe.store_id, pin.image_batch),
                image_roots,
                region=pin.region,
                crops=crops,
                workers=workers,
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
    _finish(
        built,
        roots,
        recipe.batch(),
        codec,
        image_assets_dir,
        None if images is None else images.execution() | {"workers": workers},
        format_version,
    )


if TYPE_CHECKING:
    from sve_carddb.snapshot.export import Batch
    from sve_carddb.snapshot.media import MediaPlan


def _finish(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- common sealing boundary receives the explicit codec, verified build, roots and profile
    built: Built,
    roots: Roots,
    batch: Batch,
    codec: Brotli | None,
    image_source: Path | None,
    image_execution: dict[str, int] | None,
    version: str,
) -> None:
    def seal(plan: MediaPlan) -> None:
        projection = plan.projection
        snapshot = export_snapshot(
            projection, built.ownership, batch, brotli=codec, format_version=version
        )
        require_preview(snapshot.manifest, regions=batch.regions)
        _write(
            roots,
            "inputs/" + digest(built.input_content)[7:] + ".json",
            built.input_content,
            immutable=True,
            private=True,
        )
        report = write_preview(
            snapshot,
            roots,
            dict(built.report),
            brotli=codec,
            image_source=image_source,
            regions=batch.regions,
            media_plan=plan,
        )
        if image_execution is not None:
            report["image_execution"] = dict(image_execution)
        typer.echo(canonical(report).decode())

    revision, previous = reserve(roots)
    seal(
        prepare_media(
            built.projection,
            image_source,
            revision=revision,
            previous=previous,
        )
    )


@app.command("publish")
def publish_command(
    manifest: Annotated[Path, typer.Argument(exists=True, dir_okay=False)],
) -> None:
    """Reject preview first; complete formal release gates are tracked by #34."""
    require_formal(object_value(parse(manifest.read_bytes())))
    raise typer.BadParameter("Formal release gates are not implemented yet (#34)")

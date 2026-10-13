"""Explicit preview mode and fail-closed formal publication command boundary."""

from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime annotations
from typing import TYPE_CHECKING, Annotated

import typer

from sve_carddb.cli_paths import required_root
from sve_carddb.contracts.profiles import MEDIA, profile
from sve_carddb.core.json import canonical, digest
from sve_carddb.core.stage_log import mark
from sve_carddb.export.image_index import ImageIndex
from sve_carddb.export.preview import Roots, write_artifact, write_preview
from sve_carddb.export.publication import require_preview
from sve_carddb.export.transport import export_snapshot
from sve_carddb.export.transport.compression import python_brotli
from sve_carddb.workflows.offline import Built, Inputs
from sve_carddb.workflows.offline import build as build_offline

if TYPE_CHECKING:
    from sve_carddb.export.transport import Brotli

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
def export_offline_command(  # ruff: ignore[too-many-arguments, too-many-positional-arguments] -- CLI binds explicit recipe, isolated outputs, pinned image metadata and optional compressor
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    bundle_dir: Annotated[Path, typer.Option()],
    preview_dir: Annotated[Path | None, typer.Option(envvar="SVE_EXPORT_DIR")] = None,
    private_dir: Annotated[
        Path | None, typer.Option(envvar="SVE_CARDDB_PRIVATE_DIR")
    ] = None,
    brotli: Annotated[bool, typer.Option("--brotli/--no-brotli")] = False,
    image_index: Annotated[
        Path | None, typer.Option(exists=True, dir_okay=False)
    ] = None,
    format_version: Annotated[str, typer.Option()] = MEDIA,
) -> None:
    """Export both launch regions to an isolated preview plus a verified private DB bundle."""
    preview_dir = required_root(preview_dir, "--preview-dir", "SVE_EXPORT_DIR")
    private_dir = required_root(private_dir, "--private-dir", "SVE_CARDDB_PRIVATE_DIR")
    profile(format_version)
    roots = Roots(preview_dir, private_dir)
    recipe = Inputs.model_validate_json(inputs.read_bytes())
    verify_inputs(roots, recipe)
    mark("load recipe + verify inputs")
    for protected in (
        recipe.repo,
        recipe.archive,
        inputs,
        preview_dir,
        *(() if image_index is None else (image_index,)),
    ):
        output, source = bundle_dir.resolve(), protected.resolve()
        if output.is_relative_to(source) or source.is_relative_to(output):
            raise ValueError("Offline bundle must be disjoint from protected inputs")
    if any(
        inputs.resolve().is_relative_to(root.resolve())
        for root in (preview_dir, private_dir)
    ):
        raise ValueError("Offline preview must be disjoint from recipe")
    if image_index is not None and image_index.resolve().is_relative_to(
        preview_dir.resolve()
    ):
        raise ValueError("Private image index must be outside the public output root")
    codec = python_brotli() if brotli else None
    index = None if image_index is None else ImageIndex.read(image_index)
    mark("load image index")
    built = build_offline(recipe, bundle_dir=bundle_dir, image_index=index)
    _finish(built, roots, recipe.batch(), codec, format_version)


if TYPE_CHECKING:
    from sve_carddb.export.transport import Batch


def _finish(
    built: Built, roots: Roots, batch: Batch, codec: Brotli | None, version: str
) -> None:
    snapshot = export_snapshot(
        built.projection, built.ownership, batch, brotli=codec, format_version=version
    )
    mark("export snapshot (shard + compress)")
    require_preview(snapshot.manifest, regions=batch.regions)
    write_artifact(
        roots,
        "inputs/" + digest(built.input_content)[7:] + ".json",
        built.input_content,
        immutable=True,
        private=True,
    )
    mark("write inputs record")
    report = write_preview(
        snapshot, roots, dict(built.report), brotli=codec, regions=batch.regions
    )
    mark("write preview + verify")
    typer.echo(canonical(report).decode())

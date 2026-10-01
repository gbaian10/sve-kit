"""Explicit preview mode and fail-closed formal publication command boundary."""

import subprocess  # ruff: ignore[suspicious-subprocess-import] -- explicitly injected optional compressor boundary
from pathlib import Path
from typing import Annotated

import typer

from sve_carddb.snapshot.export import Brotli, export_snapshot
from sve_carddb.snapshot.preview import Roots, _write, write_preview
from sve_carddb.snapshot.preview.build import Inputs, build
from sve_carddb.snapshot.publication import require_formal, require_preview
from sve_carddb.snapshot.values import canonical, digest, object_value, parse

app = typer.Typer(no_args_is_help=True, help="Export isolated offline JP previews.")


def command_brotli(command: Path) -> Brotli:
    """Use an explicitly supplied compressor, rather than an undeclared dependency."""
    executable = str(command.resolve(strict=True))
    version = subprocess.check_output([executable, "--version"], text=True).strip()  # ruff: ignore[subprocess-without-shell-equals-true] -- explicit caller-selected executable, no shell
    pin = version + " / " + digest(Path(executable).read_bytes())

    def compress(raw: bytes) -> bytes:
        return subprocess.check_output([executable, "-q", "11", "-c"], input=raw)  # ruff: ignore[subprocess-without-shell-equals-true] -- explicit caller-selected executable, no shell

    return Brotli(pin, compress)


def verify_inputs(roots: Roots, inputs: Inputs) -> None:
    """No output may modify the input repo or archived immutable evidence."""
    roots.verify()
    for protected in (inputs.repo, inputs.archive):
        root, source = roots.preview.resolve(), protected.resolve()
        if root.is_relative_to(source) or source.is_relative_to(root):
            raise ValueError(
                "Preview output must be disjoint from immutable input roots"
            )


@app.command("export")
def export_command(
    inputs: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    preview_dir: Annotated[Path, typer.Option(envvar="SVE_PREVIEW_DIR")],
    cdn_dir: Annotated[Path, typer.Option(envvar="SVE_CDN_DIR")],
    brotli_command: Annotated[
        Path | None, typer.Option(exists=True, dir_okay=False)
    ] = None,
) -> None:
    """Require explicit roots and pins; write no formal index, active state or cache."""
    recipe = Inputs.model_validate_json(inputs.read_bytes())
    roots = Roots(preview_dir, cdn_dir)
    verify_inputs(roots, recipe)
    codec = None if brotli_command is None else command_brotli(brotli_command)
    built = build(recipe)
    snapshot = export_snapshot(
        built.projection, built.ownership, recipe.batch(), brotli=codec
    )
    require_preview(snapshot.manifest)
    _write(
        roots,
        "private/inputs/" + digest(built.input_content)[7:] + ".json",
        built.input_content,
        immutable=True,
    )
    report = write_preview(snapshot, roots, built.report, brotli=codec)
    typer.echo(canonical(report).decode())


@app.command("publish")
def publish_command(
    manifest: Annotated[Path, typer.Argument(exists=True, dir_okay=False)],
) -> None:
    """Reject preview first; complete formal release gates are tracked by #34."""
    require_formal(object_value(parse(manifest.read_bytes())))
    raise typer.BadParameter("Formal release gates are not implemented yet (#34)")

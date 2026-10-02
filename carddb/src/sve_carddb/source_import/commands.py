"""An explicit offline command; no environment-derived manifest or HTTP client."""

import json
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves option types at runtime.
from typing import Annotated

import typer

from sve_carddb.manifest import ManifestError
from sve_carddb.source_import.importer import (
    check_destination,
    prepare,
    register,
    verify_program,
)

app = typer.Typer(
    no_args_is_help=True,
    help="Register already acquired official rule sources offline.",
)


@app.command("register")
def register_command(
    input_dir: Annotated[Path, typer.Option()],
    selection: Annotated[Path, typer.Option()],
    program_root: Annotated[Path, typer.Option()],
    output: Annotated[Path, typer.Option()],
    execute: Annotated[bool, typer.Option("--execute/--check")] = False,
) -> None:
    """Check by default; execution creates only an explicit isolated dataset."""
    try:
        plan = prepare(input_dir, selection)
        if execute:
            report = register(plan, output, program_root)
        else:
            check_destination(plan, output)
            verify_program(plan, program_root)
            report = plan.report() | {"state": "checked"}
        typer.echo(json.dumps(report, sort_keys=True, separators=(",", ":")))
    except OSError, ValueError, ManifestError:
        raise typer.BadParameter(
            "Offline source import verification failed; no recovery was attempted"
        ) from None

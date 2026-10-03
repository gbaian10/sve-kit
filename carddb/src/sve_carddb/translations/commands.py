"""Explicit immutable PR-base checks before adopting name exceptions."""

from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves CLI path annotations at runtime
from typing import Annotated

import typer

from sve_carddb.translations.importer import Inputs
from sve_carddb.translations.name_replay import verify_name_adoption_base

app = typer.Typer(
    no_args_is_help=True, help="Check adopted translation inputs offline."
)


@app.command("check-name-adoption-base")
def check_name_adoption_base(
    authored: Annotated[Path, typer.Option(exists=True, file_okay=False)],
    repository: Annotated[Path, typer.Option(exists=True, file_okay=False)],
    authored_revision: Annotated[str, typer.Option()],
    base_main_revision: Annotated[
        str,
        typer.Option(help="Trusted full PR base-main SHA supplied by the reviewer."),
    ],
) -> None:
    """Check all exception histories; complete frozen evidence replay is still required."""
    inputs = Inputs(authored, repository, authored_revision)
    verify_name_adoption_base(inputs, base_main_revision)
    typer.echo("Name adoption backgrounds belong to the explicit base-main history.")

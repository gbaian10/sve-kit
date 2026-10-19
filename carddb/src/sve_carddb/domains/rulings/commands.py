"""Admit historical ruling domains explicitly, outside ordinary snapshot builds."""

import sqlite3
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves command annotations at runtime
from typing import Annotated

import typer

from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.authored import authored_root
from sve_carddb.domains.effects.reader import load as load_catalog
from sve_carddb.domains.rulings.historical import Observations, references, validate
from sve_carddb.domains.rulings.reader import load

app = typer.Typer(no_args_is_help=True)


@app.command("validate")
def validate_command(
    repo: Annotated[
        Path, typer.Option(help="Repository containing candidate authored data.")
    ],
    build_db: Annotated[
        Path,
        typer.Option(
            help="Previously validated offline build containing the historical source revisions."
        ),
    ],
) -> None:
    """Check candidate rulings and classifications before admitting data changes."""
    if not repo.is_absolute() or not build_db.is_absolute():
        raise typer.BadParameter("Validation requires explicit absolute paths")
    try:
        documents = load(repo)
        explicit = references(documents)
        if explicit:
            catalog = load_catalog(authored_root(repo))
            with open_database(compile_build(), build_db) as db:
                validate(documents, catalog, Observations(db))
    except OSError, ValueError, TypeError, sqlite3.Error:
        # Validation diagnostics can contain source wording or private build paths.
        typer.echo(
            "Historical ruling validation failed; inspect the candidate inputs locally.",
            err=True,
        )
        raise typer.Exit(1) from None
    typer.echo(
        f"Validated {len(explicit)} historical definitions across {len(documents)} rulings."
    )

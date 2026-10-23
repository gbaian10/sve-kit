"""Check Q&A shards using a closed offline build, without loading live sources."""

from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves annotations at runtime
from typing import Annotated

import typer

from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.translations.qa import load
from sve_carddb.domains.translations.qa_checks import check

app = typer.Typer(no_args_is_help=True, help="Check whole-field Q&A translations.")


@app.command("check")
def check_command(
    authored: Annotated[Path, typer.Option(exists=True, file_okay=False)],
    database: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
) -> None:
    """Validate exact sources, adopted names, glossary terms, numbers and answer openings."""
    schema = compile_build(("t0", "qa", "translation_evidence", "translation_names"))
    entries = load(authored)
    with open_database(schema, database) as db:
        errors = check(db, entries)
    for error in errors:
        typer.echo(error)
    typer.echo(f"Q&A entries: {len(entries)}; issues: {len(errors)}")
    if errors:
        raise typer.Exit(1)

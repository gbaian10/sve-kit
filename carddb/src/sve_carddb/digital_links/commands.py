"""Offline candidate reports with explicit inputs and disjoint private output."""

import os
import tempfile
from pathlib import Path
from typing import Annotated

import typer

from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import BuildContext
from sve_carddb.digital_links.candidates import generate
from sve_carddb.translations.sources import Sources

app = typer.Typer(no_args_is_help=True, help="Review offline digital-link candidates.")


def output_path(output: Path, protected: tuple[Path, ...]) -> None:
    """Reject unsafe output before source traversal or temporary-file creation."""
    if not output.is_absolute() or any(
        p.is_symlink() for p in (output, *output.parents)
    ):
        raise ValueError("Candidate output must be absolute and not symlinked")
    for root in protected:
        if output.resolve().is_relative_to(
            root.resolve()
        ) or root.resolve().is_relative_to(output.resolve()):
            raise ValueError("Candidate output overlaps protected inputs")


@app.command("candidates")
def candidates_command(
    draft: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    context: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    repository: Annotated[Path, typer.Option(exists=True, file_okay=False)],
    store: Annotated[
        list[str], typer.Option(help="Declared archive store_id=absolute_path")
    ],
    output: Annotated[Path, typer.Option()],
) -> None:
    """Write a private report; no sampling receipt or authored record is produced."""
    stores: dict[str, Path] = {}
    for value in store:
        identifier, separator, path = value.partition("=")
        if (
            not separator
            or not identifier
            or identifier in stores
            or not Path(path).is_absolute()
        ):
            raise ValueError("Candidate store must be unique id=absolute_path")
        stores[identifier] = Path(path)
    output_path(output, (repository, draft, context, *stores.values()))
    build = BuildContext.model_validate_json(context.read_bytes())
    report = generate(draft.read_bytes(), Sources(stores, repository, build))
    output.parent.mkdir(parents=True, exist_ok=True)
    # Replace only a completed file, so interrupted reports never look complete.
    handle, name = tempfile.mkstemp(prefix=".digital-candidates-", dir=output.parent)
    temporary = Path(name)
    try:
        with os.fdopen(handle, "wb") as stream:
            stream.write(canonical(report))
            stream.flush()
            os.fsync(stream.fileno())
        Path(temporary).replace(output)
    finally:
        temporary.unlink(missing_ok=True)
    typer.echo(canonical(report["summary"]).decode())

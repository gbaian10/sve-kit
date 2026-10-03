"""Explicit offline report inputs with an atomic, disjoint private output."""

import os
import tempfile
from pathlib import Path
from typing import Annotated

import typer

from sve_carddb.build_inputs import BuildContext
from sve_carddb.digital_links.commands import output_path
from sve_carddb.digital_name_policies.loader import load
from sve_carddb.digital_name_policies.report import generate
from sve_carddb.snapshot.values import canonical
from sve_carddb.translations.sources import Sources

app = typer.Typer(
    no_args_is_help=True, help="Validate frozen digital-name policies offline."
)


@app.command("report")
def report_command(  # ruff: ignore[too-many-arguments,too-many-positional-arguments] -- CLI inputs must explicitly declare all immutable stores and comparison roots
    authored: Annotated[Path, typer.Option(exists=True, file_okay=False)],
    authored_revision: Annotated[str, typer.Option()],
    context: Annotated[Path, typer.Option(exists=True, dir_okay=False)],
    repository: Annotated[Path, typer.Option(exists=True, file_okay=False)],
    store: Annotated[
        list[str], typer.Option(help="Declared archive store_id=absolute_path")
    ],
    output: Annotated[Path, typer.Option()],
    baseline: Annotated[Path | None, typer.Option(exists=True, dir_okay=False)] = None,
    baseline_empty: Annotated[bool, typer.Option()] = False,
    compare_context: Annotated[
        Path | None, typer.Option(exists=True, dir_okay=False)
    ] = None,
) -> None:
    """Write diagnostics only; never write policies, receipts, databases or snapshots."""
    stores: dict[str, Path] = {}
    for value in store:
        identifier, separator, path = value.partition("=")
        if (
            not separator
            or not identifier
            or identifier in stores
            or not Path(path).is_absolute()
            or any(p.is_symlink() for p in (Path(path), *Path(path).parents))
        ):
            raise ValueError(
                "Policy report store must be unique id=absolute_non_symlink_path"
            )
        stores[identifier] = Path(path)
    if (baseline is None) != baseline_empty:
        raise ValueError("Policy report requires exactly one explicit baseline")
    protected = (
        authored,
        repository,
        context,
        *stores.values(),
        *(p for p in (baseline, compare_context) if p is not None),
    )
    output_path(output, protected)
    snapshot = load(authored, repository, authored_revision)
    sources = Sources(
        stores, repository, BuildContext.model_validate_json(context.read_bytes())
    )
    comparison = (
        None
        if compare_context is None
        else Sources(
            stores,
            repository,
            BuildContext.model_validate_json(compare_context.read_bytes()),
        )
    )
    report = generate(
        snapshot,
        sources,
        None if baseline is None else baseline.read_bytes(),
        comparison,
    )
    output.parent.mkdir(parents=True, exist_ok=True)
    handle, name = tempfile.mkstemp(prefix=".digital-name-report-", dir=output.parent)
    temporary = Path(name)
    try:
        with os.fdopen(handle, "wb") as stream:
            stream.write(canonical(report))
            stream.flush()
            os.fsync(stream.fileno())
        temporary.replace(output)
    finally:
        temporary.unlink(missing_ok=True)
    typer.echo(canonical(report["summary"]).decode())

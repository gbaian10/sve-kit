"""CLI adapters for the shared authoring workspace."""

from dataclasses import asdict
from pathlib import Path
from typing import TYPE_CHECKING, Annotated, Literal

import typer
from pydantic import JsonValue, ValidationError

from sve_carddb.core.authored import check_path
from sve_carddb.core.json import canonical
from sve_carddb.workflows import authoring

if TYPE_CHECKING:
    from collections.abc import Callable

app = typer.Typer(
    no_args_is_help=True, help="Inspect, write and check exact authored effects."
)
InputOption = Annotated[
    Path, typer.Option("--inputs", help="Absolute JSON profile configuration.")
]
OutputOption = Annotated[Path, typer.Option("--output")]


def _path(path: Path, option: str) -> None:
    if not path.is_absolute():
        raise typer.BadParameter(f"{option} requires an absolute path")
    try:
        check_path(Path(path.anchor), path)
    except ValueError:
        raise typer.BadParameter(f"{option} rejects symlinks and traversal") from None


def _inputs(path: Path) -> authoring.Inputs:
    _path(path, "--inputs")
    try:
        return authoring.Inputs.model_validate_json(path.read_bytes())
    except ValidationError:
        raise typer.BadParameter("Invalid authoring profile configuration") from None


def _run(action: Callable[[], JsonValue], output: Path | None = None) -> None:
    try:
        if output is not None:
            _path(output, "--output")
        report = action()
        if output is not None:
            output.write_bytes(canonical(report) + b"\n")
        else:
            typer.echo(canonical(report).decode())
    except authoring.AuthorInputError as error:
        diagnostic = asdict(error.diagnostic)
        diagnostic.pop("source", None)
        diagnostic["path"] = list(error.diagnostic.path)
        typer.echo(canonical(diagnostic).decode(), err=True)
        raise typer.Exit(3) from None
    except authoring.SourceError as error:
        typer.echo(f"source/{error.reason}", err=True)
        raise typer.Exit(3) from None
    except authoring.WriteError as error:
        typer.echo(str(error), err=True)
        raise typer.Exit(4) from None
    except OSError:
        typer.echo(
            "Authoring I/O failed; verify explicit input/output and baseline paths",
            err=True,
        )
        raise typer.Exit(4) from None
    except authoring.ConfigurationError as error:
        typer.echo(str(error), err=True)
        raise typer.Exit(2) from None
    except ValueError, TypeError:
        typer.echo(
            "Authoring input rejected: invalid configuration or semantic value",
            err=True,
        )
        raise typer.Exit(3) from None


@app.command("inspect")
def inspect_command(
    inputs: InputOption,
    card: Annotated[list[str], typer.Option("--card")],
    output: OutputOption,
) -> None:
    """Write a local report including source wording and unsupported positions."""

    def action() -> JsonValue:
        with authoring.prepare(_inputs(inputs), cards=frozenset(card)) as workspace:
            return {"cards": [authoring.inspect(workspace, number) for number in card]}

    _run(action, output)


@app.command("write")
def write_command(  # ruff: ignore[too-many-arguments] -- keep the complete ordered boundary in one operation
    *,
    inputs: InputOption,
    main: Annotated[Path, typer.Option("--main")],
    output_root: Annotated[Path, typer.Option("--output-root")],
    storage: Annotated[Literal["legacy"], typer.Option("--storage")],
    bodies: Annotated[Path | None, typer.Option("--bodies")] = None,
    card: Annotated[list[str] | None, typer.Option("--card")] = None,
    all_cards: Annotated[bool, typer.Option("--all")] = False,
    remove_prototype: Annotated[bool, typer.Option("--remove-prototype")] = False,
    base: Annotated[str | None, typer.Option("--base")] = None,
    baseline_root: Annotated[Path | None, typer.Option("--baseline-root")] = None,
) -> None:
    """Write the existing storage format from the new handwritten format."""
    if bool(card) == all_cards:
        raise typer.BadParameter("Specify --card or --all and --storage legacy")

    def action() -> JsonValue:
        configured = _inputs(inputs)
        with authoring.prepare(
            configured, cards=frozenset(card or ()), main=main
        ) as workspace:
            return authoring.write(
                workspace,
                main=main,
                bodies=bodies,
                output_root=output_root,
                cards=frozenset(card or ()),
                all_cards=all_cards,
                remove_prototype=remove_prototype,
                base=base,
                baseline_root=baseline_root,
            )

    assert storage == "legacy"
    _run(action)


@app.command("check")
def check_command(
    inputs: InputOption,
    output: OutputOption,
    compare_public: Annotated[Path | None, typer.Option("--compare-public")] = None,
    base: Annotated[str | None, typer.Option("--base")] = None,
    baseline_root: Annotated[Path | None, typer.Option("--baseline-root")] = None,
) -> None:
    """Check the entire indexed set and optionally compare independently prepared profiles."""

    def action() -> JsonValue:
        public = None if compare_public is None else _inputs(compare_public)
        with authoring.prepare(_inputs(inputs)) as workspace:
            return authoring.check(
                workspace, compare_public=public, base=base, baseline_root=baseline_root
            )

    _run(action, output)

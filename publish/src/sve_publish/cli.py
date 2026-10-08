"""Publish verified offline preview exports to R2."""

import typer

from sve_publish.commands import upload_command
from sve_publish.gc_commands import gc_command

app = typer.Typer(
    no_args_is_help=True,
    help="Upload offline previews or collect unused public objects. Formal release gates are not implemented yet (#34).",
)
app.command("upload")(upload_command)
app.command("gc")(gc_command)


def main() -> None:
    """Run the publisher CLI."""
    app()

"""Publish verified offline preview exports to R2."""

import typer

from sve_publish.commands import upload_command
from sve_publish.gc_commands import gc_command, image_gc_command
from sve_publish.image_commands import image_upload_command

app = typer.Typer(
    no_args_is_help=True,
    help="Upload offline previews or collect unused public objects. Formal release gates are not implemented yet (#34).",
)
snapshot = typer.Typer(no_args_is_help=True, help="Upload or collect snapshot JSON.")
images = typer.Typer(no_args_is_help=True, help="Upload or collect card images.")
app.add_typer(snapshot, name="snapshot")
app.add_typer(images, name="images")
snapshot.command("upload")(upload_command)
snapshot.command("gc")(gc_command)
images.command("upload")(image_upload_command)
images.command("gc")(image_gc_command)


def main() -> None:
    """Run the publisher CLI."""
    app()

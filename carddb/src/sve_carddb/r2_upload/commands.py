"""Offline by default; explicit snapshot 2.0 publication and collection."""

import typer

from sve_carddb.r2_upload.v2.commands import upload_v2
from sve_carddb.r2_upload.v2.gc_commands import gc_v2

app = typer.Typer(
    no_args_is_help=True,
    help="Validate and publish snapshot 2.0 releases or inspect collection plans.",
)

app.command("upload-v2")(upload_v2)
app.command("gc-v2")(gc_v2)

"""Offline by default; explicit snapshot 2.0 upload and collection."""

import typer

from sve_carddb.r2_upload.v2.commands import upload_v2
from sve_carddb.r2_upload.v2.gc_commands import gc_v2

app = typer.Typer(
    no_args_is_help=True,
    help="Upload an export-offline preview root or collect unused public objects.",
)

app.command("upload-v2")(upload_v2)
app.command("gc-v2")(gc_v2)

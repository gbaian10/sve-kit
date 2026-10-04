"""Offline by default; execution is an explicit contemporary maintainer action."""

import json
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime annotations
from typing import Annotated

import typer

from sve_carddb.r2_upload.plan import UploadError, plan_preview
from sve_carddb.r2_upload.s3 import S3, Credentials
from sve_carddb.r2_upload.sdk import sdk_client
from sve_carddb.r2_upload.upload import upload
from sve_carddb.r2_upload.v2.commands import upload_v2
from sve_carddb.r2_upload.v2.gc_commands import gc_v2


def _authorization(execute: bool, confirmed: bool) -> None:
    if execute and not confirmed:
        raise UploadError("Execution requires contemporary maintainer authorization")


def _target(account: str | None, bucket: str | None) -> tuple[str, str]:
    if account is None or bucket is None:
        raise UploadError("Execution requires an explicit R2 account ID and bucket")
    return account, bucket


app = typer.Typer(
    no_args_is_help=True,
    help="Validate public previews and upload conditionally to R2.",
)


@app.command("upload-preview")
def upload_command(
    preview_dir: Annotated[Path, typer.Option()],
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    confirm_maintainer_authorization: Annotated[bool, typer.Option()] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
) -> None:
    """Print counts and bytes without contacting R2 unless execution is authorized."""
    try:
        _authorization(execute, confirm_maintainer_authorization)
        plan = plan_preview(preview_dir)
        report = plan.report()
        if execute:
            account_id, bucket = _target(account_id, bucket)
            credentials = Credentials.environment()
            with sdk_client(account_id, bucket, credentials) as client:
                remote = S3(account_id, bucket, credentials, client)
                report = upload(plan, remote)
        typer.echo(json.dumps(report, sort_keys=True, separators=(",", ":")))
    except UploadError as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError:
        raise typer.BadParameter("Local upload preparation failed") from None


app.command("upload-v2")(upload_v2)

app.command("gc-v2")(gc_v2)

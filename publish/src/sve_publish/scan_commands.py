"""Upload a scanner release folder: offline dry-run by default."""

import json
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime annotations
from typing import Annotated

import typer
from sve_carddb.export.read_api import ExportError

from sve_publish.adapter import R2Store
from sve_publish.commands import target_values
from sve_publish.scan import load_release, report, upload_scan
from sve_publish.sdk import Credentials, sdk_client


def scan_upload_command(
    release_dir: Annotated[
        Path,
        typer.Argument(
            help="Folder written by `sve_scan release`: manifest.json, models/, index/."
        ),
    ],
    *,
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
    jobs: Annotated[int, typer.Option(min=1, help="Concurrent object uploads.")] = 16,
) -> None:
    """Dry-run validates the release without credentials or network access."""
    if not release_dir.is_absolute():
        raise typer.BadParameter(
            "RELEASE_DIR must be an absolute path", param_hint="RELEASE_DIR"
        )
    try:
        release = load_release(release_dir)
        result = report(release)
        if execute:
            credentials = Credentials.environment()
            account, target = target_values(account_id, bucket)
            with sdk_client(account, target, credentials) as client:
                store = R2Store(account, target, credentials, client)
                result |= upload_scan(
                    store,
                    release,
                    jobs=jobs,
                    progress=lambda line: typer.echo(line, err=True),
                )
            result["remote"] = "checked"
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except ExportError as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError, TypeError, KeyError:
        raise typer.BadParameter("Local scan upload preparation failed") from None

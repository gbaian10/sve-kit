"""Upload an export-offline preview root: offline dry-run by default."""

import json
import os
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime annotations
from typing import TYPE_CHECKING, Annotated

import typer
from sve_carddb.export.read_api import ExportError, load_export

from sve_publish.adapter import R2Store
from sve_publish.cli_paths import required_root
from sve_publish.publish import report, upload
from sve_publish.sdk import Credentials, sdk_client

if TYPE_CHECKING:
    from sve_carddb.export.read_api import Export


def upload_command(
    *,
    export_dir: Annotated[Path | None, typer.Option(envvar="SVE_EXPORT_DIR")] = None,
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
    jobs: Annotated[int, typer.Option(min=1, help="Concurrent object uploads.")] = 16,
) -> None:
    """Dry-run validates the export without credentials or network access."""
    export_dir = required_root(export_dir, "--export-dir", "SVE_EXPORT_DIR")
    try:
        export = load_export(export_dir)
        result = report(export)
        if execute:
            result = _execute(export, account_id, bucket, jobs)
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except ExportError as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError, TypeError, KeyError:
        raise typer.BadParameter("Local upload preparation failed") from None


def _execute(
    export: Export,
    account_id: str | None,
    bucket: str | None,
    jobs: int,
) -> dict[str, object]:
    credentials = Credentials.environment()
    account, target = target_values(account_id, bucket)
    with sdk_client(account, target, credentials) as origin_client:
        store = R2Store(account, target, credentials, origin_client)
        return upload(
            store,
            export,
            jobs=jobs,
            progress=lambda line: typer.echo(line, err=True),
        ) | {
            "data_version": export.entry["data_version"],
            "manifest_sha256": export.entry["manifest_sha256"],
        }


def target_values(account_id: str | None, bucket: str | None) -> tuple[str, str]:
    """Require both explicit deployment values before any HTTP client is opened."""
    account = account_id or os.environ.get("R2_ACCOUNT_ID", "")
    target = bucket or os.environ.get("R2_DEV_BUCKET", "")
    if not account or not target:
        raise ExportError("Execution requires an explicit R2 account ID and bucket")
    return account, target

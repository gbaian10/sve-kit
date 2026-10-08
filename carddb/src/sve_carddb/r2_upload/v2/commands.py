"""Upload an export-offline preview root: offline dry-run by default."""

import json
import os
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime annotations
from typing import TYPE_CHECKING, Annotated

import httpx
import typer

from sve_carddb.cli_paths import required_root
from sve_carddb.r2_upload.boundary import UploadError
from sve_carddb.r2_upload.sdk import Credentials, sdk_client
from sve_carddb.r2_upload.v2.adapter import R2Store
from sve_carddb.r2_upload.v2.export import load_export
from sve_carddb.r2_upload.v2.freshness import CDNFreshness, cdn_root
from sve_carddb.r2_upload.v2.publish import report, upload

if TYPE_CHECKING:
    from sve_carddb.r2_upload.v2.export import Export


def upload_v2(
    *,
    export_dir: Annotated[Path | None, typer.Option(envvar="SVE_EXPORT_DIR")] = None,
    cdn_base_url: Annotated[str | None, typer.Option()] = None,
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    skip_cdn_verify: Annotated[bool, typer.Option("--skip-cdn-verify")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
) -> None:
    """Dry-run validates the export without credentials or network access."""
    export_dir = required_root(export_dir, "--export-dir", "SVE_EXPORT_DIR")
    try:
        root = _cdn(cdn_base_url, required=execute and not skip_cdn_verify)
        export = load_export(export_dir)
        result = report(export)
        if execute:
            result = _execute(
                export, None if skip_cdn_verify else root, account_id, bucket
            )
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except UploadError as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError, TypeError, KeyError:
        raise typer.BadParameter("Local upload preparation failed") from None


def _cdn(value: str | None, *, required: bool) -> str | None:
    if value is None and required:
        raise UploadError(
            "CDN verification needs --cdn-base-url, or pass --skip-cdn-verify"
        )
    return None if value is None else cdn_root(value)


def _execute(
    export: Export, root: str | None, account_id: str | None, bucket: str | None
) -> dict[str, object]:
    credentials = Credentials.environment()
    account, target = target_values(account_id, bucket)
    with (
        sdk_client(account, target, credentials) as origin_client,
        httpx.Client(trust_env=False, follow_redirects=False, timeout=30) as client,
    ):
        store = R2Store(account, target, credentials, origin_client)
        cdn = None if root is None else CDNFreshness(root, client)
        return upload(store, export, cdn) | {
            "data_version": export.entry["data_version"],
            "manifest_sha256": export.entry["manifest_sha256"],
        }


def target_values(account_id: str | None, bucket: str | None) -> tuple[str, str]:
    """Require both explicit deployment values before any HTTP client is opened."""
    account = account_id or os.environ.get("R2_ACCOUNT_ID", "")
    target = bucket or os.environ.get("R2_DEV_BUCKET", "")
    if not account or not target:
        raise UploadError("Execution requires an explicit R2 account ID and bucket")
    return account, target

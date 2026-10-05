"""Explicit local execution of a frozen, formally gated 2.0 release."""

import json
import os
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime annotations
from typing import TYPE_CHECKING, Annotated

if TYPE_CHECKING:
    from sve_carddb.snapshot.publish import Ledger, Release

import httpx
import typer

from sve_carddb.r2_upload.boundary import UploadError
from sve_carddb.r2_upload.sdk import Credentials, sdk_client
from sve_carddb.r2_upload.v2.adapter import R2Store
from sve_carddb.r2_upload.v2.bundle import (
    ledger_at,
    load_bundle,
    report,
    save_checkpoint,
    verify_checkpoint,
)
from sve_carddb.r2_upload.v2.freshness import CDNFreshness
from sve_carddb.r2_upload.v2.freshness import cdn_root as checked_cdn_root
from sve_carddb.snapshot.publish import PublishError, publish


def upload_v2(  # ruff: ignore[too-many-arguments] -- all deployment and recovery inputs must be explicit, independent flags
    *,
    release_dir: Annotated[Path, typer.Option()],
    ledger_dir: Annotated[Path, typer.Option()],
    backup_dir: Annotated[Path, typer.Option()],
    checkpoint_file: Annotated[Path, typer.Option()],
    cdn_base_url: Annotated[str, typer.Option()],
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    skip_cdn_verify: Annotated[bool, typer.Option("--skip-cdn-verify")] = False,
    confirm_maintainer_authorization: Annotated[bool, typer.Option()] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
) -> None:
    """Dry-run is offline and read-only; execution never promotes preview artifacts."""
    try:
        _authorize(execute, confirm_maintainer_authorization)
        root = checked_cdn_root(cdn_base_url)
        ledger = ledger_at(ledger_dir, backup_dir)
        verify_checkpoint(checkpoint_file, ledger)
        release = load_bundle(release_dir, ledger, cdn_root=root)
        result = report(release, ledger)
        if execute:
            result |= _execute(
                ledger,
                release,
                checkpoint_file,
                account_id,
                bucket,
                skip_cdn_verify=skip_cdn_verify,
            )
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except (PublishError, UploadError) as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError, TypeError, KeyError:
        raise typer.BadParameter("Local 2.0 publication preparation failed") from None


def _authorize(execute: bool, confirmed: bool) -> None:
    if execute and not confirmed:
        raise PublishError("Execution requires contemporary maintainer authorization")


def _execute(
    ledger: Ledger,
    release: Release,
    checkpoint: Path,
    account_id: str | None,
    bucket: str | None,
    *,
    skip_cdn_verify: bool,
) -> dict[str, object]:
    credentials = Credentials.environment()
    account, target = target_values(account_id, bucket)
    with (
        sdk_client(account, target, credentials) as origin_client,
        httpx.Client(trust_env=False, follow_redirects=False, timeout=30) as cdn_client,
    ):
        store = R2Store(account, target, credentials, origin_client)
        try:
            receipt = publish(
                ledger,
                store,
                release,
                CDNFreshness(release.cdn_root, cdn_client),
                verify_cdn=not skip_cdn_verify,
            )
            result: dict[str, object] = {
                "mode": "execute",
                "receipt": receipt,
                "remote_existence": "verified",
            }
            if skip_cdn_verify:
                result["cdn_verification"] = "skipped"
            return result
        finally:
            save_checkpoint(checkpoint, ledger)


def target_values(account_id: str | None, bucket: str | None) -> tuple[str, str]:
    """Require both explicit deployment values before any HTTP client is opened."""
    account = account_id or os.environ.get("R2_ACCOUNT_ID", "")
    target = bucket or os.environ.get("R2_DEV_BUCKET", "")
    if not account or not target:
        raise PublishError("Execution requires an explicit R2 account ID and bucket")
    return account, target

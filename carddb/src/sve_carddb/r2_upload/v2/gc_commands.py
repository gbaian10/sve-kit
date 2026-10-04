"""Offline saved-list preview; explicit inspection and separately approved GC."""

import json
import os
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves runtime option annotations
from typing import Annotated

import httpx
import typer

from sve_carddb.r2_upload.plan import UploadError, read_member
from sve_carddb.r2_upload.s3 import Credentials
from sve_carddb.r2_upload.v2 import gc
from sve_carddb.r2_upload.v2.adapter import PUBLIC_PREFIXES, R2Store
from sve_carddb.r2_upload.v2.bundle import directory, ledger_at, verify_checkpoint
from sve_carddb.r2_upload.v2.commands import target_values
from sve_carddb.snapshot.publish.storage import PublishError
from sve_carddb.snapshot.values import canonical, object_value, parse


def gc_v2(  # ruff: ignore[too-many-arguments] -- remote credentials, private ledger and per-run approval are separate operator inputs
    *,
    plan_file: Annotated[Path, typer.Option()],
    ledger_dir: Annotated[Path, typer.Option()],
    backup_dir: Annotated[Path, typer.Option()],
    checkpoint_file: Annotated[Path, typer.Option()],
    namespace: Annotated[list[str] | None, typer.Option()] = None,
    inspect_remote: Annotated[bool, typer.Option()] = False,
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    confirm_delete: Annotated[str, typer.Option()] = "",
    confirm_maintainer_authorization: Annotated[bool, typer.Option()] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
) -> None:
    """Without explicit inspection/execution flags, only display a saved local list."""
    try:
        result = _run(
            plan_file,
            ledger_dir,
            backup_dir,
            checkpoint_file,
            namespace,
            inspect_remote,
            execute,
            confirm_delete,
            confirm_maintainer_authorization,
            account_id,
            bucket,
        )
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except (PublishError, UploadError) as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError, TypeError, KeyError:
        raise typer.BadParameter("Local GC preparation failed") from None


def _run(  # ruff: ignore[too-many-arguments,too-many-positional-arguments] -- internal forwarding preserves explicit CLI inputs
    plan_file: Path,
    ledger_dir: Path,
    backup_dir: Path,
    checkpoint_file: Path,
    namespaces: list[str] | None,
    inspect_remote: bool,
    execute: bool,
    confirmed: str,
    authorized: bool,
    account_id: str | None,
    bucket: str | None,
) -> dict[str, object]:
    if inspect_remote and execute:
        raise PublishError("GC inspection and deletion must be separate actions")
    directory(plan_file.parent)
    ledger = ledger_at(ledger_dir, backup_dir)
    verify_checkpoint(checkpoint_file, ledger)
    plan = {}
    result: dict[str, object] = {}
    if not inspect_remote:
        plan = object_value(parse(read_member(plan_file.parent, plan_file.name)))
        result = gc.report(plan)
        if not execute:
            return result
        if not authorized or confirmed != gc.confirmation(plan):
            raise PublishError(
                "GC execution requires authorization and the exact confirmation string"
            )
    elif not authorized or not namespaces:
        raise PublishError(
            "Remote GC inspection requires explicit authorization and namespaces"
        )
    if inspect_remote:
        if plan_file.exists() or plan_file.is_symlink():
            raise PublishError("GC approval list destination already exists")
        if not frozenset(namespaces or ()) <= PUBLIC_PREFIXES:
            raise PublishError("GC namespace is not explicitly public")
    credentials = Credentials.environment()
    account, target = target_values(account_id, bucket)
    with httpx.Client(trust_env=False, follow_redirects=False, timeout=30) as client:
        store = R2Store(account, target, credentials, client)
        if inspect_remote:
            plan = gc.inspect(ledger, store, namespaces=frozenset(namespaces or ()))
            descriptor = os.open(plan_file, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(descriptor, "wb") as file:
                file.write(canonical(plan))
                file.flush()
                os.fsync(file.fileno())
            return gc.report(plan) | {
                "mode": "remote_inspection",
                "remote_existence": "verified",
            }
        deleted = gc.execute(ledger, store, plan, confirmed=confirmed)
        return result | {
            "mode": "execute",
            "deleted": list(deleted),
            "remote_existence": "verified",
        }

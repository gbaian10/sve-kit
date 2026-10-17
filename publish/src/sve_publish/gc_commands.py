"""List or delete public objects outside the remote current/previous window."""

import json
from typing import Annotated

import typer
from sve_carddb.export.read_api import ExportError

from sve_publish import gc
from sve_publish.adapter import IMAGE_PREFIXES, SNAPSHOT_PREFIXES, R2Store
from sve_publish.commands import target_values
from sve_publish.sdk import Credentials, sdk_client


def _gc_command(
    *,
    images: bool,
    namespace: list[str] | None,
    execute: bool,
    account_id: str | None,
    bucket: str | None,
) -> None:
    try:
        credentials = Credentials.environment()
        account, target = target_values(account_id, bucket)
        with sdk_client(account, target, credentials) as client:
            collect = gc.collect_images if images else gc.collect
            result = collect(
                R2Store(account, target, credentials, client),
                frozenset(
                    namespace or (IMAGE_PREFIXES if images else SNAPSHOT_PREFIXES)
                ),
                execute=execute,
                progress=lambda line: typer.echo(line, err=True),
            )
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except ExportError as error:
        raise typer.BadParameter(str(error)) from None


def gc_command(
    *,
    namespace: Annotated[list[str] | None, typer.Option()] = None,
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
) -> None:
    """Dry-run reads the bucket and lists candidates; --execute deletes them."""
    _gc_command(
        images=False,
        namespace=namespace,
        execute=execute,
        account_id=account_id,
        bucket=bucket,
    )


def image_gc_command(
    *,
    namespace: Annotated[list[str] | None, typer.Option()] = None,
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
) -> None:
    """Read current image references and list or delete only image objects."""
    _gc_command(
        images=True,
        namespace=namespace,
        execute=execute,
        account_id=account_id,
        bucket=bucket,
    )

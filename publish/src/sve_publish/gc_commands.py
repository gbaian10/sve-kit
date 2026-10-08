"""List or delete public objects outside the remote current/previous window."""

import json
from typing import Annotated

import typer
from sve_carddb.export.read_api import ExportError

from sve_publish import gc
from sve_publish.adapter import PUBLIC_PREFIXES, R2Store
from sve_publish.commands import target_values
from sve_publish.sdk import Credentials, sdk_client


def gc_command(
    *,
    namespace: Annotated[list[str] | None, typer.Option()] = None,
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
) -> None:
    """Dry-run reads the bucket and lists candidates; --execute deletes them."""
    try:
        credentials = Credentials.environment()
        account, target = target_values(account_id, bucket)
        with sdk_client(account, target, credentials) as client:
            result = gc.collect(
                R2Store(account, target, credentials, client),
                frozenset(namespace or PUBLIC_PREFIXES),
                execute=execute,
            )
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except ExportError as error:
        raise typer.BadParameter(str(error)) from None

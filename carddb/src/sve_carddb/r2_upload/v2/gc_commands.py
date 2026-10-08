"""List or delete public objects outside the remote current/previous window."""

import json
from typing import Annotated

import typer

from sve_carddb.r2_upload.sdk import Credentials, sdk_client
from sve_carddb.r2_upload.v2 import gc
from sve_carddb.r2_upload.v2.adapter import PUBLIC_PREFIXES, R2Store
from sve_carddb.r2_upload.v2.commands import target_values
from sve_carddb.snapshot.read_api import ExportError as UploadError


def gc_v2(
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
    except UploadError as error:
        raise typer.BadParameter(str(error)) from None

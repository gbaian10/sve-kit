"""Independent image handoff upload; dry-run opens no images or remote clients."""

import json
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves annotations at runtime
from typing import Annotated

import typer
from sve_carddb.export.read_api import ExportError, load_image_upload

from sve_publish.adapter import R2Store
from sve_publish.commands import target_values
from sve_publish.images import upload_images
from sve_publish.sdk import Credentials, sdk_client


def image_upload_command(
    *,
    image_upload: Annotated[Path, typer.Option()],
    image_dir: Annotated[Path, typer.Option()],
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
    jobs: Annotated[int, typer.Option(min=1)] = 16,
) -> None:
    """Compare one producer description with this bucket's successful uploads."""
    try:
        description = load_image_upload(image_upload)
        result: dict[str, object] = {
            "mode": "dry_run",
            "image_files": len(description.objects),
            "image_bytes": sum(item.bytes for item in description.objects),
            "remote": "not_checked",
        }
        if execute:
            credentials = Credentials.environment()
            account, target = target_values(account_id, bucket)
            with sdk_client(account, target, credentials) as client:
                result = upload_images(
                    R2Store(account, target, credentials, client),
                    description,
                    image_dir,
                    jobs=jobs,
                    progress=lambda line: typer.echo(line, err=True),
                )
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except ExportError as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError, TypeError, KeyError:
        raise typer.BadParameter("Local image upload preparation failed") from None

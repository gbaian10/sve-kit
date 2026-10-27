"""Independent image uploads with explicit scope and read-only comparison."""

import json
from pathlib import Path  # ruff: ignore[typing-only-standard-library-import] -- Typer resolves annotations at runtime
from typing import TYPE_CHECKING, Annotated

import typer
from sve_carddb.export.read_api import (
    ExportError,
    directory,
    load_image_scope,
    load_image_upload,
    read_image_inventory,
    read_member,
)

from sve_publish.adapter import R2Store
from sve_publish.commands import target_values
from sve_publish.images import plan_images, upload_images
from sve_publish.inventory import INVENTORY, read_inventory
from sve_publish.sdk import Credentials, sdk_client

if TYPE_CHECKING:
    from sve_carddb.export.read_api import ImageUpload


def selected_keys(
    description: ImageUpload,
    scopes: tuple[str, ...],
    printings: tuple[str, ...],
    manifest: Path | None,
    remote_inventory: Path | None,
    *,
    execute: bool,
) -> frozenset[str] | None:
    """Reject incomplete or conflicting scope arguments before credentials or network."""
    if execute and remote_inventory is not None:
        raise ExportError("--remote-inventory is only for a scoped dry-run")
    if not scopes and not printings:
        if manifest is not None or remote_inventory is not None:
            raise ExportError("Scope inputs require --scope or --printing-id")
        return None
    if manifest is None:
        raise ExportError("Scoped upload requires --scope-manifest")
    membership = load_image_scope(manifest, description)
    try:
        return membership.select(scopes, printings)
    except ValueError as error:
        raise ExportError(str(error)) from None


def image_upload_command(  # ruff: ignore[too-many-arguments] -- Typer exposes explicit producer, scope and remote options
    *,
    image_upload: Annotated[Path, typer.Option()],
    image_dir: Annotated[Path, typer.Option()],
    execute: Annotated[bool, typer.Option("--execute/--dry-run")] = False,
    account_id: Annotated[str | None, typer.Option()] = None,
    bucket: Annotated[str | None, typer.Option()] = None,
    jobs: Annotated[int, typer.Option(min=1)] = 16,
    scope: Annotated[list[str] | None, typer.Option()] = None,
    printing_id: Annotated[list[str] | None, typer.Option()] = None,
    scope_manifest: Annotated[Path | None, typer.Option()] = None,
    remote_inventory: Annotated[
        Path | None,
        typer.Option(
            help="Downloaded remote image inventory for an offline scoped dry-run."
        ),
    ] = None,
) -> None:
    """Compare one producer description with this bucket's successful uploads."""
    try:  # ruff: ignore[too-many-statements-in-try-clause] -- the CLI redacts failures from local and remote preparation at one boundary
        description = load_image_upload(image_upload)
        selected = selected_keys(
            description,
            tuple(scope or ()),
            tuple(printing_id or ()),
            scope_manifest,
            remote_inventory,
            execute=execute,
        )
        scoped = selected is not None
        result: dict[str, object] = {
            "mode": "dry_run",
            "image_files": len(description.objects),
            "image_bytes": sum(item.bytes for item in description.objects),
            "remote": "not_checked",
        }
        if scoped and remote_inventory is not None:
            directory(remote_inventory.parent)
            before = read_image_inventory(
                read_member(remote_inventory.parent, remote_inventory.name)
            )
            result = plan_images(
                description, {item.key: item for item in before.objects}, selected
            )
            result["remote"] = "provided_inventory"
        elif execute or scoped:
            credentials = Credentials.environment()
            account, target = target_values(account_id, bucket)
            with sdk_client(account, target, credentials) as client:
                store = R2Store(account, target, credentials, client)
                if execute:
                    result = upload_images(
                        store,
                        description,
                        image_dir,
                        jobs=jobs,
                        progress=lambda line: typer.echo(line, err=True),
                        selected=selected,
                    )
                else:
                    before = read_inventory(store.get(INVENTORY))
                    result = plan_images(
                        description,
                        {item.key: item for item in before.objects},
                        selected,
                    )
        typer.echo(json.dumps(result, sort_keys=True, separators=(",", ":")))
    except ExportError as error:
        raise typer.BadParameter(str(error)) from None
    except OSError, ValueError, TypeError, KeyError:
        raise typer.BadParameter("Local image upload preparation failed") from None

"""Register manual unlisted printings from a reviewed spec, allocating their int_ids."""

import hashlib
from pathlib import Path
from typing import Annotated, Literal

import typer
from PIL import Image
from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import canonical, parse
from sve_carddb.core.models import RecordData
from sve_carddb.domains.manual_printings import (
    ManualSpec,
    load_manual_printings,
    plan_manual_files,
)
from sve_carddb.domains.manual_printings.records import ManualName, Owner
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import write_files

app = typer.Typer(
    no_args_is_help=True,
    help="Register official-catalog-unlisted printings from reviewed specs.",
)
AuthoredOption = Annotated[
    Path, typer.Option("--authored", help="Absolute path of the authored/ root.")
]


class MasterImage(RecordData):
    """A local master PNG; only its hash and dimensions enter the authored record."""

    master: Path
    origin: Literal["maintainer_photo", "official_image", "official_crop"]
    source_url: str | None


class Spec(RecordData):
    key: str
    region: Literal["jp", "en"]
    home_set_id: Owner
    card_type: Literal["ep", "sep"]
    card_no: str | None
    card_no_state: Literal["none", "official", "provisional"]
    printed_code: str | None
    name: ManualName
    release_kind: Literal["event", "basic"]
    references: tuple[str, ...]
    image: MasterImage | None


def _absolute(path: Path, option: str) -> Path:
    if not path.is_absolute():
        raise typer.BadParameter(f"{option} requires an absolute path")
    return path


def _image(master: MasterImage) -> dict[str, JsonValue]:
    path = _absolute(master.master, "image.master")
    raw = path.read_bytes()
    with Image.open(path) as opened:
        if opened.format != "PNG":
            raise typer.BadParameter(f"Master must be a PNG: {path.name}")
        width, height = opened.size
    return {
        "sha256": "sha256:" + hashlib.sha256(raw).hexdigest(),
        "width": width,
        "height": height,
        "origin": master.origin,
        "source_url": master.source_url,
        "approved": True,
    }


def _specs(path: Path) -> tuple[ManualSpec, ...]:
    try:
        raw = parse(path.read_bytes())
        items = raw if isinstance(raw, list) else [raw]
        specs = tuple(Spec.model_validate_json(canonical(item)) for item in items)
        return tuple(
            ManualSpec.model_validate_json(
                canonical(
                    spec.model_dump(mode="json", exclude={"image"})
                    | {"image": None if spec.image is None else _image(spec.image)}
                )
            )
            for spec in specs
        )
    except ValidationError as error:
        details = "; ".join(
            ".".join(map(str, issue["loc"])) + ":" + issue["type"]
            for issue in error.errors(include_input=False, include_context=False)
        )
        raise typer.BadParameter("Invalid manual printing spec: " + details) from None


@app.command("append")
def append_command(
    authored: AuthoredOption,
    spec: Annotated[
        Path, typer.Option("--spec", help="Absolute JSON list of printings to add.")
    ],
) -> None:
    """Validate the whole input, allocate unlisted int_ids and append new shards."""
    root = _absolute(authored, "--authored")
    specs = _specs(_absolute(spec, "--spec"))
    registry = load_registry(root)
    index = registry.files.index()
    existing = load_manual_printings(
        root, next_unlisted_int_id=index.unlisted_cursors(), registry=registry
    )
    plan = plan_manual_files(root, existing, index, specs)
    write_files(plan.files)
    # Re-read so a planning mistake fails here instead of in the next build.
    load_manual_printings(
        root,
        next_unlisted_int_id=load_registry(root).files.index().unlisted_cursors(),
        registry=registry,
    )
    typer.echo(
        canonical(
            {
                "written_files": list[JsonValue](
                    sorted(path.relative_to(root).as_posix() for path in plan.files)
                ),
                "allocated": [
                    {
                        "key": entry.data.key,
                        "region": entry.data.region,
                        "int_id": entry.data.int_id,
                        "printing_id": entry.data.id,
                    }
                    for entry in plan.entries
                ],
            }
        ).decode()
    )


@app.command("check")
def check_command(authored: AuthoredOption) -> None:
    """Validate manual printings against the registry and the unlisted cursors."""
    root = _absolute(authored, "--authored")
    registry = load_registry(root)
    snapshot = load_manual_printings(
        root,
        next_unlisted_int_id=registry.files.index().unlisted_cursors(),
        registry=registry,
    )
    typer.echo(
        canonical(
            {
                "printings": len(snapshot.records),
                "with_image": sum(
                    data.image is not None for data in snapshot.printings()
                ),
                "shards": [shard.path for shard in snapshot.shards],
            }
        ).decode()
    )

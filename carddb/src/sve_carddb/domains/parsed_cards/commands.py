"""Explicit local candidates; commands never publish or replace reviewed data."""

import json
from pathlib import Path
from typing import Annotated, Self

import typer
from pydantic import model_validator

from sve_carddb.core.json import parse
from sve_carddb.core.models import Hash, RecordData, Text
from sve_carddb.core.provenance import Revision
from sve_carddb.core.regions import Region
from sve_carddb.domains.card_extras.reviewed_errata import validate as validate_errata
from sve_carddb.domains.card_extras.reviewed_errata_verify import (
    verify as verify_errata,
)
from sve_carddb.domains.parsed_cards import codec
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.models import BatchFile, Family, Index
from sve_carddb.domains.parsed_cards.storage import load, write
from sve_carddb.domains.parsed_cards.wire import CardFile, EvidenceFile
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.source_archive import ArchiveError

app = typer.Typer(no_args_is_help=True)


class SourceBatch(RecordData):
    region: Region
    root: Path
    store_id: Text
    batch_id: Hash


class Filing(RecordData):
    region: Region
    card_no: Text
    family: Family


class Request(RecordData):
    sources: tuple[SourceBatch, ...]
    filing: tuple[Filing, ...]
    producer_revision: Revision

    @model_validator(mode="after")
    def check_filing(self) -> Self:
        """Reject ambiguous author-supplied filing before opening source archives."""
        if len({(f.region, f.card_no) for f in self.filing}) != len(self.filing):
            raise ValueError("Duplicate filing")
        return self


def _absolute(path: Path) -> Path:
    if not path.is_absolute():
        raise ValueError("An explicit absolute path is required")
    return path


@app.command("generate")
def generate_command(
    request: Path,
    output: Path,
    previous: Annotated[Path | None, typer.Option()] = None,
) -> None:
    """Read a private generation request and write a new portable candidate root."""
    try:
        raw = _absolute(request).read_bytes()
        parse(raw)
        spec = Request.model_validate_json(raw)
        filing = {(f.region, f.card_no): f.family for f in spec.filing}
        sources = [
            (s.region, FrozenSources(_absolute(s.root), s.store_id, s.batch_id))
            for s in spec.sources
        ]
        result = generate(
            sources,
            filing,
            producer_revision=spec.producer_revision,
            previous=load(_absolute(previous)) if previous else None,
        )
        write(_absolute(output), result)
    except OSError, ValueError:
        # Validation errors can embed complete official wording and private request paths.
        typer.echo(
            "Parsed candidate generation failed; inspect the inputs locally.", err=True
        )
        raise typer.Exit(1) from None
    typer.echo(f"Validated {len(result.index.files)} files; no publication performed.")


@app.command("validate")
def validate_command(
    root: Path, previous: Annotated[Path | None, typer.Option()] = None
) -> None:
    """Check the full closure and, for updates, preservation of reviewed history."""
    try:
        catalog = load(_absolute(root))
        if previous:
            catalog.validate_update(load(_absolute(previous)))
        validate_errata(root, previous=previous)
    except OSError, ValueError:
        typer.echo("Parsed input validation failed.", err=True)
        raise typer.Exit(1) from None
    typer.echo(f"Validated {len(catalog.index.files)} files.")


@app.command("schema")
def schema_command(output: Path) -> None:
    """Write the closed index, card and evidence schemas into a new directory."""
    _absolute(output).mkdir(parents=True)
    for name, model in (
        ("index", Index),
        ("batch", BatchFile),
        ("cards", CardFile),
        ("evidence", EvidenceFile),
    ):
        (output / f"{name}.schema.json").write_text(
            json.dumps(codec.schema(model), ensure_ascii=False, indent=2) + "\n",
            encoding="utf-8",
        )


@app.command("verify-errata")
def verify_errata_command(root: Path, archive: Path, store_id: str) -> None:
    """Verify announcement byte ranges using an explicitly selected frozen archive."""
    try:
        result = verify_errata(_absolute(root), _absolute(archive), store_id)
    except OSError, ValueError, ArchiveError:
        typer.echo("Frozen errata evidence verification failed.", err=True)
        raise typer.Exit(1) from None
    typer.echo(
        f"Verified {result['notices']} notices and {result['ranges']} byte ranges."
    )

"""One synthetic parsed page for CLI errors that precede source admission."""

import hashlib
from dataclasses import dataclass
from datetime import UTC, datetime
from pathlib import Path, PurePosixPath
from typing import TYPE_CHECKING

from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.storage import write
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind, Manifest, Resource
from sve_carddb.ingest.archive.source_archive import ArchiveStore, seal_batch
from sve_carddb.parse.pages import official_jp

from .prepared_values import freeze, restore, shared_preparation

if TYPE_CHECKING:
    from sve_carddb.workflows import authoring


@dataclass(frozen=True)
class RepositoryCheck:
    exit_code: int
    stdout: str
    output: Path


def repository_check(root: Path, inputs: authoring.Inputs) -> RepositoryCheck:
    root.mkdir(exist_ok=True)

    def prepare() -> bytes:
        path = root / "inputs.json"
        path.write_text(inputs.model_dump_json())
        output = root / "report.json"
        result = CliRunner().invoke(
            app, ["author", "check", "--inputs", str(path), "--output", str(output)]
        )
        # Failed commands must expose their error instead of publishing a cache entry.
        assert result.exit_code == 0, result.stderr
        return freeze(RepositoryCheck(result.exit_code, result.stdout, output))

    return restore(shared_preparation(root / "result.pickle", prepare), RepositoryCheck)


def synthetic_card_inputs(root: Path) -> tuple[Path, str]:
    data = root / "data"
    data.mkdir(parents=True)
    store = ArchiveStore(
        data,
        data / "manifest/manifest.sqlite",
        data / "manifest/.lock",
        root / "archive",
        "synthetic-authoring",
        (),
    )
    raw = (
        Path(__file__).resolve().parents[1] / "fixtures/synthetic_jp/F07-card.html"
    ).read_bytes()
    path = data / "raw/card.html"
    path.parent.mkdir()
    path.write_bytes(raw)
    now = datetime(2026, 9, 29, tzinfo=UTC)
    with Manifest.open(store.manifest_path) as manifest, manifest.transaction():
        manifest.resources.put(
            Resource(
                url=official_jp.card_url("SYN01-007"),
                region=SourceRegion.JP,
                kind=Kind.CARD,
                path=PurePosixPath("raw/card.html"),
                sha256=hashlib.sha256(raw).hexdigest(),
                raw_bytes=len(raw),
                stored_bytes=len(raw),
                content_type="text/html",
                etag=None,
                last_modified=None,
                first_fetched_at=now,
                last_checked_at=now,
                last_changed_at=now,
                archived_at=None,
            )
        )
    batch = seal_batch(store)
    catalog = generate(
        [("jp", FrozenSources(store.root, store.store_id, batch.batch_id))],
        {("jp", "SYN01-007"): "SYN"},
        producer_revision="a" * 40,
    )
    parsed = root / "parsed"
    write(parsed, catalog)
    return parsed, batch.batch_id

"""Exercise admission-time verification against a real synthetic sealed archive."""

import json
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.core.json import digest
from sve_carddb.domains.card_extras.reviewed_errata import (
    Block,
    Evidence,
    File,
    Index,
    Notice,
    Notices,
    Slice,
)
from sve_carddb.domains.card_extras.reviewed_errata_verify import verify
from sve_carddb.domains.parsed_cards.commands import app
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import seal_batch

from ...ingest.test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.ingest.archive.source_archive import ArchiveStore


@pytest.fixture
def inputs(tmp_path: Path) -> tuple[Path, ArchiveStore]:
    store = _store(tmp_path)
    raw = "前後".encode()
    urls = tuple(
        f"https://shadowverse-evolve.com/errata/synthetic-{number}/"
        for number in (1, 2)
    )
    for number, url in enumerate(urls):
        _put(store, _resource(url, f"raw/{number}.html", raw, Kind.ERRATA), raw)
    batch = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, batch.batch_id)
    notices = []
    for current in sources.current_versions:
        source, _ = sources.metadata(
            current.source_version_id, parser_version="synthetic-v1"
        )
        pin = source.archive
        notices.append(
            Notice(
                source=Evidence(
                    id=source.id,
                    url=source.url,
                    sha256=source.sha256,
                    fetched_at=source.fetched_at,
                    etag=source.etag,
                    last_modified=source.last_modified,
                    parser_version=source.parser_version,
                    batch_id=pin.batch_id,
                    descriptor_sha256=pin.descriptor_sha256,
                    first_receipt_id=pin.first_receipt_id,
                ),
                region="jp",
                announced_on=None,
                effective_on=None,
                date_raw=None,
                reason=None,
                exchange_offered=None,
                changes=(),
                printings=(),
                blocks=(
                    Block(
                        ordinal=0,
                        context=(),
                        before="前",
                        after="後",
                        raw_before=Slice(start=0, end=3, sha256=digest(raw[:3])),
                        raw_after=Slice(start=3, end=6, sha256=digest(raw[3:])),
                    ),
                ),
                review_note="Synthetic byte evidence only",
            )
        )
    root = tmp_path / "inputs"
    directory = root / "errata"
    directory.mkdir(parents=True)
    pins = []
    for region in ("jp", "en"):
        document = Notices(format=1, notices=tuple(notices) if region == "jp" else ())
        content = document.model_dump_json().encode()
        name = f"{region}.json"
        (directory / name).write_bytes(content)
        pins.append(
            File(
                path="jp.json" if region == "jp" else "en.json",
                sha256=digest(content),
                bytes=len(content),
            )
        )
    (directory / "index.json").write_text(
        Index(format=1, files=tuple(pins)).model_dump_json()
    )
    return root, store


def test_verify_reads_sealed_sources_and_cli_reports_ranges(
    inputs: tuple[Path, ArchiveStore],
) -> None:
    root, store = inputs
    assert verify(root, store.root, store.store_id) == {"notices": 2, "ranges": 4}
    result = CliRunner().invoke(
        app, ["verify-errata", str(root), str(store.root), store.store_id]
    )
    assert result.exit_code == 0
    assert result.output.strip() == "Verified 2 notices and 4 byte ranges."


def test_verify_and_cli_reject_admitted_but_mismatched_source_evidence(
    inputs: tuple[Path, ArchiveStore],
) -> None:
    root, store = inputs
    path = root / "errata/jp.json"
    document = json.loads(path.read_bytes())
    document["notices"][0]["source"]["etag"] = '"changed"'
    raw = json.dumps(document).encode()
    path.write_bytes(raw)
    index = root / "errata/index.json"
    value = json.loads(index.read_bytes())
    value["files"][0].update(sha256=digest(raw), bytes=len(raw))
    index.write_text(json.dumps(value))
    with pytest.raises(ValueError, match=r"^Errata frozen source evidence differs$"):
        verify(root, store.root, store.store_id)
    result = CliRunner().invoke(
        app, ["verify-errata", str(root), str(store.root), store.store_id]
    )
    assert result.exit_code == 1
    assert result.output.strip() == "Frozen errata evidence verification failed."


def test_cli_rejects_unavailable_archive(inputs: tuple[Path, ArchiveStore]) -> None:
    root, store = inputs
    result = CliRunner().invoke(
        app, ["verify-errata", str(root), str(store.root / "absent"), store.store_id]
    )
    assert result.exit_code == 1
    assert result.output.strip() == "Frozen errata evidence verification failed."

"""Date precision is explicit, approved and preserved through the archive closure."""

import json
import sqlite3
from dataclasses import replace
from datetime import date, datetime
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Manifest, ManifestError
from sve_carddb.ingest.archive.source_archive import (
    ArchiveError,
    ArchiveStore,
    backup_batch,
    restore_backup,
    seal_batch,
)
from sve_carddb.ingest.archive.source_import.importer import prepare, register

from ..support.source_import_fixtures import (
    URLS,
    Inputs,
    copy_inputs,
    create_seed,
    index_rows,
    write_index,
)

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def seed(tmp_path_factory: pytest.TempPathFactory) -> Inputs:
    return create_seed(tmp_path_factory.mktemp("date-import-seed"))


@pytest.fixture
def inputs(seed: Inputs, tmp_path: Path) -> Inputs:
    inputs = copy_inputs(seed, tmp_path)
    rows = index_rows(inputs)
    rows[0].update(
        fetched_at=None,
        observed_on="2020-01-02",
        observed_precision="date",
        observed_timezone="Asia/Taipei",
        observed_basis="Synthetic acquisition day in local UTC+08:00; no instant retained.",
    )
    write_index(inputs, rows)
    selection = json.loads(inputs.selection.read_bytes())
    selection.update(
        source_import_format=2,
        date_precision_approval={
            "authority": "maintainer-approved-one-time",
            "basis": "Synthetic maintainer decision authorizes this exact one-time set.",
        },
    )
    inputs.selection.write_text(json.dumps(selection))
    return inputs


def test_date_and_instant_remain_distinct_through_reuse_and_restore(
    inputs: Inputs, tmp_path: Path
) -> None:
    plan = prepare(inputs.input_dir, inputs.selection)
    output = tmp_path / "isolated"
    assert register(plan, output, inputs.program)["state"] == "registered"
    database = output / "manifest/manifest.sqlite"
    original = database.read_bytes()
    assert register(plan, output, inputs.program)["state"] == "reused"
    assert database.read_bytes() == original
    store = ArchiveStore(
        output, database, output / "manifest/.lock", tmp_path / "archive", "date-import"
    )
    batch = seal_batch(store)
    backup_batch(
        store, tmp_path / "backup", batch.batch_id, require_separate_device=False
    )
    restore_backup(
        tmp_path / "backup", tmp_path / "restored", store.store_id, batch.batch_id
    )
    restored = tmp_path / "restored/batches" / batch.batch_id[7:] / "manifest.sqlite"
    with Manifest.open_snapshot(restored) as manifest:
        receipt = manifest.source_import_receipt()
        assert receipt is not None
        assert receipt.index_bytes == (inputs.input_dir / "index.jsonl").read_bytes()
        assert (
            receipt.content.date_precision_approval
            == plan.content.date_precision_approval
        )
        assert receipt.registered_at.tzinfo is not None
        resource = manifest.resources.get(URLS[0])
        assert resource is not None
        assert type(resource.first_fetched_at) is date
        assert (
            resource.first_fetched_at
            == resource.last_checked_at
            == resource.last_changed_at
            == date(2020, 1, 2)
        )
        instant = manifest.resources.get(URLS[1])
        assert instant is not None
        assert isinstance(instant.first_fetched_at, datetime)
    with sqlite3.connect(restored) as connection:
        row = connection.execute(
            "SELECT first_fetched_at,last_checked_at,last_changed_at FROM resource WHERE url=?",
            (URLS[0],),
        ).fetchone()
        assert row == ("2020-01-02",) * 3
    frozen = FrozenSources(tmp_path / "restored", store.store_id, batch.batch_id)
    version = next(
        item.source_version_id
        for item in frozen.current_versions
        if item.url == URLS[0]
    )
    with pytest.raises(ArchiveError, match="date-aware adoption reader"):
        frozen.metadata(version, parser_version="synthetic-date-rejection")


@pytest.mark.parametrize(
    "field",
    ["observed_on", "observed_precision", "observed_timezone", "observed_basis"],
)
def test_date_requires_all_precision_metadata(inputs: Inputs, field: str) -> None:
    rows = index_rows(inputs)
    del rows[0][field]
    write_index(inputs, rows)
    with pytest.raises(ValidationError):
        prepare(inputs.input_dir, inputs.selection)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("observed_on", "2020-02-30"),
        ("observed_on", "20200102"),
        ("observed_on", "2020-01-02T00:00:00Z"),
        ("observed_timezone", "Unknown/Zone"),
        ("observed_precision", "instant"),
        ("observed_basis", "   "),
        ("fetched_at", "2020-01-02T00:00:00Z"),
    ],
)
def test_invalid_or_mixed_precision_is_rejected(
    inputs: Inputs, field: str, value: str
) -> None:
    rows = index_rows(inputs)
    rows[0][field] = value
    write_index(inputs, rows)
    with pytest.raises(ValidationError):
        prepare(inputs.input_dir, inputs.selection)


@pytest.mark.parametrize(
    "change", ["legacy", "missing", "wrong-authority", "blank", "unneeded"]
)
def test_date_requires_one_time_approval(inputs: Inputs, change: str) -> None:
    selection = json.loads(inputs.selection.read_bytes())
    if change == "legacy":
        selection["source_import_format"] = 1
        del selection["date_precision_approval"]
    elif change == "missing":
        del selection["date_precision_approval"]
    elif change == "wrong-authority":
        selection["date_precision_approval"]["authority"] = "crawler"
    elif change == "blank":
        selection["date_precision_approval"]["basis"] = " "
    else:
        rows = index_rows(inputs)
        for key in tuple(rows[0]):
            if key.startswith("observed_"):
                del rows[0][key]
        rows[0]["fetched_at"] = "2020-01-02T03:04:05+00:00"
        write_index(inputs, rows)
    inputs.selection.write_text(json.dumps(selection))
    with pytest.raises(ValueError, match="approval"):
        prepare(inputs.input_dir, inputs.selection)


def test_live_resource_store_rejects_dates_on_write_and_read(
    inputs: Inputs, tmp_path: Path
) -> None:
    plan = prepare(inputs.input_dir, inputs.selection)
    output = tmp_path / "isolated"
    register(plan, output, inputs.program)
    live = tmp_path / "live.sqlite"
    with (
        Manifest.open_snapshot(output / "manifest/manifest.sqlite") as imported,
        Manifest.open(live) as manifest,
    ):
        dated = imported.resources.get(URLS[0])
        instant = imported.resources.get(URLS[1])
        assert dated is not None
        assert instant is not None
        with (
            pytest.raises(ManifestError, match="full observation instants"),
            manifest.transaction(),
        ):
            manifest.resources.put(dated)
        with manifest.transaction():
            manifest.resources.put(instant)
    with sqlite3.connect(live) as connection:
        connection.execute("UPDATE resource SET last_changed_at='2020-01-02'")
    connection.close()
    with Manifest.open_snapshot(live) as manifest, pytest.raises(ManifestError):
        manifest.resources.get(URLS[1])


@pytest.mark.parametrize("mutation", ["midnight", "approval"])
def test_date_receipt_and_resources_cannot_be_reinterpreted(
    inputs: Inputs, tmp_path: Path, mutation: str
) -> None:
    output = tmp_path / "isolated"
    register(prepare(inputs.input_dir, inputs.selection), output, inputs.program)
    database = output / "manifest/manifest.sqlite"
    with sqlite3.connect(database) as connection:
        if mutation == "midnight":
            connection.execute(
                "UPDATE resource SET last_changed_at='2020-01-02T00:00:00+00:00' WHERE url=?",
                (URLS[0],),
            )
        else:
            content = json.loads(
                connection.execute(
                    "SELECT content FROM source_import_receipt"
                ).fetchone()[0]
            )
            del content["date_precision_approval"]
            connection.execute(
                "UPDATE source_import_receipt SET content=?",
                (json.dumps(content).encode(),),
            )
    with pytest.raises(ManifestError):
        Manifest.open_snapshot(database)


@pytest.mark.parametrize(
    "invalid", ["2020-01-02T03:04:05", "2020-01-02T03:04:05+01:00"]
)
def test_live_resources_require_utc_instants(
    inputs: Inputs, tmp_path: Path, invalid: str
) -> None:
    output = tmp_path / "isolated"
    register(prepare(inputs.input_dir, inputs.selection), output, inputs.program)
    live = tmp_path / "live.sqlite"
    with (
        Manifest.open_snapshot(output / "manifest/manifest.sqlite") as imported,
        Manifest.open(live) as manifest,
    ):
        resource = imported.resources.get(URLS[1])
        assert resource is not None
        with (
            pytest.raises(ManifestError, match="full observation instants"),
            manifest.transaction(),
        ):
            manifest.resources.put(
                replace(resource, first_fetched_at=datetime.fromisoformat(invalid))
            )
        with manifest.transaction():
            manifest.resources.put(resource)
    with sqlite3.connect(live) as connection:
        connection.execute("UPDATE resource SET first_fetched_at=?", (invalid,))
    connection.close()
    with (
        Manifest.open_snapshot(live) as manifest,
        pytest.raises(ManifestError, match="must be UTC"),
    ):
        manifest.resources.get(URLS[1])

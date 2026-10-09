"""Shared-fixture isolation and validation-preserving load regressions."""

from concurrent.futures import ThreadPoolExecutor
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.core.json import digest
from sve_carddb.domains.registry import snapshot, storage
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.registry.storage import Entry

from ...support import official_registry_fixtures as shared
from ...support.registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose synthetic pytest fixture
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose dependency of the synthetic registry fixture

if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

    from sve_carddb.domains.registry.snapshot import RegistrySnapshot


def test_legacy_fixture_cannot_pollute_later_consumers() -> None:
    source = (
        Entry(
            kind="printing",
            owner="BP01",
            data={"source_face_map": [{"face_id": "original"}]},
        ),
    )
    first = shared.detached_entries(source)
    maps = first[0].data["source_face_map"]
    assert isinstance(maps, list)
    assert isinstance(maps[0], dict)
    maps[0]["face_id"] = "polluted"
    first[0].owner = "changed"
    first.clear()
    second = shared.detached_entries(source)
    assert second[0].owner == "BP01"
    assert second[0].data == {"source_face_map": [{"face_id": "original"}]}
    assert source[0].data == second[0].data


def test_shared_snapshot_rejects_nested_pollution(
    official_snapshot: RegistrySnapshot,
) -> None:
    printing = next(
        record
        for record in official_snapshot.records.values()
        if isinstance(record.data, PrintingData)
    )
    assert isinstance(printing.data, PrintingData)
    observation = printing.data.observation
    before = observation.card_no
    with pytest.raises(ValidationError, match="frozen"):
        observation.card_no = "polluted"  # type: ignore[misc]  # exercise runtime frozen validation
    assert printing.data.observation.card_no == before
    with pytest.raises(TypeError):
        official_snapshot.records["polluted"] = printing  # type: ignore[index]  # exercise runtime mapping immutability
    first = printing.entry()
    first.data.clear()
    assert printing.entry().data


def test_workers_share_one_source_load_and_exact_snapshot(
    registry_root: Path, tmp_path: Path, mocker: MockerFixture
) -> None:
    load = mocker.spy(shared, "load_registry")
    cache = tmp_path / "shared.json"
    with ThreadPoolExecutor(max_workers=2) as executor:
        futures = [
            executor.submit(shared.load_shared_registry, registry_root, cache)
            for _ in range(2)
        ]
        first, second = [future.result() for future in futures]
    assert load.call_count == 1
    assert first.files == second.files
    assert first.records == second.records
    assert first is not second
    assert cache.read_bytes()
    record = next(iter(second.records.values()))
    with pytest.raises(TypeError):
        second.records["polluted"] = record  # type: ignore[index]  # exercise restored mapping immutability
    printing = next(
        record.data
        for record in second.records.values()
        if isinstance(record.data, PrintingData)
    )
    assert isinstance(printing, PrintingData)
    with pytest.raises(ValidationError, match="frozen"):
        printing.observation.card_no = "polluted"  # type: ignore[misc]  # exercise restored nested immutability


def test_source_write_during_initial_shared_load_is_detected(
    registry_root: Path, tmp_path: Path, mocker: MockerFixture
) -> None:
    original = snapshot.load_registry

    def damaging_load(root: Path) -> RegistrySnapshot:
        result = original(root)
        path = root / "ids/index.yaml"
        path.write_bytes(path.read_bytes() + b"\n")
        return result

    mocker.patch.object(shared, "load_registry", side_effect=damaging_load)
    cache = tmp_path / "shared.json"
    with pytest.raises(AssertionError, match="changed authored files"):
        shared.load_shared_registry(registry_root, cache)
    assert not cache.exists()


def test_canonical_input_is_encoded_once_per_file(
    registry_root: Path, mocker: MockerFixture
) -> None:
    canonical = mocker.spy(storage, "canonical")
    files = storage.read_registry_files(registry_root)
    assert canonical.call_count == len(files.shards) + 1
    for shard in files.shards:
        assert shard.content_hash == digest(
            canonical(storage.read_yaml(registry_root / shard.path))
        )


def test_snapshot_validates_original_checked_entries_without_json_reparse(
    registry_root: Path, mocker: MockerFixture
) -> None:
    parse = mocker.spy(Entry, "model_validate_json")
    validation = mocker.spy(snapshot, "validate")
    result = snapshot.load_registry(registry_root)
    assert parse.call_count == 0
    validation.assert_called_once()
    checked = validation.call_args.args[0]
    assert [entry.record_key for entry in checked] == list(result.records)
    assert all(entry.data for entry in checked)

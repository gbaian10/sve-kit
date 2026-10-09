"""Exercise sealed raw input, source failures and report-only CLI through real readers."""

import sys
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value, parse
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.registry.english_inventory import main, scan
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import ArchiveError, Scope, seal_batch
from sve_carddb.parse.pages import official_en, official_jp

from ...ingest.test_source_archive import _put, _resource, _store
from ...support.registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose synthetic fixture
from ..text_observations.test_effect_presence import page
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose synthetic fixture dependency

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.regions import Region
    from sve_carddb.ingest.archive.source_archive import ArchiveStore


def sealed(
    store: ArchiveStore, region: Region, *, raw: bytes | None = None
) -> FrozenSources:
    content = (
        raw
        if raw is not None
        else page(region, '<div class="detail">Invented effect.</div>', double=True)
    )
    url = (official_jp if region == "jp" else official_en).card_url("SYN-01")
    resource = replace(
        _resource(url, f"raw/{region}.html", content, Kind.CARD),
        region=SourceRegion(region),
    )
    _put(store, resource, content)
    result = seal_batch(store, scope=(Scope(provider=region, kind="card"),))
    return FrozenSources(store.root, store.store_id, result.batch_id)


@pytest.mark.parametrize("region", ["jp", "en"])
def test_replays_all_faces_and_records_actual_batch_versions(
    tmp_path: Path, region: Region
) -> None:
    source = sealed(_store(tmp_path), region)
    before = {
        path: path.read_bytes() for path in source.root.rglob("*") if path.is_file()
    }
    result = scan(source, region)
    assert not result.failures
    assert len(result.cards["SYN-01"].faces) == 2
    proof = result.sources["SYN-01"]
    assert proof["batch_id"] == source.batch_id
    assert proof["source_version_id"] == source.inventory.current[0].source_version_id
    assert result.pin["source_versions"] == [proof["source_version_id"]]
    assert proof["raw_sha256"] is not None
    assert before == {path: path.read_bytes() for path in before}
    with pytest.raises(ValueError, match="exclusively"):
        scan(source, "jp" if region == "en" else "en")


def test_projection_failure_is_hash_only_and_does_not_disappear(tmp_path: Path) -> None:
    result = scan(sealed(_store(tmp_path), "en", raw=b"Synthetic malformed page"), "en")
    assert not result.cards
    assert result.failures[0]["reason"] == "source_projection_failed"
    assert b"Synthetic malformed page" not in canonical(result.failures[0])


def test_modified_frozen_raw_cannot_be_classified(tmp_path: Path) -> None:
    source = sealed(_store(tmp_path), "en")
    (source.root / source.inventory.entries[0].blob.path).write_bytes(
        b"Changed synthetic page"
    )
    with pytest.raises(ArchiveError, match="content hash mismatch"):
        scan(source, "en")


def test_cli_writes_a_private_report_and_keeps_sources_read_only(
    tmp_path: Path,
    registry_root: Path,
    monkeypatch: pytest.MonkeyPatch,
    capsys: pytest.CaptureFixture[str],
) -> None:
    store = _store(tmp_path / "sources")
    jp, en = sealed(store, "jp"), sealed(store, "en")
    output = tmp_path.parent / (tmp_path.name + ".json")
    before = {
        path: path.read_bytes()
        for root in (store.root, registry_root)
        for path in root.rglob("*")
        if path.is_file()
    }
    argv = [
        "inventory",
        "--archive",
        str(store.root),
        "--store-id",
        store.store_id,
        "--jp-batch",
        jp.batch_id,
        "--en-batch",
        en.batch_id,
        "--authored",
        str(registry_root),
        "--program-revision",
        "synthetic-revision",
        "--output",
        str(output),
    ]
    monkeypatch.setattr(sys, "argv", argv)
    main()
    report = object_value(parse(output.read_bytes()))
    assert report["program_revision"] == "synthetic-revision"
    assert report["program_sha256"] is not None
    assert report["counts"] == {"has_jp": 0, "confirmed_no_jp": 0, "unresolved": 3}
    assert "Invented effect." not in capsys.readouterr().out
    assert b"Invented effect." not in output.read_bytes()
    assert before == {path: path.read_bytes() for path in before}
    monkeypatch.setattr(sys, "argv", [*argv[:-1], str(store.root / "overwrite.json")])
    with pytest.raises(ValueError, match="outside"):
        main()

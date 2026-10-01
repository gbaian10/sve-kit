"""Artifact loss, re-signed tampering and publication failures cannot pass closure checks."""

import hashlib
import json
import shutil
import sqlite3
from contextlib import closing
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_db.database import install_functions, open_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import SourceUse, input_record, insert_raw_sources
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind, Manifest
from sve_carddb.source_archive import ArchiveError, seal_batch

from .registry_preview_fixtures import BUILD
from .test_source_archive import NOW, _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import InputRecord


@pytest.fixture(scope="module")
def sealed_uses_template(
    tmp_path_factory: pytest.TempPathFactory,
) -> tuple[Path, tuple[SourceUse, ...]]:
    store = _store(tmp_path_factory.mktemp("sealed-uses-template") / "frozen")
    raw = b"<html>Synthetic card and product</html>"
    resource = replace(
        _resource("https://example.invalid/card", "raw/card.html", raw, Kind.CARD),
        first_fetched_at=NOW.replace(day=27),
        etag="first-etag",
    )
    _put(store, resource, raw)
    seal_batch(store)
    _put(
        store,
        replace(resource, etag="later-etag", last_checked_at=NOW.replace(day=30)),
        raw,
    )
    sealed = seal_batch(store)
    sources = FrozenSources(store.root, store.store_id, sealed.batch_id)
    version = sources.inventory.current[0].source_version_id
    identity, _, _ = sources.read(version, parser_version="identity-v1")
    product, _, _ = sources.read(version, parser_version="product-v1")
    return store.root, (
        SourceUse(source=identity, usage="registry_observation", locator="card page"),
        SourceUse(
            source=product, usage="product_observation", locator="product block 0"
        ),
    )


@pytest.fixture
def sealed_uses(
    tmp_path: Path, sealed_uses_template: tuple[Path, tuple[SourceUse, ...]]
) -> tuple[Path, tuple[SourceUse, ...]]:
    store, uses = sealed_uses_template
    destination = tmp_path / "frozen/archive"
    shutil.copytree(store, destination)
    return destination, uses


def publish(
    root: Path,
    sealed_uses: tuple[Path, tuple[SourceUse, ...]],
    *,
    failure: str | None = None,
) -> InputRecord:
    store, expected = sealed_uses

    def populate(db: Database) -> InputRecord:
        insert_raw_sources(db, (use.source for use in expected))
        if failure == "body":
            raise ValueError("Synthetic late failure")
        if failure == "commit":
            db.insert(
                "card_int_id",
                {
                    "printing_id": "missing",
                    "int_id": 20001,
                    "allocated_at": "2026-09-30",
                },
            )
        return input_record(BUILD, expected[:-1] if failure == "closure" else expected)

    return publish_bundle(
        compile_build(),
        root,
        BUILD,
        expected,
        populate,
        {"synthetic": True},
        stores={"test-store": store},
    )


def resign(root: Path) -> None:
    """Use an independent stdlib signer so mutations reach semantic validation."""
    seal = json.loads((root / "seal.json").read_bytes())
    for name, key in (
        ("build.sqlite", "database_sha256"),
        ("inputs.json", "inputs_sha256"),
        ("report.json", "report_sha256"),
    ):
        seal[key] = "sha256:" + hashlib.sha256((root / name).read_bytes()).hexdigest()
    (root / "seal.json").write_text(
        json.dumps(seal, sort_keys=True, separators=(",", ":"))
    )


def test_complete_bundle_rechecks_receipts_archives_and_closed_database(
    tmp_path: Path,
    sealed_uses: tuple[Path, tuple[SourceUse, ...]],
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    root = tmp_path / "bundle"
    store, expected = sealed_uses
    record = publish(root, sealed_uses)
    before = {p: p.read_bytes() for p in root.iterdir()}

    def forbidden(*_args: object, **_kwargs: object) -> None:
        raise AssertionError("Live manifest must not be opened")

    monkeypatch.setattr(Manifest, "open", forbidden)
    assert (
        verify_bundle(
            compile_build(), root, BUILD, expected, stores={"test-store": store}
        )
        == record
    )
    assert before == {p: p.read_bytes() for p in root.iterdir()}
    assert record.uses[0].source.etag == "first-etag"
    assert record.uses[0].source.fetched_at == "2026-09-29T00:00:00Z"
    source = record.uses[0].source
    frozen = FrozenSources(store, "test-store", source.archive.batch_id)
    entry = frozen.entries[source.id]
    assert source.archive.first_receipt_id != entry.receipt_id
    assert "Synthetic card and product" not in (root / "inputs.json").read_text()


@pytest.mark.parametrize(
    "name", ["inputs.json", "report.json", "build.sqlite", "seal.json"]
)
@pytest.mark.parametrize("case", ["missing", "corrupt", "symlink"])
def test_each_artifact_is_required_and_hashed(
    tmp_path: Path,
    sealed_uses: tuple[Path, tuple[SourceUse, ...]],
    name: str,
    case: str,
) -> None:
    root = tmp_path / "bundle"
    store, expected = sealed_uses
    publish(root, sealed_uses)
    target = root / name
    if case == "corrupt":
        target.write_bytes(target.read_bytes() + b" ")
    else:
        target.unlink()
        if case == "symlink":
            outside = tmp_path / "outside"
            outside.write_bytes(b"synthetic")
            target.symlink_to(outside)
    with pytest.raises(
        ValueError, match=r"closure mismatch|hash mismatch|Unsafe|Noncanonical"
    ):
        verify_bundle(
            compile_build(), root, BUILD, expected, stores={"test-store": store}
        )


@pytest.mark.parametrize(
    "name", ["unexpected.txt", "build.sqlite-journal", "unexpected-directory"]
)
def test_bundle_rejects_every_unlisted_directory_entry(
    tmp_path: Path,
    sealed_uses: tuple[Path, tuple[SourceUse, ...]],
    name: str,
) -> None:
    root = tmp_path / "bundle"
    store, expected = sealed_uses
    publish(root, sealed_uses)
    extra = root / name
    if name == "unexpected-directory":
        extra.mkdir()
    else:
        extra.write_bytes(b"synthetic extra entry")
    with pytest.raises(ValueError, match="Build bundle file closure mismatch"):
        verify_bundle(
            compile_build(), root, BUILD, expected, stores={"test-store": store}
        )


@pytest.mark.parametrize(
    "case",
    [
        "missing_use",
        "wrong_parser",
        "wrong_descriptor",
        "wrong_receipt",
        "wrong_configuration",
        "raw_parser",
        "wrong_db_hash",
        "wrong_report_reference",
    ],
)
def test_resigned_tampering_reaches_independent_closure_validation(
    tmp_path: Path, sealed_uses: tuple[Path, tuple[SourceUse, ...]], case: str
) -> None:
    root = tmp_path / "bundle"
    store, expected = sealed_uses
    publish(root, sealed_uses)
    if case in {"raw_parser", "wrong_db_hash"}:
        with closing(sqlite3.connect(root / "build.sqlite")) as connection, connection:
            install_functions(connection, compile_build())
            connection.execute(
                "UPDATE source_record SET parser_version = 'wrong'"
                if case == "raw_parser"
                else "UPDATE source_record SET sha256 = ?",
                () if case == "raw_parser" else ("sha256:" + "0" * 64,),
            )
    elif case == "wrong_report_reference":
        report = json.loads((root / "report.json").read_bytes())
        report["build_inputs_hash"] = "sha256:" + "0" * 64
        (root / "report.json").write_text(
            json.dumps(report, sort_keys=True, separators=(",", ":"))
        )
    else:
        data = json.loads((root / "inputs.json").read_bytes())
        if case == "missing_use":
            data["uses"] = data["uses"][:-1]
        elif case == "wrong_parser":
            data["uses"][0]["source"]["parser_version"] = "wrong"
        elif case == "wrong_configuration":
            data["context"]["configuration"] = '{"synthetic":false}'
        else:
            field = (
                "descriptor_sha256"
                if case == "wrong_descriptor"
                else "first_receipt_id"
            )
            data["uses"][0]["source"]["archive"][field] = "sha256:" + "0" * 64
        data["uses"].sort(
            key=lambda item: json.dumps(item, sort_keys=True, separators=(",", ":"))
        )
        content = json.dumps(data, sort_keys=True, separators=(",", ":")).encode()
        (root / "inputs.json").write_bytes(content)
        report = json.loads((root / "report.json").read_bytes())
        report["build_inputs_hash"] = "sha256:" + hashlib.sha256(content).hexdigest()
        (root / "report.json").write_text(
            json.dumps(report, sort_keys=True, separators=(",", ":"))
        )
    resign(root)
    with pytest.raises(
        ValueError, match=r"use closure|source metadata|report input reference"
    ):
        verify_bundle(
            compile_build(), root, BUILD, expected, stores={"test-store": store}
        )


@pytest.mark.parametrize("kind", ["raw", "descriptors", "receipts", "unconfigured"])
def test_offline_verification_cannot_replace_missing_archive_evidence(
    tmp_path: Path, sealed_uses: tuple[Path, tuple[SourceUse, ...]], kind: str
) -> None:
    root = tmp_path / "bundle"
    store, expected = sealed_uses
    publish(root, sealed_uses)
    if kind != "unconfigured":
        next((store / kind).rglob("*.raw" if kind == "raw" else "*.json")).unlink()
    with pytest.raises((ValueError, ArchiveError)):
        verify_bundle(
            compile_build(),
            root,
            BUILD,
            expected,
            stores={} if kind == "unconfigured" else {"test-store": store},
        )


@pytest.mark.parametrize("case", ["body", "commit", "closure", "archive", "report"])
def test_failed_build_never_publishes_a_partial_bundle(
    tmp_path: Path, sealed_uses: tuple[Path, tuple[SourceUse, ...]], case: str
) -> None:
    root = tmp_path / "bundle"
    if case == "archive":
        next((sealed_uses[0] / "raw").rglob("*.raw")).unlink()
    if case == "report":
        with pytest.raises(ValueError, match="Floating point JSON is forbidden"):
            publish_bundle(
                compile_build(),
                root,
                BUILD,
                (),
                lambda _db: input_record(BUILD, ()),
                float("nan"),
                stores={},
            )
    else:
        with pytest.raises((ValueError, sqlite3.IntegrityError, ArchiveError)):
            publish(root, sealed_uses, failure=case)
    assert not root.exists()
    assert not list(tmp_path.glob(".build-bundle-*"))


def test_existing_bundle_and_rebuild_destination_survive_failure(
    tmp_path: Path, sealed_uses: tuple[Path, tuple[SourceUse, ...]]
) -> None:
    root = tmp_path / "bundle"
    publish(root, sealed_uses)
    before = {p: p.read_bytes() for p in root.iterdir()}
    with pytest.raises(FileExistsError):
        publish(root, sealed_uses, failure="body")
    assert before == {p: p.read_bytes() for p in root.iterdir()}


def test_source_returning_to_old_bytes_reuses_first_version_metadata(
    tmp_path: Path,
) -> None:
    store = _store(tmp_path / "frozen")
    url = "https://example.invalid/card"
    original = b"<html>Synthetic A</html>"
    changed = b"<html>Synthetic B</html>"
    _put(
        store,
        replace(_resource(url, "raw/card.html", original, Kind.CARD), etag="original"),
        original,
    )
    first = seal_batch(store)
    version = first.inventory.current[0].source_version_id
    before, _, _ = FrozenSources(store.root, store.store_id, first.batch_id).read(
        version, parser_version="parser-v1"
    )
    _put(
        store,
        replace(_resource(url, "raw/card.html", changed, Kind.CARD), etag="changed"),
        changed,
    )
    second = seal_batch(store)
    assert second.inventory.current[0].source_version_id != version
    _put(
        store,
        replace(
            _resource(url, "raw/card.html", original, Kind.CARD),
            etag="returned",
            last_changed_at=NOW.replace(day=30),
        ),
        original,
    )
    third = seal_batch(store)
    after, _, _ = FrozenSources(store.root, store.store_id, third.batch_id).read(
        third.inventory.current[0].source_version_id, parser_version="parser-v2"
    )
    assert after.id == before.id
    assert after.values() == before.values()
    assert after.archive.first_receipt_id == before.archive.first_receipt_id
    assert after.archive.batch_id != before.archive.batch_id


@pytest.mark.parametrize("suffix", ["-wal", "-shm", "-journal"])
def test_offline_database_reader_rejects_sidecars(
    tmp_path: Path, sealed_uses: tuple[Path, tuple[SourceUse, ...]], suffix: str
) -> None:
    root = tmp_path / "bundle"
    publish(root, sealed_uses)
    database = root / "build.sqlite"
    database.with_name(database.name + suffix).write_bytes(b"synthetic sidecar")
    with (
        pytest.raises(ValueError, match="sidecars"),
        open_database(compile_build(), database),
    ):
        pass


def test_offline_database_reader_cannot_write(
    tmp_path: Path, sealed_uses: tuple[Path, tuple[SourceUse, ...]]
) -> None:
    root = tmp_path / "bundle"
    publish(root, sealed_uses)
    path = root / "build.sqlite"
    before = path.read_bytes()
    with open_database(compile_build(), path) as db:
        db.verify()
        with (
            pytest.raises(sqlite3.OperationalError, match="readonly"),
            db.transaction(),
        ):
            db.update(
                "source_record",
                {"id": sealed_uses[1][0].source.id},
                {"etag": "changed"},
            )
    assert path.read_bytes() == before


@pytest.mark.parametrize("filename", ["inputs.json", "seal.json"])
def test_boolean_format_is_rejected_even_after_resigning(
    tmp_path: Path, sealed_uses: tuple[Path, tuple[SourceUse, ...]], filename: str
) -> None:
    root = tmp_path / "bundle"
    store, expected = sealed_uses
    publish(root, sealed_uses)
    data = json.loads((root / filename).read_bytes())
    key = "input_format" if filename == "inputs.json" else "bundle_format"
    data[key] = True
    content = json.dumps(data, sort_keys=True, separators=(",", ":")).encode()
    (root / filename).write_bytes(content)
    if filename == "inputs.json":
        report = json.loads((root / "report.json").read_bytes())
        report["build_inputs_hash"] = "sha256:" + hashlib.sha256(content).hexdigest()
        (root / "report.json").write_text(
            json.dumps(report, sort_keys=True, separators=(",", ":"))
        )
        resign(root)
    with pytest.raises(ValueError, match="integer 1"):
        verify_bundle(
            compile_build(), root, BUILD, expected, stores={"test-store": store}
        )


@pytest.mark.parametrize(
    "field", ["first_receipt_id", "descriptor_sha256", "etag", "url"]
)
def test_self_consistent_false_claim_still_fails_archived_metadata(
    tmp_path: Path,
    sealed_uses: tuple[Path, tuple[SourceUse, ...]],
    field: str,
) -> None:
    store, uses = sealed_uses
    source = uses[0].source
    if field in {"first_receipt_id", "descriptor_sha256"}:
        source = source.model_copy(
            update={
                "archive": source.archive.model_copy(
                    update={field: "sha256:" + "0" * 64}
                )
            }
        )
    else:
        source = source.model_copy(
            update={
                field: "https://example.invalid/wrong" if field == "url" else "wrong"
            }
        )
    expected = (uses[0].model_copy(update={"source": source}),)
    root = tmp_path / "bundle"

    def populate(db: Database) -> InputRecord:
        insert_raw_sources(db, (source,))
        return input_record(BUILD, expected)

    with pytest.raises(ValueError, match="archive provenance mismatch"):
        publish_bundle(
            compile_build(),
            root,
            BUILD,
            expected,
            populate,
            {},
            stores={"test-store": store},
        )
    assert not root.exists()


def test_noncanonical_inputs_are_rejected_after_hashes_are_resigned(
    tmp_path: Path,
    sealed_uses: tuple[Path, tuple[SourceUse, ...]],
) -> None:
    root = tmp_path / "bundle"
    store, expected = sealed_uses
    publish(root, sealed_uses)
    data = json.loads((root / "inputs.json").read_bytes())
    content = json.dumps(data, indent=2).encode()
    (root / "inputs.json").write_bytes(content)
    report = json.loads((root / "report.json").read_bytes())
    report["build_inputs_hash"] = "sha256:" + hashlib.sha256(content).hexdigest()
    (root / "report.json").write_text(
        json.dumps(report, sort_keys=True, separators=(",", ":"))
    )
    resign(root)
    with pytest.raises(ValueError, match="Noncanonical build input record"):
        verify_bundle(
            compile_build(), root, BUILD, expected, stores={"test-store": store}
        )

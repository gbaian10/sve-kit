"""Offline registration and sealed v2 provenance, without official source content."""

import hashlib
import json
import shutil
import sqlite3
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from pathlib import Path
from pydantic import ValidationError
from typer.testing import CliRunner

from sve_carddb import manifest as manifest_module
from sve_carddb import source_archive as archive
from sve_carddb.cli import app
from sve_carddb.core.json import canonical, digest
from sve_carddb.manifest import Manifest, ManifestError
from sve_carddb.source_archive import (
    ArchiveError,
    ArchiveStore,
    backup_batch,
    restore_backup,
    seal_batch,
    verify_batch,
)
from sve_carddb.source_import import importer
from sve_carddb.source_import.importer import SourceImportError, prepare, register

from .source_import_fixtures import (
    HTML,
    PDF,
    URLS,
    Inputs,
    copy_inputs,
    create_seed,
    git,
    index_rows,
    write_index,
)


@pytest.fixture(scope="module")
def seed(tmp_path_factory: pytest.TempPathFactory) -> Inputs:
    return create_seed(tmp_path_factory.mktemp("source-import-seed"))


@pytest.fixture
def inputs(seed: Inputs, tmp_path: Path) -> Inputs:
    return copy_inputs(seed, tmp_path)


def _register(inputs: Inputs, tmp_path: Path) -> Path:
    output = tmp_path / "isolated"
    register(prepare(inputs.input_dir, inputs.selection), output, inputs.program)
    return output


def test_exact_index_receipt_and_no_http_events(inputs: Inputs, tmp_path: Path) -> None:
    output = _register(inputs, tmp_path)
    with Manifest.open_snapshot(output / "manifest/manifest.sqlite") as manifest:
        receipt = manifest.source_import_receipt()
        assert receipt is not None
        assert receipt.index_bytes == (inputs.input_dir / "index.jsonl").read_bytes()
        assert receipt.receipt_id == prepare(inputs.input_dir, inputs.selection).id
        assert manifest.schema_version == 2
        resources = tuple(manifest.resources.all())
        assert len(resources) == 3
        for resource in resources:
            assert resource.raw_bytes == resource.stored_bytes
            assert (
                resource.first_fetched_at
                == resource.last_changed_at
                == resource.last_checked_at
            )
            assert resource.first_fetched_at.isoformat() == "2020-01-02T03:04:05+00:00"
            assert resource.archived_at is None
        pdf = next(item for item in resources if item.url == URLS[2])
        assert pdf.region.value == "en"
        assert pdf.path.parts[:3] == ("raw", "en", "rules")
        assert (output / pdf.path).read_bytes() == PDF
        html = [item for item in resources if item.kind.value != "rules"]
        assert html[0].sha256 == html[1].sha256
        assert html[0].path != html[1].path
        assert manifest.historical_raw_hashes() == []


def test_reuse_preserves_database_and_registration_time(
    inputs: Inputs, tmp_path: Path
) -> None:
    output = _register(inputs, tmp_path)
    db = output / "manifest/manifest.sqlite"
    before = db.read_bytes()
    result = register(
        prepare(inputs.input_dir, inputs.selection), output, inputs.program
    )
    assert result["state"] == "reused"
    assert db.read_bytes() == before
    with pytest.raises(ManifestError, match="schema 2"):
        Manifest.open(db)
    assert db.read_bytes() == before
    assert not db.with_name("manifest.sqlite-wal").exists()


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("status", 404),
        ("sha256", "A" * 64),
        ("bytes", 0),
        ("bytes", True),
        ("fetched_at", "2020-01-02"),
        ("fetched_at", "2020-01-02T03:04:05"),
        ("fetched_at", "2020-01-02T03:04:05+01:00"),
        ("content_type", "image/png"),
        ("chain", []),
        ("url", "http://shadowverse-evolve.com/card_limit/"),
        ("url", "https://example.invalid/card_limit/"),
        ("final_url", URLS[1]),
    ],
)
def test_invalid_observation_before_output(
    inputs: Inputs, tmp_path: Path, field: str, value: object
) -> None:
    rows = index_rows(inputs)
    rows[0][field] = value
    write_index(inputs, rows)
    with pytest.raises(ValidationError):
        prepare(inputs.input_dir, inputs.selection)
    assert not (tmp_path / "isolated").exists()


def test_duplicate_requested_url_and_unknown_metadata(inputs: Inputs) -> None:
    rows = index_rows(inputs)
    write_index(inputs, [*rows, rows[0]])
    with pytest.raises(
        ValueError, match=r"^Source index must be nonempty with unique requested URLs$"
    ):
        prepare(inputs.input_dir, inputs.selection)
    rows[0]["unobserved"] = "not acquisition evidence"
    write_index(inputs, rows)
    with pytest.raises(ValidationError):
        prepare(inputs.input_dir, inputs.selection)


def test_raw_hash_and_size_counterexamples(inputs: Inputs) -> None:
    raw_path = inputs.input_dir / "raw" / hashlib.sha256(HTML).hexdigest()
    raw_path.write_bytes(HTML + b"changed")
    with pytest.raises(
        SourceImportError,
        match=r"^Source import raw hash or bytes differ from the index$",
    ):
        prepare(inputs.input_dir, inputs.selection)


@pytest.mark.parametrize(
    "raw",
    [
        b"not html",
        b"<html><head><title>404</title></head><body>Missing.</body></html>",
        b"<html><head><title></title></head><body>Text.</body></html>",
    ],
    ids=("plain", "404", "empty-title"),
)
def test_invalid_html_even_with_valid_hash(inputs: Inputs, raw: bytes) -> None:
    rows = index_rows(inputs)
    rows[0].update(sha256=hashlib.sha256(raw).hexdigest(), bytes=len(raw))
    (inputs.input_dir / "raw" / str(rows[0]["sha256"])).write_bytes(raw)
    write_index(inputs, rows)
    with pytest.raises(
        SourceImportError,
        match=r"^Source import (raw is not a nonempty HTML document|HTML is an error page)$",
    ):
        prepare(inputs.input_dir, inputs.selection)


def test_invalid_pdf_with_valid_hash(inputs: Inputs) -> None:
    raw = b"%PDF-1.7\nIncomplete container"
    (inputs.input_dir / "raw" / hashlib.sha256(PDF).hexdigest()).unlink()
    rows = index_rows(inputs)
    rows[2].update(sha256=hashlib.sha256(raw).hexdigest(), bytes=len(raw))
    (inputs.input_dir / "raw" / str(rows[2]["sha256"])).write_bytes(raw)
    write_index(inputs, rows)
    with pytest.raises(
        SourceImportError, match=r"^Source import raw is not a complete PDF container$"
    ):
        prepare(inputs.input_dir, inputs.selection)


@pytest.mark.parametrize(
    "change", ["missing", "region", "kind", "format", "dependency", "program"]
)
def test_selection_and_program_counterexamples(
    inputs: Inputs, tmp_path: Path, change: str
) -> None:
    value = json.loads(inputs.selection.read_bytes())
    if change == "missing":
        value["sources"].pop()
    elif change == "region":
        value["sources"][0]["provider"] = "en"
    elif change == "kind":
        value["sources"][2]["kind"] = "news"
    elif change == "format":
        value["source_import_format"] = True
    elif change == "dependency":
        value["dependencies"][0]["sha256"] = "sha256:" + "0" * 64
    else:
        value["program_revision"] = "0" * 40
    inputs.selection.write_text(json.dumps(value))
    messages = {
        "missing": r"^Source selection differs from the complete index$",
        "region": r"^HTML source import URL does not match its explicit purpose$",
        "kind": r"^PDF source import must be explicitly classified as rules$",
        "format": "Source import format must be integer 1",
        "dependency": r"^Source import program dependency differs from executing code$",
        "program": r"^Source import program revision differs from the repository HEAD$",
    }
    with pytest.raises(ValueError, match=messages[change]):
        register(
            prepare(inputs.input_dir, inputs.selection),
            tmp_path / "isolated",
            inputs.program,
        )
    assert not (tmp_path / "isolated").exists()


def test_reject_input_and_live_overlap_before_manifest(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan = prepare(inputs.input_dir, inputs.selection)
    with pytest.raises(
        SourceImportError,
        match=r"^Source import output overlaps inputs or configured live data$",
    ):
        register(plan, inputs.input_dir / "output", inputs.program)
    monkeypatch.setenv("SVE_DATA_DIR", str(tmp_path / "live"))
    with pytest.raises(
        SourceImportError,
        match=r"^Source import output overlaps inputs or configured live data$",
    ):
        register(plan, tmp_path / "live/output", inputs.program)
    assert not (tmp_path / "live").exists()


def test_no_symlink_inputs_or_outputs(inputs: Inputs, tmp_path: Path) -> None:
    alias = tmp_path / "alias"
    alias.symlink_to(inputs.input_dir, target_is_directory=True)
    with pytest.raises(NotADirectoryError):
        prepare(alias, inputs.selection)
    output = tmp_path / "output"
    output.symlink_to(tmp_path / "missing", target_is_directory=True)
    with pytest.raises(
        SourceImportError,
        match=r"^Source import output must be absolute without symlinks or traversal$",
    ):
        register(prepare(inputs.input_dir, inputs.selection), output, inputs.program)


@pytest.mark.parametrize("stage", ["write", "publish"])
def test_interruption_leaves_no_partial_output(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch, stage: str
) -> None:
    def interrupt(*_args: object) -> None:
        raise KeyboardInterrupt

    monkeypatch.setattr(
        importer, "_write" if stage == "write" else "_rename_no_replace", interrupt
    )
    with pytest.raises(KeyboardInterrupt):
        _register(inputs, tmp_path)
    assert not (tmp_path / "isolated").exists()
    assert not list(tmp_path.glob(".source-import-*"))


def test_output_race_never_replaces_competitor(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = archive._rename_no_replace

    def race(stage: Path, destination: Path) -> None:
        destination.mkdir()
        (destination / "competitor").write_bytes(b"preserve")
        original(stage, destination)

    monkeypatch.setattr(importer, "_rename_no_replace", race)
    with pytest.raises(FileExistsError):
        _register(inputs, tmp_path)
    assert (tmp_path / "isolated/competitor").read_bytes() == b"preserve"
    assert not list(tmp_path.glob(".source-import-*"))


@pytest.mark.parametrize(
    "sql",
    [
        "DROP TABLE source_import_receipt",
        "ALTER TABLE source_import_receipt RENAME COLUMN index_bytes TO missing",
        "DELETE FROM source_import_receipt",
        "UPDATE source_import_receipt SET index_sha256 = 'wrong'",
        "UPDATE source_import_receipt SET content = x'7b7d'",
        "UPDATE resource SET stored_bytes = 1",
        "DELETE FROM resource",
        "INSERT INTO discovery_generation (id,root,status,started_at) VALUES(1,'synthetic','validated','2020-01-01')",
    ],
)
def test_bad_v2_schema_or_receipt_is_not_repaired(
    inputs: Inputs, tmp_path: Path, sql: str
) -> None:
    output = _register(inputs, tmp_path)
    db = output / "manifest/manifest.sqlite"
    with sqlite3.connect(db) as conn:
        conn.execute(sql)
    before = db.read_bytes()
    with pytest.raises(ManifestError):
        Manifest.open_snapshot(db)
    assert db.read_bytes() == before


def test_v2_seal_backup_restore_and_version_pin(inputs: Inputs, tmp_path: Path) -> None:
    output = _register(inputs, tmp_path)
    store = ArchiveStore(
        output,
        output / "manifest/manifest.sqlite",
        output / "manifest/.lock",
        tmp_path / "archive",
        "synthetic-import",
    )
    batch = seal_batch(store)
    assert batch.inventory.manifest.schema_version == 2
    assert len(batch.inventory.entries) == 3
    assert len({entry.blob.sha256 for entry in batch.inventory.entries}) == 2
    assert verify_batch(store.root, store.store_id, batch.batch_id) == batch.inventory
    backup_batch(
        store, tmp_path / "backup", batch.batch_id, require_separate_device=False
    )
    restore_backup(
        tmp_path / "backup", tmp_path / "restored", store.store_id, batch.batch_id
    )
    assert (
        verify_batch(tmp_path / "restored", store.store_id, batch.batch_id)
        == batch.inventory
    )
    manifest = tmp_path / "restored/batches" / batch.batch_id[7:] / "manifest.sqlite"
    with Manifest.open_snapshot(manifest) as snapshot:
        receipt = snapshot.source_import_receipt()
        assert receipt is not None
        assert receipt.index_bytes == (inputs.input_dir / "index.jsonl").read_bytes()
    inventory_path = batch.path / "inventory.json"
    value = json.loads(inventory_path.read_bytes())
    value["manifest"]["schema_version"] = 1
    forged = canonical(value)
    forged_id = digest(forged)
    forged_dir = store.root / "batches" / forged_id[7:]
    shutil.copytree(batch.path, forged_dir)
    (forged_dir / "inventory.json").write_bytes(forged)
    (forged_dir / "seal.json").write_bytes(
        canonical(
            {
                "input_format": 1,
                "inventory_sha256": forged_id,
                "inventory_bytes": len(forged),
            }
        )
    )
    with pytest.raises(
        ArchiveError,
        match=r"^Inventory manifest version differs from its frozen database$",
    ):
        verify_batch(store.root, store.store_id, forged_id)


def test_cli_check_and_execute_are_offline(inputs: Inputs, tmp_path: Path) -> None:
    output = tmp_path / "isolated"
    arguments = [
        "source-import",
        "register",
        "--input-dir",
        str(inputs.input_dir),
        "--selection",
        str(inputs.selection),
        "--program-root",
        str(inputs.program),
        "--output",
        str(output),
    ]
    runner = CliRunner()
    checked = runner.invoke(app, arguments)
    assert checked.exit_code == 0, checked.output
    assert json.loads(checked.output)["state"] == "checked"
    assert not output.exists()
    executed = runner.invoke(app, [*arguments, "--execute"])
    assert executed.exit_code == 0, executed.output
    assert json.loads(executed.output)["state"] == "registered"
    again = runner.invoke(app, [*arguments, "--execute"])
    assert again.exit_code == 0, again.output
    assert json.loads(again.output)["state"] == "reused"


@pytest.mark.parametrize("change", ["hash", "size"])
def test_raw_hash_and_length_independently(inputs: Inputs, change: str) -> None:
    if change == "hash":
        (inputs.input_dir / "raw" / hashlib.sha256(HTML).hexdigest()).write_bytes(
            b"X" + HTML[1:]
        )
    else:
        rows = index_rows(inputs)
        rows[0]["bytes"] = len(HTML) + 1
        write_index(inputs, rows)
    with pytest.raises(
        SourceImportError,
        match=r"^Source import raw hash or bytes differ from the index$",
    ):
        prepare(inputs.input_dir, inputs.selection)


def test_unlisted_raw_refuses_without_cleaning(inputs: Inputs) -> None:
    unlisted = inputs.input_dir / "raw" / ("0" * 64)
    unlisted.write_bytes(b"Do not clean unrelated input")
    with pytest.raises(
        SourceImportError,
        match=r"^Source import raw set differs from the complete index$",
    ):
        prepare(inputs.input_dir, inputs.selection)
    assert unlisted.read_bytes() == b"Do not clean unrelated input"


def test_inputs_changed_between_prepare_and_publish(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan = prepare(inputs.input_dir, inputs.selection)
    path = inputs.input_dir / "index.jsonl"
    path.write_bytes(path.read_bytes().replace(b'"url":', b'"url" :', 1))

    def refuse(*_args: object) -> None:
        pytest.fail("Changed preparation must be rejected before staging writes")

    monkeypatch.setattr(importer, "_write", refuse)
    with pytest.raises(
        SourceImportError,
        match=r"^Source import inputs changed after preparation$",
    ):
        register(plan, tmp_path / "isolated", inputs.program)
    assert not (tmp_path / "isolated").exists()


def test_existing_unmarked_output_is_not_opened(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    output = tmp_path / "existing"
    output.mkdir()
    (output / "keep").write_bytes(b"Existing unrelated state")

    def refuse(*_args: object) -> None:
        pytest.fail("The manifest must not be opened without the exact import marker")

    monkeypatch.setattr(Manifest, "open_snapshot", refuse)
    with pytest.raises(FileNotFoundError):
        register(prepare(inputs.input_dir, inputs.selection), output, inputs.program)
    assert (output / "keep").read_bytes() == b"Existing unrelated state"


def test_existing_manifest_symlink_rejected_before_sqlite(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    output = _register(inputs, tmp_path)
    db = output / "manifest/manifest.sqlite"
    target = tmp_path / "other.sqlite"
    db.rename(target)
    db.symlink_to(target)

    def refuse(*_args: object) -> None:
        pytest.fail("A symlink manifest must be rejected before SQLite open")

    monkeypatch.setattr(Manifest, "open_snapshot", refuse)
    with pytest.raises(OSError, match=r"Too many levels of symbolic links"):
        register(prepare(inputs.input_dir, inputs.selection), output, inputs.program)


def test_downgraded_import_receipt_is_not_treated_as_v1(
    inputs: Inputs, tmp_path: Path
) -> None:
    output = _register(inputs, tmp_path)
    db = output / "manifest/manifest.sqlite"
    with sqlite3.connect(db) as conn:
        conn.execute("PRAGMA user_version = 1")
    before = db.read_bytes()
    with pytest.raises(
        ManifestError, match=r"^Import receipt requires manifest schema 2$"
    ):
        Manifest.open_snapshot(db)
    with pytest.raises(
        ManifestError, match=r"^Import manifests cannot be opened by the crawl writer$"
    ):
        Manifest.open(db)
    assert db.read_bytes() == before


def test_v1_creation_reader_and_backup_remain_supported(tmp_path: Path) -> None:
    path = tmp_path / "legacy.sqlite"
    with Manifest.open(path) as writer:
        assert writer.schema_version == 1
        assert writer.source_import_receipt() is None
        writer.backup(tmp_path / "legacy-backup.sqlite")
    with Manifest.open_snapshot(tmp_path / "legacy-backup.sqlite") as reader:
        assert reader.schema_version == 1
        assert reader.source_import_receipt() is None


def test_redirect_keeps_requested_identity_and_exact_chain(
    inputs: Inputs, tmp_path: Path
) -> None:
    rows = index_rows(inputs)
    rows[0]["final_url"] = URLS[1]
    rows[0]["chain"] = [
        {"url": URLS[0], "status": 302, "location": "/news/post-900001/"},
        {"url": URLS[1], "status": 200, "location": None},
    ]
    write_index(inputs, rows)
    output = _register(inputs, tmp_path)
    with Manifest.open_snapshot(output / "manifest/manifest.sqlite") as manifest:
        receipt = manifest.source_import_receipt()
        assert receipt is not None
        assert receipt.index_bytes == (inputs.input_dir / "index.jsonl").read_bytes()
        assert {resource.url for resource in manifest.resources.all()} == set(URLS)


@pytest.mark.parametrize("change", ["status", "missing-location", "location", "host"])
def test_invalid_redirect_chain(inputs: Inputs, change: str) -> None:
    rows = index_rows(inputs)
    first: dict[str, object] = {
        "url": URLS[0],
        "status": 302,
        "location": "/news/post-900001/",
    }
    final = URLS[1]
    message = "Source import has an inconsistent redirect hop"
    if change == "status":
        first["status"] = 200
    elif change == "missing-location":
        first["location"] = None
    elif change == "location":
        first["location"] = "/news/other/"
    else:
        final = "https://example.invalid/news/post-900001/"
        first["location"] = final
        message = "Source import requires an explicit official HTTPS URL"
    rows[0]["final_url"] = final
    rows[0]["chain"] = [first, {"url": final, "status": 200, "location": None}]
    write_index(inputs, rows)
    with pytest.raises(ValidationError, match=message):
        prepare(inputs.input_dir, inputs.selection)


@pytest.mark.parametrize("change", ["marker", "raw"])
def test_existing_import_conflicts_never_overwrite(
    inputs: Inputs, tmp_path: Path, change: str
) -> None:
    plan = prepare(inputs.input_dir, inputs.selection)
    output = _register(inputs, tmp_path)
    if change == "marker":
        changed = output / ".source-import"
        message = r"^Existing output is not the exact isolated import$"
    else:
        changed = output / plan.content.source_mappings[0].path
        message = r"^Source import raw hash or bytes differ from the index$"
    changed.write_bytes(b"Preserve conflicting state")
    db = output / "manifest/manifest.sqlite"
    before = db.read_bytes()
    with pytest.raises(SourceImportError, match=message):
        register(plan, output, inputs.program)
    assert changed.read_bytes() == b"Preserve conflicting state"
    assert db.read_bytes() == before


def test_noncanonical_receipt_content_is_refused(
    inputs: Inputs, tmp_path: Path
) -> None:
    output = _register(inputs, tmp_path)
    db = output / "manifest/manifest.sqlite"
    with sqlite3.connect(db) as conn:
        conn.execute(
            "UPDATE source_import_receipt SET content=CAST(' ' || content AS BLOB)"
        )
    before = db.read_bytes()
    with pytest.raises(
        ManifestError, match=r"^Import receipt content must be canonical JSON$"
    ):
        Manifest.open_snapshot(db)
    assert db.read_bytes() == before


@pytest.mark.parametrize("change", ["missing", "unknown", "dirty"])
def test_program_dependency_closure_is_checked(
    inputs: Inputs, tmp_path: Path, change: str
) -> None:
    value = json.loads(inputs.selection.read_bytes())
    if change == "missing":
        value["dependencies"].pop()
        message = r"^Source import program dependency set is incomplete or unknown$"
    elif change == "unknown":
        value["dependencies"].append({"name": "z-extra", "sha256": digest(b"extra")})
        message = r"^Source import program dependency set is incomplete or unknown$"
    else:
        program = tmp_path / "dirty-program"
        shutil.copytree(inputs.program, program)
        changed = program / importer.PROGRAM_FILES[0]
        changed.write_bytes(changed.read_bytes() + b"\n")
        git(program, "add", ".")
        git(program, "commit", "-qm", "Changed synthetic program")
        value["program_revision"] = git(program, "rev-parse", "HEAD")
        inputs = Inputs(inputs.input_dir, inputs.selection, program)
        message = r"^Source import program dependency differs from executing code$"
    inputs.selection.write_text(json.dumps(value))
    with pytest.raises(SourceImportError, match=message):
        register(
            prepare(inputs.input_dir, inputs.selection),
            tmp_path / "isolated",
            inputs.program,
        )
    assert not (tmp_path / "isolated").exists()


def test_news_path_on_wrong_regional_host_is_rejected(inputs: Inputs) -> None:
    selection = json.loads(inputs.selection.read_bytes())
    selection["sources"][1]["provider"] = "en"
    inputs.selection.write_text(json.dumps(selection))
    with pytest.raises(
        SourceImportError,
        match=r"^HTML source import URL does not match its explicit purpose$",
    ):
        prepare(inputs.input_dir, inputs.selection)


def test_inputs_changed_during_staging_never_publish(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    original = importer._write
    changed = False

    def changing(path: Path, raw: bytes) -> None:
        nonlocal changed
        original(path, raw)
        if not changed:
            index = inputs.input_dir / "index.jsonl"
            index.write_bytes(index.read_bytes().replace(b'"url":', b'"url" :', 1))
            changed = True

    monkeypatch.setattr(importer, "_write", changing)
    with pytest.raises(
        SourceImportError, match=r"^Source import inputs changed after preparation$"
    ):
        _register(inputs, tmp_path)
    assert changed
    assert not (tmp_path / "isolated").exists()
    assert not list(tmp_path.glob(".source-import-*"))


def test_create_import_failure_removes_only_its_new_database(
    inputs: Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan = prepare(inputs.input_dir, inputs.selection)
    path = tmp_path / "new.sqlite"

    def failing(_self: Manifest) -> None:
        raise ManifestError("Synthetic receipt failure")

    monkeypatch.setattr(Manifest, "source_import_receipt", failing)
    with pytest.raises(ManifestError, match=r"^Synthetic receipt failure$"):
        Manifest.create_import(path, plan.index, plan.content)
    assert not path.exists()
    path.write_bytes(b"Existing state")
    with pytest.raises(FileExistsError):
        Manifest.create_import(path, plan.index, plan.content)
    assert path.read_bytes() == b"Existing state"


def test_non_utc_registration_time_is_rejected(inputs: Inputs, tmp_path: Path) -> None:
    output = _register(inputs, tmp_path)
    db = output / "manifest/manifest.sqlite"
    with sqlite3.connect(db) as conn:
        conn.execute(
            "UPDATE source_import_receipt SET registered_at='2020-01-02T04:04:05+01:00'"
        )
    with pytest.raises(ManifestError, match=r"^Import registration time must be UTC$"):
        Manifest.open_snapshot(db)


@pytest.mark.parametrize("column", ["index_bytes", "content"])
def test_receipt_bytes_boundary_rejects_non_bytes(
    inputs: Inputs, tmp_path: Path, column: str
) -> None:
    output = _register(inputs, tmp_path)
    db = output / "manifest/manifest.sqlite"
    position = 1 if column == "index_bytes" else 3

    def non_bytes_row(
        _cursor: sqlite3.Cursor, row: tuple[object, ...]
    ) -> tuple[object, ...]:
        # STRICT persisted rows are bytes; exercise the independent library boundary.
        if len(row) == 5:
            values = list(row)
            values[position] = "Synthetic non-bytes SQLite result"
            return tuple(values)
        return row

    with sqlite3.connect(db) as conn:
        conn.row_factory = non_bytes_row
        with pytest.raises(
            ManifestError, match=r"^Import receipt index and content must be bytes$"
        ):
            manifest_module._decode_import_receipt(conn)
    with Manifest.open_snapshot(db):
        pass


@pytest.mark.parametrize("change", ["pin", "classification", "fields", "missing"])
def test_cli_diagnostic_is_specific_without_input_values(
    inputs: Inputs, tmp_path: Path, change: str
) -> None:
    value = json.loads(inputs.selection.read_bytes())
    if change == "pin":
        value["dependencies"][0]["sha256"] = "sha256:" + "0" * 64
        expected = "Source import program dependency differs from executing code"
    elif change == "classification":
        value["sources"][1]["provider"] = "en"
        expected = "HTML source import URL does not match its explicit purpose"
    elif change == "fields":
        value["sources"][0]["provider"] = "PRIVATE_INPUT_VALUE_DO_NOT_PRINT"
        expected = "sources.0.provider:literal_error"
    else:
        inputs.selection.unlink()
        expected = "Offline source import I/O failed: FileNotFoundError"
    if change != "missing":
        inputs.selection.write_text(json.dumps(value))
    result = CliRunner().invoke(
        app,
        [
            "source-import",
            "register",
            "--input-dir",
            str(inputs.input_dir),
            "--selection",
            str(inputs.selection),
            "--program-root",
            str(inputs.program),
            "--output",
            str(tmp_path / "isolated"),
        ],
        env={"FORCE_COLOR": None, "NO_COLOR": "1", "TERM": "dumb"},
    )
    assert result.exit_code == 2
    assert expected in result.output
    assert "PRIVATE_INPUT_VALUE_DO_NOT_PRINT" not in result.output
    assert str(inputs.input_dir) not in result.output
    assert not (tmp_path / "isolated").exists()

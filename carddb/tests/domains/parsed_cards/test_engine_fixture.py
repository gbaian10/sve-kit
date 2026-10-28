import json
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.domains.parsed_cards.commands import app
from sve_carddb.domains.parsed_cards.engine_fixture import project
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.storage import write
from tests.support import repository_current

from .test_inputs import REVISION, _batch

if TYPE_CHECKING:
    from pathlib import Path


def test_engine_selects_current_version_without_dropping_faces(tmp_path: Path) -> None:
    source, number = _batch(tmp_path, "jp")
    catalog = generate(
        [("jp", source)], {("jp", number): "SYN"}, producer_revision=REVISION
    )
    pair = catalog.files[0]
    card = pair.cards[0]
    old = card.observations[0]
    assert old.document.region == "jp"
    record = old.document.record
    changed = replace(
        record, faces=[replace(record.faces[0], cost="9"), *record.faces[1:]]
    )
    version = "sha256:" + "b" * 64
    latest = old.model_copy(
        update={
            "document": old.document.model_copy(update={"record": changed}),
            "evidence": old.evidence.model_copy(
                update={
                    "descriptor": old.evidence.descriptor.model_copy(
                        update={"id": version}
                    )
                }
            ),
        }
    )
    batch = catalog.batches[0].model_copy(
        update={"versions": {old.evidence.descriptor.id: False, version: True}}
    )
    catalog = replace(
        catalog,
        files=(
            pair.model_copy(
                update={
                    "cards": (card.model_copy(update={"observations": (old, latest)}),)
                }
            ),
        ),
        batches=(batch,),
    )
    with pytest.raises(ValueError, match="Duplicate current engine version"):
        project(
            replace(catalog, files=(*catalog.files, *catalog.files)), source.batch_id
        )
    row = json.loads(project(catalog, source.batch_id))
    assert row["number"] == number
    assert row["faces"][0]["cost"] == "9"
    assert len(row["faces"]) == len(record.faces)
    ambiguous = batch.model_copy(
        update={"versions": {old.evidence.descriptor.id: True, version: True}}
    )
    with pytest.raises(ValueError, match="Ambiguous current"):
        project(replace(catalog, batches=(ambiguous,)), source.batch_id)


def test_engine_input_is_jp_and_requires_a_complete_selection(tmp_path: Path) -> None:
    source, number = _batch(tmp_path, "en")
    catalog = generate(
        [("en", source)], {("en", number): "SYN"}, producer_revision=REVISION
    )
    with pytest.raises(ValueError, match="requires a JP batch"):
        project(catalog, source.batch_id)
    empty = catalog.batches[0].model_copy(update={"region": "jp", "versions": {}})
    with pytest.raises(ValueError, match="incomplete or empty"):
        project(replace(catalog, batches=(empty,)), source.batch_id)


def test_engine_cli_works_without_html_and_never_overwrites(tmp_path: Path) -> None:
    source, number = _batch(tmp_path / "archive", "jp")
    catalog = generate(
        [("jp", source)], {("jp", number): "SYN"}, producer_revision=REVISION
    )
    root = tmp_path / "card-data"
    write(root, catalog)
    source.root.rename(tmp_path / "unmounted")
    output = tmp_path / "cards.jsonl"
    runner = CliRunner()
    args = ["engine-fixture", str(root), source.batch_id, str(output)]
    assert runner.invoke(app, args).exit_code == 0
    assert json.loads(output.read_bytes())["number"] == number
    before = output.read_bytes()
    assert runner.invoke(app, args).exit_code == 1
    assert output.read_bytes() == before
    assert runner.invoke(app, [*args[:-1], str(root / "new.jsonl")]).exit_code == 1
    assert runner.invoke(app, [args[0], "relative", *args[2:]]).exit_code == 1


@pytest.mark.parametrize("missing", ["cards", "evidence", "batch"])
def test_combined_inputs_fail_before_writing_when_data_is_missing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, missing: str
) -> None:
    source, number = _batch(tmp_path / "archive", "jp")
    catalog = generate(
        [("jp", source)], {("jp", number): "SYN"}, producer_revision=REVISION
    )
    repository = tmp_path / "repository"
    root = repository / "card-data"
    write(root, catalog)
    pin = repository / "tests/engine/card-input.json"
    pin.parent.mkdir(parents=True)
    pin.write_text(json.dumps({"batch_id": source.batch_id}))
    member = {
        "cards": catalog.index.files[0].path,
        "evidence": catalog.index.files[0].evidence_path,
        "batch": catalog.index.batches[0].path(0),
    }[missing]
    (root / member).unlink()
    monkeypatch.setattr(repository_current, "REPOSITORY", repository)
    cards, selection = tmp_path / "cards.jsonl", tmp_path / "selection.json"
    with pytest.raises(FileNotFoundError):
        repository_current.generate(cards, selection)
    assert not cards.exists()
    assert not selection.exists()


def test_combined_inputs_reject_an_unavailable_pin_before_writing(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    source, number = _batch(tmp_path / "archive", "jp")
    catalog = generate(
        [("jp", source)], {("jp", number): "SYN"}, producer_revision=REVISION
    )
    repository = tmp_path / "repository"
    write(repository / "card-data", catalog)
    pin = repository / "tests/engine/card-input.json"
    pin.parent.mkdir(parents=True)
    pin.write_text(json.dumps({"batch_id": "sha256:" + "f" * 64}))
    monkeypatch.setattr(repository_current, "REPOSITORY", repository)
    cards, selection = tmp_path / "cards.jsonl", tmp_path / "selection.json"
    with pytest.raises(ValueError, match="requested batch"):
        repository_current.generate(cards, selection)
    assert not cards.exists()
    assert not selection.exists()

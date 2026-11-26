"""Admission validates historical source uses; ordinary mapping reads reviewed data."""

import json
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.cli import app
from sve_carddb.contracts.four_layer import FaceRevisionOwner
from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.rulings.historical import rebuild
from sve_carddb.domains.rulings.resolution import build

from ...support.build_db_fixtures import seed
from .test_historical import reference
from .test_rulings import ruling

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture
def candidate(tmp_path: Path) -> tuple[Path, Path]:
    repo = tmp_path / "repo"
    ref = reference()
    use = ref.uses[0].model_copy(
        update={
            "cards": ("TEST-001",),
            "occurrence": ref.uses[0].occurrence.model_copy(
                update={
                    "owner": FaceRevisionOwner(
                        kind="face_revision", revision_id="revision"
                    ),
                }
            ),
        }
    )
    ref = ref.model_copy(update={"uses": (use,)})
    doc = ruling(applies_to=[ref.model_dump(mode="json")])
    target = repo / doc.source.path
    target.parent.mkdir(parents=True)
    target.write_bytes(canonical(doc.ruling.model_dump(mode="json")))
    database = tmp_path / "build.sqlite"
    with create_database(compile_build()) as db:
        seed(db)
        with db.transaction():
            db.update(
                "text_unit",
                {"id": "text"},
                {"text": "synthetic", "content_hash": digest(b"synthetic")},
            )
            db.update("printing", {"id": "printing"}, {"card_no": "TEST-001"})
        db.save(database)
    return repo, database


def test_validate_command_checks_historical_uses_in_closed_build(
    candidate: tuple[Path, Path],
) -> None:
    repo, database = candidate
    before = database.read_bytes()
    result = CliRunner().invoke(
        app, ["rulings", "validate", "--repo", str(repo), "--build-db", str(database)]
    )
    assert result.exit_code == 0, result.output
    assert "Validated 1 historical definitions across 1 rulings" in result.output
    assert database.read_bytes() == before
    assert not database.with_name(database.name + "-wal").exists()


@pytest.mark.parametrize("change", ["owner", "printing", "line", "missing_db"])
def test_validate_command_rejects_unadmitted_data_without_echoing_text(
    candidate: tuple[Path, Path], change: str
) -> None:
    repo, database = candidate
    ruling_path = repo / "authored/rules/rulings/R-0001.yaml"
    doc = json.loads(ruling_path.read_bytes())
    if change == "missing_db":
        database.unlink()
    else:
        use = doc["applies_to"][0]["uses"][0]
        if change == "owner":
            use["occurrence"]["owner"]["revision_id"] = "absent"
        elif change == "printing":
            use["cards"] = ["TEST-002"]
        else:
            use["occurrence"]["line_ordinal"] = 1
        ruling_path.write_text(json.dumps(doc))
    result = CliRunner().invoke(
        app, ["rulings", "validate", "--repo", str(repo), "--build-db", str(database)]
    )
    assert result.exit_code == 1
    assert "Historical ruling validation failed" in result.output
    assert "synthetic" not in result.output
    assert str(database) not in result.output


def test_ordinary_rebuild_uses_reviewed_historical_domains() -> None:
    ref = reference()
    documents = (ruling(applies_to=[ref.model_dump(mode="json")]),)
    report = build(documents, rebuild(documents, ()))
    assert report.resolutions[0].legacy.occurrence == ref.uses[0].occurrence
    assert report.resolutions[0].reason == "no_candidate"
    assert report.payload()["active_template_edges"] == 0

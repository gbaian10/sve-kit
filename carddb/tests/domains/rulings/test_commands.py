"""Admission owns complete-domain checks; ordinary mapping reads reviewed data."""

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
from ..effects.test_storage import write_catalog
from .test_historical import reference
from .test_rulings import ruling

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture
def candidate(tmp_path: Path) -> tuple[Path, Path]:
    repo = tmp_path / "repo"
    records, sources = write_catalog(repo / "authored")
    for path in (records, sources):
        content = json.loads(path.read_bytes())
        content["namespace"] = "classification-v0"
        path.write_text(json.dumps(content).replace("T-example", "T0123456789"))
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


def test_validate_command_checks_real_catalog_and_closed_build(
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


@pytest.mark.parametrize(
    "change", ["body", "member", "owner", "printing", "line", "missing_db"]
)
def test_validate_command_rejects_unadmitted_data_without_echoing_text(
    candidate: tuple[Path, Path], change: str
) -> None:
    repo, database = candidate
    ruling_path = repo / "authored/rules/rulings/R-0001.yaml"
    doc = json.loads(ruling_path.read_bytes())
    if change == "body":
        path = repo / "authored/rules/classification-sources/TEST/001.yaml"
        content = json.loads(path.read_bytes())
        text = "protected"
        checksum = digest(canonical({"raw": text, "body": text}))
        content["records"][0] = {"raw": text, "id": checksum}
        path.write_text(json.dumps(content))
        path = repo / "authored/rules/classifications/TEST/001.yaml"
        content = json.loads(path.read_bytes())
        content["records"][1]["source"] = checksum
        path.write_text(json.dumps(content))
    elif change == "member":
        path = repo / "authored/rules/classifications/TEST/001.yaml"
        content = json.loads(path.read_bytes())
        member = content["records"][1]
        extra = member | {
            "id": "TEST-002:effect:0",
            "position": member["position"] | {"card_no": "TEST-002"},
        }
        content["records"].append(extra)
        content["records"][0]["members"].append(extra["id"])
        path.write_text(json.dumps(content))
    elif change == "missing_db":
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
    assert "protected" not in result.output
    assert str(database) not in result.output


def test_ordinary_rebuild_reads_admitted_domains_without_source_or_catalog_inputs() -> (
    None
):
    ref = reference()
    documents = (ruling(applies_to=[ref.model_dump(mode="json")]),)
    report = build(documents, rebuild(documents, ()))
    assert report.resolutions[0].legacy.occurrence == ref.uses[0].occurrence
    assert report.resolutions[0].reason == "no_candidate"
    assert report.payload()["active_template_edges"] == 0

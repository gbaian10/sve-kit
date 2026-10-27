"""CLI exit codes and local reports use the public authoring workflow."""

import shutil
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.json import array, object_value, parse
from sve_carddb.workflows import authoring

from ..support.authoring_cli_fixtures import synthetic_card_inputs

if TYPE_CHECKING:
    from collections.abc import Callable

    from ..support.authoring_cli_fixtures import RepositoryCheck


@pytest.mark.parametrize(
    ("case", "expected"),
    [("bad-json", 2), ("relative", 2), ("missing", 4), ("profile", 2)],
)
def test_configuration_exit_codes(tmp_path: Path, case: str, expected: int) -> None:
    inputs = tmp_path / "inputs.json"
    if case == "bad-json":
        inputs.write_text("{")
    elif case == "profile":
        inputs.write_text('{"profile":"invalid"}')
    argument = "relative.json" if case == "relative" else str(inputs)
    result = CliRunner().invoke(
        app,
        [
            "author",
            "check",
            "--inputs",
            argument,
            "--output",
            str(tmp_path / "report.json"),
        ],
    )
    assert result.exit_code == expected
    assert not (tmp_path / "report.json").exists()
    assert "Traceback" not in result.output


@pytest.mark.slow(
    reason="Keep one CLI end-to-end check that prepares the real public profile and validates the complete authored repository"
)
def test_cli_public_check_writes_the_complete_report(
    complete_repository_check: Callable[[], RepositoryCheck],
) -> None:
    result = complete_repository_check()
    output = result.output
    assert result.exit_code == 0
    report = object_value(parse(output.read_bytes()))
    assert report["status"] == "ok"
    authors = array(object_value(report["selection"])["authors"])
    assert len(authors) >= 14
    assert all(object_value(item)["status"] == "accepted" for item in authors)
    assert not result.stdout


def test_write_requires_explicit_selection_and_storage() -> None:
    result = CliRunner().invoke(app, ["author", "write"])
    assert result.exit_code == 2


def test_check_reports_index_location_and_card_without_wording(
    tmp_path: Path, authoring_inputs: authoring.Inputs, authoring_inventory: Path
) -> None:
    authored = Path(shutil.copytree(authoring_inventory, tmp_path / "authored"))
    (authored / "translations/glossary").mkdir(parents=True)
    parsed, batch = synthetic_card_inputs(tmp_path / "card-inputs")
    missing = "rules/author-effects/BP06/000.yaml"
    (authored / missing).unlink()
    inputs = authoring_inputs.model_copy(
        update={"authored": authored, "card_data": parsed, "jp_batch_id": batch}
    )
    path = tmp_path / "inputs.json"
    path.write_text(inputs.model_dump_json())
    output = tmp_path / "report.json"
    result = CliRunner().invoke(
        app, ["author", "check", "--inputs", str(path), "--output", str(output)]
    )
    assert result.exit_code == 3
    diagnostic = object_value(parse(result.stderr.encode()))
    assert diagnostic["stage"] == "index"
    assert diagnostic["reason"] == "index_file_missing"
    assert diagnostic["path"] == [missing]
    assert diagnostic["card_no"] == "BP06-048"
    assert "source" not in diagnostic
    assert not output.exists()


def test_output_symlink_is_rejected_before_inputs_are_opened(tmp_path: Path) -> None:
    target = tmp_path / "keep.txt"
    target.write_text("keep")
    output = tmp_path / "output.json"
    output.symlink_to(target)
    result = CliRunner().invoke(
        app,
        [
            "author",
            "check",
            "--inputs",
            str(tmp_path / "missing.json"),
            "--output",
            str(output),
        ],
    )
    assert result.exit_code == 2
    assert "--output rejects symlinks" in result.output
    assert target.read_text() == "keep"


def test_yaml_formatter_preserves_long_fingerprints_without_trailing_spaces() -> None:
    raw = authoring.format_yaml({"basis": "effect-anchor-v1:sha256:" + "a" * 64})
    assert all(line == line.rstrip() for line in raw.splitlines())
    assert b"effect-anchor-v1:sha256:" + b"a" * 64 in raw

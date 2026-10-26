"""CLI exit codes and local reports use the public authoring workflow."""

import shutil
from pathlib import Path

import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.core.json import object_value, parse, string
from sve_carddb.workflows import authoring

REPO = Path(__file__).resolve().parents[3]


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


def test_cli_public_check_writes_the_complete_report(tmp_path: Path) -> None:
    batch = string(
        object_value(parse((REPO / "tests/engine/card-input.json").read_bytes()))[
            "batch_id"
        ]
    )
    inputs = authoring.Inputs(
        profile="public",
        repo=REPO,
        jp_batch_id=batch,
        revision="1036aa89bb1ce52cf4a964df8461663ab644b614",
    )
    path = tmp_path / "inputs.json"
    path.write_text(inputs.model_dump_json())
    output = tmp_path / "report.json"
    result = CliRunner().invoke(
        app, ["author", "check", "--inputs", str(path), "--output", str(output)]
    )
    assert result.exit_code == 0
    assert object_value(parse(output.read_bytes()))["status"] == "ok"
    assert not result.stdout


def test_write_requires_explicit_selection_and_storage() -> None:
    result = CliRunner().invoke(app, ["author", "write"])
    assert result.exit_code == 2


def test_check_reports_index_location_and_card_without_wording(tmp_path: Path) -> None:
    authored = Path(shutil.copytree(REPO / "authored", tmp_path / "authored"))
    missing = "rules/author-effects/BP06/000.yaml"
    (authored / missing).unlink()
    batch = string(
        object_value(parse((REPO / "tests/engine/card-input.json").read_bytes()))[
            "batch_id"
        ]
    )
    inputs = authoring.Inputs(
        profile="public",
        repo=REPO,
        authored=authored,
        jp_batch_id=batch,
        revision="1036aa89bb1ce52cf4a964df8461663ab644b614",
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

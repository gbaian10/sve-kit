"""Publisher roots and environment precedence."""

from typing import TYPE_CHECKING

import pytest
from sve_carddb.export.read_api import ExportError
from typer.testing import CliRunner

from sve_publish import commands
from sve_publish.cli import app

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.export.read_api import Export


@pytest.mark.parametrize("command", [[], ["upload"], ["gc"]])
def test_help_needs_no_output_roots(command: list[str]) -> None:
    assert CliRunner().invoke(app, [*command, "--help"]).exit_code == 0


@pytest.mark.parametrize(
    "source", ["missing", "empty-env", "relative-env", "empty-cli", "relative-cli"]
)
def test_upload_rejects_invalid_root(source: str, tmp_path: Path) -> None:
    arguments = ["upload"]
    env = {}
    if source.endswith("env"):
        env["SVE_EXPORT_DIR"] = "" if source == "empty-env" else "relative"
    if source.endswith("cli"):
        arguments.extend(["--export-dir", "" if source == "empty-cli" else "relative"])
        env["SVE_EXPORT_DIR"] = str(tmp_path / "unused-env")
    result = CliRunner().invoke(
        app,
        arguments,
        env=env | {"FORCE_COLOR": None, "NO_COLOR": "1", "TERM": "dumb"},
    )
    assert result.exit_code == 2
    assert "SVE_EXPORT_DIR" in result.output
    assert "--export-dir" in result.output


@pytest.mark.parametrize("source", ["env", "cli", "cli-over-env"])
def test_upload_root_precedence(
    source: str, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    selected: list[Path] = []

    def capture(root: Path) -> Export:
        selected.append(root)
        raise ExportError("synthetic stop before reading")

    monkeypatch.setattr(commands, "load_export", capture)
    root = tmp_path / "export"
    arguments = ["upload"]
    if source != "env":
        arguments.extend(["--export-dir", str(root)])
    env = (
        {}
        if source == "cli"
        else {"SVE_EXPORT_DIR": str(root if source == "env" else tmp_path / "unused")}
    )
    result = CliRunner().invoke(
        app,
        arguments,
        env=env | {"FORCE_COLOR": None, "NO_COLOR": "1", "TERM": "dumb"},
    )
    assert "synthetic stop before reading" in result.output
    assert selected == [root]

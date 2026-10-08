"""Shared root configuration is resolved only by the commands that need it."""

from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.r2_upload.v2 import commands
from sve_carddb.snapshot.read_api import ExportError as UploadError

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.snapshot.read_api import Export


@pytest.mark.parametrize(
    "command", [[], ["snapshot", "export-offline"], ["r2", "upload-v2"]]
)
def test_help_needs_no_output_roots(command: list[str]) -> None:
    result = CliRunner().invoke(app, [*command, "--help"])
    assert result.exit_code == 0


@pytest.mark.parametrize(
    ("option", "environment"),
    [("--preview-dir", "SVE_EXPORT_DIR"), ("--private-dir", "SVE_CARDDB_PRIVATE_DIR")],
)
@pytest.mark.parametrize(
    "source", ["missing", "empty-env", "relative-env", "empty-cli", "relative-cli"]
)
def test_export_rejects_invalid_roots_before_reading_recipe(
    tmp_path: Path, option: str, environment: str, source: str
) -> None:
    inputs = tmp_path / "inputs.json"
    inputs.write_text("invalid recipe must not be read", encoding="utf-8")
    arguments = [
        "snapshot",
        "export-offline",
        "--inputs",
        str(inputs),
        "--bundle-dir",
        str(tmp_path / "bundle"),
    ]
    other = "--private-dir" if option == "--preview-dir" else "--preview-dir"
    arguments.extend([other, str(tmp_path / "other")])
    env = {"SVE_PREVIEW_DIR": str(tmp_path / "old-alias")}
    if source.endswith("env"):
        env[environment] = "" if source == "empty-env" else "relative"
    if source.endswith("cli"):
        arguments.extend([option, "" if source == "empty-cli" else "relative"])
        env[environment] = str(tmp_path / "unused-env")
    result = CliRunner().invoke(
        app,
        arguments,
        env=env | {"FORCE_COLOR": None, "NO_COLOR": "1", "TERM": "dumb"},
    )
    assert result.exit_code == 2
    assert environment in result.output
    assert option in result.output
    assert not (tmp_path / "bundle").exists()
    assert not (tmp_path / "other").exists()


@pytest.mark.parametrize(
    "source", ["missing", "empty-env", "relative-env", "empty-cli", "relative-cli"]
)
def test_upload_rejects_invalid_root(source: str, tmp_path: Path) -> None:
    arguments = ["r2", "upload-v2"]
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
        raise UploadError("synthetic stop before reading")

    monkeypatch.setattr(commands, "load_export", capture)
    root = tmp_path / "export"
    arguments = ["r2", "upload-v2"]
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

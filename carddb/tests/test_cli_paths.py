"""Shared root configuration is resolved only by the commands that need it."""

from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("command", [[], ["snapshot", "export-offline"]])
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
        env=env,
    )
    assert result.exit_code == 2
    assert environment in result.output
    assert option in result.output
    assert not (tmp_path / "bundle").exists()
    assert not (tmp_path / "other").exists()

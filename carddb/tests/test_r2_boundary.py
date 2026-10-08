"""Shared local input safety after removing the preview uploader."""

from typing import TYPE_CHECKING

import pytest
from typer.testing import CliRunner

from sve_carddb.cli import app
from sve_carddb.snapshot.read_api import ExportError as UploadError
from sve_carddb.snapshot.read_api import read_member

if TYPE_CHECKING:
    from pathlib import Path


@pytest.mark.parametrize("key", ["/absolute", "../outside", "nested/../member"])
def test_member_read_rejects_escaping_keys(tmp_path: Path, key: str) -> None:
    with pytest.raises(UploadError, match=r"^Invalid public member key$"):
        read_member(tmp_path, key)


@pytest.mark.parametrize("kind", ["file-link", "directory-link", "directory"])
def test_member_read_rejects_aliases_and_nonfiles(tmp_path: Path, kind: str) -> None:
    source = tmp_path / "source"
    source.mkdir()
    (source / "member").write_bytes(b"synthetic")
    if kind == "file-link":
        (tmp_path / "member").symlink_to(source / "member")
        key = "member"
    elif kind == "directory-link":
        (tmp_path / "alias").symlink_to(source, target_is_directory=True)
        key = "alias/member"
    else:
        key = "source"
    with pytest.raises(
        UploadError, match=r"^Public member is not a regular non-symlink file$"
    ):
        read_member(tmp_path, key)


def test_member_read_preserves_exact_bytes(tmp_path: Path) -> None:
    raw = b"synthetic\r\n\x00"
    (tmp_path / "member").write_bytes(raw)
    assert read_member(tmp_path, "member") == raw


def test_r2_cli_has_only_current_publication_and_collection() -> None:
    runner = CliRunner()
    for command in ("upload-v2", "gc-v2"):
        assert runner.invoke(app, ["r2", command, "--help"]).exit_code == 0
    result = runner.invoke(app, ["r2", "upload-preview"])
    assert result.exit_code == 2

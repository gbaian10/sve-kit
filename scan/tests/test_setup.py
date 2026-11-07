import subprocess
import sys

import pytest

from sve_scan import paths, splits
from sve_scan.__main__ import COMMANDS


def test_held_out_events_include_second_search_pages():
    assert splits.is_eval("2026-10-24_GP_千葉")
    assert splits.is_eval("2026-10-24_GP_千葉_p2")
    assert not splits.is_eval("2026-09-13_GP_福岡")


def test_paths_come_from_the_environment(monkeypatch, tmp_path):
    monkeypatch.setenv("SVE_SCAN_WORK", str(tmp_path / "w"))
    monkeypatch.setenv("SVE_SCAN_DATA", str(tmp_path / "d"))
    monkeypatch.setenv("SVE_SCAN_EXPORT", str(tmp_path / "e"))
    assert paths.work() == tmp_path / "w" and (tmp_path / "w").is_dir()
    assert paths.card_images() == tmp_path / "e" / "images" / "card_l"
    assert paths.deck_photos() == tmp_path / "d" / "web-x" / "sample-events-orig"


def test_missing_root_says_which_variable(monkeypatch):
    monkeypatch.delenv("SVE_SCAN_DATA", raising=False)
    with pytest.raises(SystemExit, match="SVE_SCAN_DATA"):
        paths.private()


def test_every_command_names_a_module():
    import importlib.util

    for name, (module, help_) in COMMANDS.items():
        assert importlib.util.find_spec(f"sve_scan.{module}") is not None, name
        assert help_


def test_cli_lists_commands_and_rejects_unknown_ones():
    listed = subprocess.run([sys.executable, "-m", "sve_scan"], capture_output=True, text=True, check=True)
    assert "eval-rec" in listed.stdout and "release" in listed.stdout
    bad = subprocess.run([sys.executable, "-m", "sve_scan", "nope"], capture_output=True, text=True)
    assert bad.returncode == 2

"""The installed publisher consumes only the public carddb reader API."""

import ast
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- fresh isolated Python import, no shell
import sys
from pathlib import Path
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    import pytest


def test_publisher_imports_only_read_api() -> None:
    source = Path(__file__).resolve().parents[1] / "src" / "sve_publish"
    for path in source.glob("*.py"):
        tree = ast.parse(path.read_text(encoding="utf-8"))
        for node in ast.walk(tree):
            names = (
                [node.module or ""]
                if isinstance(node, ast.ImportFrom)
                else [alias.name for alias in node.names]
                if isinstance(node, ast.Import)
                else []
            )
            for name in names:
                if name == "sve_carddb" or name.startswith("sve_carddb."):
                    assert name == "sve_carddb.export.read_api", (path.name, name)


def test_fresh_publisher_does_not_load_carddb_writers() -> None:
    result = subprocess.run(
        [
            sys.executable,
            "-I",
            "-B",
            "-c",
            (
                "import sys; import sve_publish.cli; "
                "blocked = ('sve_carddb.build', 'sve_carddb.domains', "
                "'sve_carddb.workflows', 'sve_carddb.ingest', 'sve_carddb.parse', "
                "'sve_carddb.export.project', 'sve_carddb.export.preview', "
                "'sve_carddb.export.transport'); "
                "assert not any(n == p or n.startswith(p + '.') "
                "for n in sys.modules for p in blocked)"
            ),
        ],
        check=False,
        capture_output=True,
    )
    assert result.returncode == 0, result.stderr.decode()


def test_entry_point(monkeypatch: pytest.MonkeyPatch) -> None:
    from sve_publish import cli  # ruff: ignore[import-outside-top-level] -- exercise the installed script target

    called: list[bool] = []
    monkeypatch.setattr(cli, "app", lambda: called.append(True))
    cli.main()
    assert called == [True]

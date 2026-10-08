"""Expose cycles without pytest's warmed imports.

One fresh process per module makes startup cost grow linearly with module count.
"""

import subprocess  # ruff: ignore[suspicious-subprocess-import] -- a fresh interpreter must exclude pytest's preloaded package modules
import sys
from pathlib import Path

import pytest

SOURCE_ROOT = Path(__file__).resolve().parents[1] / "src"
MODULES = tuple(
    sorted(
        {
            ".".join(
                path.with_suffix("")
                .relative_to(SOURCE_ROOT)
                .parts[: -1 if path.stem == "__init__" else None]
            )
            for path in (SOURCE_ROOT / "sve_carddb").rglob("*.py")
        }
    )
)
IMPORT = """
import importlib
import os
import sys

def audit(event, args):
    if event in {'socket.connect', 'socket.sendto', 'socket.getaddrinfo', 'sqlite3.connect'}:
        raise RuntimeError('Module import must not access network or databases')
    if event == 'open':
        mode, flags = args[1:3]
        writing = isinstance(mode, str) and any(char in mode for char in 'wax+')
        writing = writing or (isinstance(flags, int) and bool(flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC | os.O_APPEND)))
        if writing:
            raise RuntimeError('Module import must not write files')

sys.addaudithook(audit)
assert not any(name == 'sve_carddb' or name.startswith('sve_carddb.') for name in sys.modules)
sys.path.insert(0, sys.argv[1])
importlib.import_module(sys.argv[2])
"""


@pytest.mark.parametrize("module", MODULES)
def test_module_first_import_in_fresh_process(module: str, tmp_path: Path) -> None:
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed interpreter and module argument, without a shell
        [sys.executable, "-I", "-B", "-c", IMPORT, str(SOURCE_ROOT), module],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    assert result.returncode == 0, f"First import failed for {module}: {result.stderr}"


def test_read_api_import_does_not_load_build_or_publish(tmp_path: Path) -> None:
    isolated = (
        IMPORT
        + """
for name in sys.modules:
    assert not any(name == prefix or name.startswith(prefix + '.') for prefix in (
        'sve_carddb.build', 'sve_carddb.domains', 'sve_carddb.export.transport',
        'sve_carddb.export.project', 'sve_carddb.export.preview',
        'sve_carddb.workflows', 'sve_carddb.r2_upload', 'sve_publish',
    )), name
"""
    )
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed interpreter without a shell
        [
            sys.executable,
            "-I",
            "-B",
            "-c",
            isolated,
            str(SOURCE_ROOT),
            "sve_carddb.export.read_api",
        ],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    assert result.returncode == 0, result.stderr


@pytest.mark.parametrize(
    "module",
    [
        name
        for name in MODULES
        if name == "sve_carddb.parse" or name.startswith("sve_carddb.parse.")
    ],
)
def test_parser_import_does_not_load_archive_or_posix_lock(
    module: str, tmp_path: Path
) -> None:
    isolated = (
        IMPORT.replace(
            "importlib.import_module(sys.argv[2])",
            """
class WithoutPosixLock:
    def find_spec(self, fullname, path=None, target=None):
        if fullname == 'fcntl':
            raise ModuleNotFoundError('fcntl is unavailable on this platform')

sys.meta_path.insert(0, WithoutPosixLock())
importlib.import_module(sys.argv[2])
""",
        )
        + """
assert 'fcntl' not in sys.modules
assert not any(name.startswith('sve_carddb.ingest.archive') for name in sys.modules)
assert not any(name.startswith('sve_carddb.domains') for name in sys.modules)
"""
    )
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed interpreter without a shell
        [sys.executable, "-I", "-B", "-c", isolated, str(SOURCE_ROOT), module],
        cwd=tmp_path,
        capture_output=True,
        text=True,
        timeout=10,
        check=False,
    )
    assert result.returncode == 0, result.stderr

"""Golden bytes for the permanent v2 format, independent of the executing checkout."""

import sqlite3
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- A fresh interpreter checks the manifest import boundary.
import sys
from typing import TYPE_CHECKING

from sve_carddb import manifest as module
from sve_carddb.manifest import Manifest
from sve_carddb.manifest_schema_v2 import SCHEMA_SQL
from sve_carddb.snapshot.values import canonical, digest
from sve_carddb.source_import.models import Content, receipt_id

if TYPE_CHECKING:
    from pathlib import Path

    import pytest

INDEX = b'{"bytes":1,"chain":[{"location":null,"status":200,"url":"https://shadowverse-evolve.com/rules/"}],"content_type":"text/html","etag":null,"fetched_at":"2020-01-02T03:04:05+00:00","final_url":"https://shadowverse-evolve.com/rules/","last_modified":null,"sha256":"2222222222222222222222222222222222222222222222222222222222222222","status":200,"url":"https://shadowverse-evolve.com/rules/"}\n'
CONTENT = '{"dependencies":[{"name":"synthetic-program","sha256":"sha256:3333333333333333333333333333333333333333333333333333333333333333"}],"program_revision":"1111111111111111111111111111111111111111","source_mappings":[{"kind":"rules","path":"raw/jp/rules/091153e384401c5bc5974dc9da9ca3ea7871e9ea891d6b74cb548259b2cc7c76.html","provider":"jp","raw_hash":"sha256:2222222222222222222222222222222222222222222222222222222222222222","url":"https://shadowverse-evolve.com/rules/"}]}'


def test_v2_schema_signature_golden() -> None:
    with sqlite3.connect(":memory:") as conn:
        conn.executescript(SCHEMA_SQL)
        signature = canonical([list(row) for row in module._schema_signature(conn)])
    assert digest(signature) == (
        "sha256:41b72ddccaf76b045f4bad327840bc0ae1be9f6abccc1ffaec1e1fe18fa767d7"
    )


def test_receipt_id_golden_includes_exact_index_bytes() -> None:
    content = Content.model_validate_json(CONTENT)
    assert receipt_id(INDEX, content) == (
        "sha256:424981ee03d5508c1ef2174d692d612850fd67ee15f5a5dc5cf6b99fdc879949"
    )
    assert receipt_id(INDEX + b" ", content) != receipt_id(INDEX, content)


def test_live_schema_evolution_cannot_redefine_sealed_v2(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    old = tmp_path / "old-v2.sqlite"
    content = Content.model_validate_json(CONTENT)
    with Manifest.create_import(old, INDEX, content):
        pass
    monkeypatch.setattr(
        module,
        "_SCHEMA",
        module._SCHEMA + "\nCREATE INDEX future_live ON resource(kind);",
    )
    new = tmp_path / "new-v2.sqlite"
    with Manifest.create_import(new, INDEX, content), Manifest.open_snapshot(old):
        pass
    with sqlite3.connect(new) as conn, sqlite3.connect(old) as previous:
        assert module._schema_signature(conn) == module._schema_signature(previous)


def test_v1_import_does_not_load_build_models() -> None:
    script = """
import sys
from sve_carddb.manifest import Manifest
with Manifest.open_empty() as manifest:
    assert manifest.schema_version == 1
assert 'pydantic' not in sys.modules
assert 'sve_carddb.build_inputs' not in sys.modules
assert 'sve_carddb.source_import.models' not in sys.modules
"""
    result = subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- Fixed local Python code, with no shell or external I/O.
        [sys.executable, "-c", script], capture_output=True, check=False
    )
    assert result.returncode == 0, result.stderr.decode()

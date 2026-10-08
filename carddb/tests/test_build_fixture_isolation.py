"""Database state, connection settings and corrupted archives stay test-local."""

import sqlite3
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from .build_source_fixtures import sealed_uses as sealed_uses  # ruff: ignore[useless-import-alias] -- exercise the actual per-test fixture
from .build_source_fixtures import sealed_uses_template as sealed_uses_template  # ruff: ignore[useless-import-alias] -- register module fixture dependency
from .test_image_assets import frozen as frozen  # ruff: ignore[useless-import-alias] -- exercise the actual per-test fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_inputs import SourceUse
    from sve_carddb.frozen_sources import FrozenSources

    from .database_fixtures import DatabaseTemplate


@pytest.mark.parametrize("kind", ["t0", "t1b"])
def test_database_copies_isolate_rows_transactions_and_connection_settings(
    t0_database_template: DatabaseTemplate,
    t1b_database_template: DatabaseTemplate,
    kind: str,
) -> None:
    template = t0_database_template if kind == "t0" else t1b_database_template
    with template.copy() as first:
        expected = first.rows("language")
        with first.transaction():
            first.update("language", {"code": "ja"}, {"display_name": "changed"})
        first._connection.execute("PRAGMA foreign_keys=OFF")
        first._connection.execute("PRAGMA ignore_check_constraints=ON")
        first._connection.execute("BEGIN")
        with template.copy() as second:
            assert second.rows("language") == expected
            assert not second._connection.in_transaction
            assert first._connection is not second._connection
            assert first._rules is not second._rules
            second.verify()
            with pytest.raises(sqlite3.IntegrityError), second.transaction():
                second._connection.execute("UPDATE card_int_id SET int_id=-1")
            with pytest.raises(sqlite3.IntegrityError), second.transaction():
                second._connection.execute(
                    "UPDATE card_int_id SET printing_id='absent'"
                )
        first._connection.execute("ROLLBACK")
    with template.copy() as later:
        assert later.rows("language") == expected
        later.verify()


def test_bundle_archive_copy_keeps_template_bytes_and_provenance(
    sealed_uses: tuple[Path, tuple[SourceUse, ...]],
    sealed_uses_template: tuple[Path, tuple[SourceUse, ...]],
) -> None:
    root, uses = sealed_uses
    template, original_uses = sealed_uses_template
    before = {
        p.relative_to(template): p.read_bytes()
        for p in template.rglob("*")
        if p.is_file()
    }
    for name in before:
        (root / name).write_bytes(b"Corrupt synthetic evidence")
    assert before == {
        p.relative_to(template): p.read_bytes()
        for p in template.rglob("*")
        if p.is_file()
    }
    assert uses == original_uses
    with pytest.raises(ValidationError, match="frozen"):
        uses[0].source.archive.batch_id = "sha256:" + "0" * 64  # type: ignore[misc]  # exercise nested provenance immutability


def test_image_archive_copy_isolates_files_and_provider_entries(
    frozen: FrozenSources, image_archive_template: tuple[Path, str, str]
) -> None:
    root, _, _ = image_archive_template
    before = {
        p.relative_to(root.parent): p.read_bytes()
        for p in root.parent.rglob("*")
        if p.is_file()
    }
    for name in before:
        (frozen.root.parent / name).write_bytes(b"Corrupt synthetic input")
    frozen.entries.clear()
    assert before == {
        p.relative_to(root.parent): p.read_bytes()
        for p in root.parent.rglob("*")
        if p.is_file()
    }
    assert root != frozen.root

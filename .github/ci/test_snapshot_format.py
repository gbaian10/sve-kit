# ruff: file-ignore[no-self-use, pytest-unittest-raises-assertion] -- repo CI uses standalone unittest with dev dependencies only
"""Exercise format locking with synthetic schemas and reader modules only."""

import copy
import json
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- exercises real Git against an isolated synthetic history
import tempfile
import unittest
from pathlib import Path

from pydantic import JsonValue

from snapshot_format import (
    LOCK_PATH,
    baseline_lock,
    check_lock,
    fingerprint,
    reader_sources,
    update_lock,
)


def schema() -> dict[str, JsonValue]:
    """Return a small tuple with an enum, a nullable field and a wire description key."""
    return {
        "$defs": {
            "Row": {
                "type": "array",
                "x-columns": ["id", "description"],
                "prefixItems": [{"type": "string"}, {"type": ["string", "null"]}],
                "items": False,
            },
            "Owner": {"enum": ["global", "family"]},
            "Manifest": {
                "type": "object",
                "properties": {"data_version": {"type": "string"}},
                "required": ["data_version"],
            },
        },
    }


def pin(value: str, *, frozen: bool = False) -> dict[str, JsonValue]:
    """Represent the public lock file shape without real snapshot data."""
    return {"4.0.0": {"fingerprint": value, "frozen": frozen}}


class FormatLockTests(unittest.TestCase):
    def test_tuple_changes_require_explicit_update(self) -> None:
        """Changes in tuple order, length, type or nullability require a new pin."""
        original = schema()
        locked = pin(fingerprint(original, {}))
        changes: list[dict[str, JsonValue]] = [
            {"x-columns": ["description", "id"]},
            {"x-columns": ["id"]},
            {"prefixItems": [{"type": "integer"}, {"type": ["string", "null"]}]},
            {"prefixItems": [{"type": "string"}, {"type": "string"}]},
        ]
        for change in changes:
            with self.subTest(change=change):
                changed = copy.deepcopy(original)
                definitions = changed["$defs"]
                assert isinstance(definitions, dict)
                row = definitions["Row"]
                assert isinstance(row, dict)
                row.update(change)
                current = {"4.0.0": fingerprint(changed, {})}
                with self.assertRaisesRegex(ValueError, "unfrozen.*4.0.0"):
                    check_lock(locked, current, {})
                check_lock(update_lock(locked, current, {}), current, {})

    def test_frozen_change_requires_version_bump_even_with_manual_lock_edit(
        self,
    ) -> None:
        """Frozen history survives updates, hand edits, thawing and removal."""
        old = fingerprint(schema(), {})
        new = fingerprint(schema(), {"reader": "def join(a, b): return b | a"})
        locked = pin(old, frozen=True)
        with self.assertRaisesRegex(ValueError, "Bump format_version"):
            check_lock(locked, {"4.0.0": new}, locked)
        with self.assertRaisesRegex(ValueError, "Bump format_version"):
            update_lock(locked, {"4.0.0": new}, locked)
        for edited in (pin(new, frozen=True), pin(new), {}):
            with (
                self.subTest(edited=edited),
                self.assertRaisesRegex(ValueError, "cannot be changed"),
            ):
                check_lock(edited, {"4.0.0": new}, locked)
        bumped = locked | {"5.0.0": {"fingerprint": new, "frozen": False}}
        check_lock(bumped, {"5.0.0": new}, locked)

    def test_initial_lock_and_freeze(self) -> None:
        """Allow explicit initial pinning and freezing without a wire change."""
        value = fingerprint(schema(), {})
        with self.assertRaisesRegex(ValueError, "--update"):
            check_lock({}, {"4.0.0": value}, {})
        initial = update_lock({}, {"4.0.0": value}, {})
        check_lock(initial, {"4.0.0": value}, {})
        check_lock(pin(value, frozen=True), {"4.0.0": value}, initial)

    def test_enum_and_partition_configuration_changes_require_update(self) -> None:
        """Schema enums and fixed reader band counts both participate in the pin."""
        old = fingerprint(schema(), {"profile": "BUCKETS = 64\n"})
        changed = schema()
        definitions = changed["$defs"]
        assert isinstance(definitions, dict)
        definitions["Owner"] = {"enum": ["global", "family", "extra"]}
        for current in (
            fingerprint(changed, {"profile": "BUCKETS = 64\n"}),
            fingerprint(schema(), {"profile": "BUCKETS = 32\n"}),
        ):
            with (
                self.subTest(current=current),
                self.assertRaisesRegex(ValueError, "--update"),
            ):
                check_lock(pin(old), {"4.0.0": current}, {})

    def test_annotations_and_set_order_are_not_wire_changes(self) -> None:
        """Layout, comments and unordered schema sets do not change the fingerprint."""
        original = schema()
        changed = copy.deepcopy(original)
        changed["$comment"] = "A new comment"
        changed["title"] = "A new title"
        definitions = changed["$defs"]
        assert isinstance(definitions, dict)
        row = definitions["Row"]
        assert isinstance(row, dict)
        row["description"] = "New prose about the same tuple"
        definitions["Owner"] = {"enum": ["family", "global"]}
        assert fingerprint(original, {}) == fingerprint(changed, {})
        assert fingerprint(
            original, {"reader": '"""Old docs."""\nx = 1\n'}
        ) == fingerprint(
            original,
            {"reader": '# comment\n"""New docs."""\nx=(1) # inline\n'},
        )

    def test_wire_fields_named_like_annotations_remain_protected(self) -> None:
        """A wire field or literal description is not a schema annotation."""
        original: JsonValue = {"properties": {"description": {"type": "string"}}}
        changed: JsonValue = {"properties": {"description": {"type": "integer"}}}
        assert fingerprint(original, {}) != fingerprint(changed, {})
        assert fingerprint({"const": {"description": "old"}}, {}) != fingerprint(
            {"const": {"description": "new"}},
            {},
        )

    def test_schema_equivalence_preserves_json_boolean_and_integer_types(self) -> None:
        """Python's True == 1 must not hide a changed Schema literal."""
        assert fingerprint({"const": True}, {}) != fingerprint({"const": 1}, {})
        assert fingerprint({"const": False}, {}) != fingerprint({"const": 0}, {})

    def test_type_checking_imports_are_ignored_but_runtime_else_is_protected(
        self,
    ) -> None:
        """Ignore type-only dependencies without losing an executable else branch."""
        with tempfile.TemporaryDirectory() as temp:
            package = Path(temp)
            (package / "export").mkdir()
            reader = package / "export/reader.py"
            reader.write_text(
                "if TYPE_CHECKING:\n from sve_carddb.missing import Model\nelse:\n value = 1\n"
            )
            old = fingerprint(schema(), reader_sources(package))
            reader.write_text(
                "if TYPE_CHECKING:\n from sve_carddb.other_missing import Other\nelse:\n value = 1\n"
            )
            assert fingerprint(schema(), reader_sources(package)) == old
            reader.write_text(
                "if TYPE_CHECKING:\n from sve_carddb.missing import Model\nelse:\n value = 2\n"
            )
            assert fingerprint(schema(), reader_sources(package)) != old

    def test_package_initializers_and_relative_imports_are_contract_dependencies(
        self,
    ) -> None:
        """Follow implicit package execution and imported sibling modules."""
        with tempfile.TemporaryDirectory() as temp:
            package = Path(temp)
            (package / "export").mkdir()
            initializer = package / "__init__.py"
            initializer.write_text("BUCKETS = 64\n")
            (package / "export/reader.py").write_text("from . import join\n")
            join = package / "export/join.py"
            join.write_text("def merge(a, b): return a | b\n")
            old = fingerprint(schema(), reader_sources(package))
            initializer.write_text("BUCKETS = 32\n")
            assert fingerprint(schema(), reader_sources(package)) != old
            initializer.write_text("BUCKETS = 64\n")
            join.write_text("def merge(a, b): return b | a\n")
            assert fingerprint(schema(), reader_sources(package)) != old

    def test_generated_package_version_is_not_a_wire_definition(self) -> None:
        """A commit's regenerated distribution metadata must not invalidate its pin."""
        with tempfile.TemporaryDirectory() as temp:
            package = Path(temp)
            (package / "export").mkdir()
            (package / "__init__.py").write_text("from ._version import __version__\n")
            (package / "export/reader.py").write_text("def read(value): return value\n")
            version = package / "_version.py"
            version.write_text('__version__ = "0.1.dev1+first"\n')
            old = fingerprint(schema(), reader_sources(package))
            version.write_text('__version__ = "0.1.dev2+second"\n')
            assert fingerprint(schema(), reader_sources(package)) == old

    def test_reader_dependencies_cover_canonical_bucket_and_join_rules(self) -> None:
        """Runtime dependencies participate in the same lock as reader join rules."""
        with tempfile.TemporaryDirectory() as temp:
            package = Path(temp)
            (package / "export").mkdir()
            (package / "core").mkdir()
            reader = package / "export/reader.py"
            canonical = package / "core/json.py"
            reader.write_text(
                "from sve_carddb.core.json import encode\ndef join(a, b): return a | b\n"
            )
            canonical.write_text("def encode(value): return sorted(value)\n")
            sources = reader_sources(package)
            old = fingerprint(schema(), sources)
            check_lock(pin(old), {"4.0.0": old}, {})
            canonical.write_text("def encode(value): return list(value)\n")
            with self.assertRaisesRegex(ValueError, "--update"):
                check_lock(
                    pin(old),
                    {"4.0.0": fingerprint(schema(), reader_sources(package))},
                    {},
                )
            canonical.write_text("def encode(value): return sorted(value)\n")
            reader.write_text(
                "from sve_carddb.core.json import encode\ndef join(a, b): return b | a\n"
            )
            assert fingerprint(schema(), reader_sources(package)) != old

    def test_data_version_and_payload_content_are_not_contract_inputs(self) -> None:
        """Changing only snapshot values cannot invalidate a format lock."""
        with tempfile.TemporaryDirectory() as temp:
            package = Path(temp)
            (package / "export").mkdir()
            (package / "export/reader.py").write_text("def read(value): return value\n")
            payload = package / "snapshot.json"
            payload.write_text(
                '{"data_version":"20261018T000000Z-0001","rows":["first"]}'
            )
            locked = pin(fingerprint(schema(), reader_sources(package)))
            for contents in (
                '{"data_version":"20261018T000000Z-0002","rows":["first"]}',
                '{"data_version":"20261018T000000Z-0002","rows":["second"]}',
            ):
                payload.write_text(contents)
                check_lock(
                    locked,
                    {"4.0.0": fingerprint(schema(), reader_sources(package))},
                    {},
                )

    def test_malformed_frozen_flag_cannot_be_silently_rewritten(self) -> None:
        """Malformed lock records fail before the update path can overwrite them."""
        value = fingerprint(schema(), {})
        malformed: dict[str, JsonValue] = {
            "4.0.0": {"fingerprint": value, "frozen": "true"},
        }
        with self.assertRaisesRegex(ValueError, "Invalid format lock"):
            update_lock(malformed, {"4.0.0": value}, {})

    def test_base_lock_is_read_from_git_history_not_the_working_tree(self) -> None:
        """Read protected history, permit first introduction and reject a missing ref."""
        with tempfile.TemporaryDirectory() as temp:
            repository = Path(temp)
            git = shutil.which("git") or "git"
            git_env = {
                **{
                    key: value
                    for key, value in os.environ.items()
                    if not key.startswith("GIT_")
                },
                "GIT_CONFIG_GLOBAL": "/dev/null",
                "GIT_CONFIG_NOSYSTEM": "1",
            }

            def run(*args: str) -> None:
                subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git argv and isolated test repository
                    [
                        git,
                        "-c",
                        "core.hooksPath=/dev/null",
                        "-c",
                        "user.name=Synthetic Test",
                        "-c",
                        "user.email=test@example.invalid",
                        "-c",
                        "commit.gpgsign=false",
                        *args,
                    ],
                    cwd=repository,
                    env=git_env,
                    capture_output=True,
                    check=True,
                )

            run("init", "-q")
            run("commit", "--allow-empty", "-qm", "Before lock")
            assert baseline_lock("HEAD", repository) == {}
            with self.assertRaises(subprocess.CalledProcessError):
                baseline_lock("refs/heads/missing", repository)
            path = repository / LOCK_PATH
            path.parent.mkdir(parents=True)
            frozen = pin(fingerprint(schema(), {}), frozen=True)
            path.write_text(json.dumps(frozen))
            run("add", LOCK_PATH)
            run("commit", "-qm", "Freeze synthetic format")
            path.write_text("{}")
            assert baseline_lock("HEAD", repository) == frozen


if __name__ == "__main__":
    unittest.main()

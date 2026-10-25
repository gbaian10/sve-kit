# ruff: file-ignore[no-self-use, pytest-unittest-raises-assertion] -- repo CI uses standalone unittest with dev dependencies only
"""Exercise schema, profile and golden locking with isolated synthetic checkouts."""

import copy
import io
import json
import os
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- exercises real Git against isolated synthetic histories
import tempfile
import unittest
from collections.abc import Generator
from contextlib import contextmanager, redirect_stderr
from dataclasses import replace
from pathlib import Path

from pydantic import JsonValue
from sve_carddb.contracts.generate_schema import generate
from sve_carddb.contracts.profiles import PROFILES
from sve_carddb.core.json import object_value, parse

from snapshot_format import (
    LOCK_PATH,
    baseline_lock,
    check_lock,
    current_fingerprints,
    fingerprint,
    golden_hashes,
    main,
    profile_contract,
    update_lock,
)


def git(repository: Path, *args: str) -> None:
    """Use real Git without inheriting developer identity or hook configuration."""
    subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git argv and isolated test repository
        [
            shutil.which("git") or "git",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "user.name=Synthetic Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgsign=false",
            # Background auto maintenance can still write .git while the temporary repository is removed.
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
            *args,
        ],
        cwd=repository,
        env={
            **{
                key: value
                for key, value in os.environ.items()
                if not key.startswith("GIT_")
            },
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_CONFIG_NOSYSTEM": "1",
        },
        capture_output=True,
        check=True,
    )


@contextmanager
def checkout() -> Generator[tuple[Path, Path]]:
    """Provide a real base commit, packaged Schema and small shared golden."""
    with tempfile.TemporaryDirectory() as temp:
        repository = Path(temp)
        package = repository / "package/sve_carddb"
        resource = package / "contracts/schema/v3/contract.schema.json"
        resource.parent.mkdir(parents=True)
        resource.write_bytes(generate())
        golden = repository / "tests/fixtures/snapshot-contract/v3"
        golden.mkdir(parents=True)
        (golden / "manifest.json").write_text(
            '{"data_version":"20261018T000000Z-0001","rows":[]}'
        )
        (repository / LOCK_PATH).parent.mkdir(parents=True)
        git(repository, "init", "-q")
        git(repository, "commit", "--allow-empty", "-qm", "Before lock")
        yield repository, package


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
        new = fingerprint(schema(), {}, {"expected-logical.json": "changed"})
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

    def test_enum_and_profile_constants_require_update(self) -> None:
        """Schema enums, buckets, bands, minimum reader and capabilities are pinned."""
        selected = PROFILES[0]
        old = fingerprint(schema(), profile_contract(selected))
        changed = schema()
        definitions = changed["$defs"]
        assert isinstance(definitions, dict)
        definitions["Owner"] = {"enum": ["global", "family", "extra"]}
        changes = [fingerprint(changed, profile_contract(selected))]
        changes.extend(
            fingerprint(schema(), profile_contract(profile))
            for profile in (
                replace(selected, buckets=32),
                replace(selected, min_reader_version="4.1.0"),
                replace(selected, capabilities=(*selected.capabilities, "extra")),
                replace(selected, text_bands=replace(selected.text_bands, shared=4)),
                replace(selected, image_bands=replace(selected.image_bands, family=16)),
                replace(
                    selected, text_bands=replace(selected.text_bands, overrides=())
                ),
            )
        )
        for current in changes:
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

    def test_reader_local_variable_rename_does_not_change_format(self) -> None:
        """An unfrozen reader refactor needs no lock update or format bump."""
        self.check_reader_refactor(frozen=False)

    def test_frozen_reader_refactor_needs_no_format_bump(self) -> None:
        """A frozen format remains usable after the same pure reader refactor."""
        self.check_reader_refactor(frozen=True)

    def check_reader_refactor(self, *, frozen: bool) -> None:
        """Exercise the real command using reader code outside the fingerprint inputs."""
        with checkout() as (repository, package):
            (package / "export").mkdir()
            reader = package / "export/reader.py"
            reader.write_text("def read(value):\n payload = value\n return payload\n")
            current = current_fingerprints(repository, package)
            locked = pin(current["4.0.0"], frozen=frozen)
            path = repository / LOCK_PATH
            path.write_text(json.dumps(locked))
            git(repository, "add", LOCK_PATH)
            git(repository, "commit", "-qm", "Pin synthetic format")
            reader.write_text("def read(value):\n body = value\n return body\n")
            assert current_fingerprints(repository, package) == current
            assert (
                main(["--base-ref", "HEAD"], repository=repository, package=package)
                == 0
            )
            assert object_value(parse(path.read_bytes())) == locked

    def test_main_update_writes_and_preserves_freeze_status(self) -> None:
        """Update writes a new pin and retains an explicitly frozen matching entry."""
        with checkout() as (repository, package):
            path = repository / LOCK_PATH
            assert not path.exists()
            assert (
                main(
                    ["--update", "--base-ref", "HEAD"],
                    repository=repository,
                    package=package,
                )
                == 0
            )
            locked = object_value(parse(path.read_bytes()))
            check_lock(locked, current_fingerprints(repository, package), {})
            frozen = pin(
                current_fingerprints(repository, package)["4.0.0"], frozen=True
            )
            path.write_text(json.dumps(frozen))
            git(repository, "add", LOCK_PATH)
            git(repository, "commit", "-qm", "Freeze synthetic format")
            assert (
                main(
                    ["--update", "--base-ref", "HEAD"],
                    repository=repository,
                    package=package,
                )
                == 0
            )
            assert object_value(parse(path.read_bytes())) == frozen

    def test_main_rejects_committed_schema_that_differs_from_source(self) -> None:
        """A stale packaged Schema fails with regeneration instructions before writing."""
        with checkout() as (repository, package):
            path = package / "contracts/schema/v3/contract.schema.json"
            path.write_text('{"type":"null"}')
            with redirect_stderr(io.StringIO()) as output:
                assert (
                    main(
                        ["--update", "--base-ref", "HEAD"],
                        repository=repository,
                        package=package,
                    )
                    == 1
                )
            assert "Committed Schema differs from its source" in output.getvalue()
            assert "generate_schema" in output.getvalue()
            assert not (repository / LOCK_PATH).exists()

    def test_golden_data_version_changes_are_contract_changes(self) -> None:
        """The shared synthetic data version is pinned even though live versions are not."""
        with checkout() as (repository, package):
            current = current_fingerprints(repository, package)
            locked = pin(current["4.0.0"])
            path = repository / "tests/fixtures/snapshot-contract/v3/manifest.json"
            path.write_text(path.read_text().replace("0001", "0002"))
            changed = current_fingerprints(repository, package)
            with self.assertRaisesRegex(ValueError, "--update"):
                check_lock(locked, changed, {})
            check_lock(update_lock(locked, changed, {}), changed, {})
            with self.assertRaisesRegex(ValueError, "Bump format_version"):
                check_lock(pin(current["4.0.0"], frozen=True), changed, {})

    def test_live_data_version_and_payload_contents_are_not_contract_inputs(
        self,
    ) -> None:
        """Editing ordinary snapshot payloads never changes the contract pin."""
        with checkout() as (repository, package):
            current = current_fingerprints(repository, package)
            path = repository / "live-snapshot.json"
            for value in (
                '{"data_version":"20261018T000000Z-0001","rows":["first"]}',
                '{"data_version":"20261018T000000Z-0002","rows":["second"]}',
            ):
                path.write_text(value)
                assert current_fingerprints(repository, package) == current

    def test_golden_nested_file_additions_removals_and_content_are_pinned(self) -> None:
        """Hash the complete shared directory, including nested and non-JSON files."""
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            with self.assertRaisesRegex(ValueError, "golden files are missing"):
                golden_hashes(directory)
            path = directory / "payloads/vector.json"
            path.parent.mkdir()
            path.write_text("[]")
            old = golden_hashes(directory)
            path.write_text("[1]")
            assert golden_hashes(directory) != old
            path.write_text("[]")
            other = directory / "README.md"
            other.write_text("Synthetic contract")
            assert golden_hashes(directory) != old
            other.unlink()
            assert golden_hashes(directory) == old
            path.rename(path.with_name("renamed.json"))
            assert golden_hashes(directory) != old

    def test_malformed_frozen_flag_cannot_be_silently_rewritten(self) -> None:
        """Malformed lock records fail before the update path can overwrite them."""
        value = fingerprint(schema())
        malformed: dict[str, JsonValue] = {
            "4.0.0": {"fingerprint": value, "frozen": "true"}
        }
        with self.assertRaisesRegex(ValueError, "Invalid format lock"):
            update_lock(malformed, {"4.0.0": value}, {})

    def test_base_lock_is_read_from_git_history_not_the_working_tree(self) -> None:
        """Read protected history and explain how to obtain a missing base ref."""
        with checkout() as (repository, _package):
            assert baseline_lock("HEAD", repository) == {}
            with self.assertRaisesRegex(
                ValueError, "git fetch origin main.*--base-ref.*SVE_FORMAT_BASE_REF"
            ):
                baseline_lock("refs/heads/missing", repository)
            path = repository / LOCK_PATH
            frozen = pin(fingerprint(schema()), frozen=True)
            path.write_text(json.dumps(frozen))
            git(repository, "add", LOCK_PATH)
            git(repository, "commit", "-qm", "Freeze synthetic format")
            path.write_text("{}")
            assert baseline_lock("HEAD", repository) == frozen


if __name__ == "__main__":
    unittest.main()

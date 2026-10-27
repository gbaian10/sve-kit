"""Lock the snapshot schema and producer profile values."""

import argparse
import json
import os
import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- reads the committed lock from Git without changing the checkout
import sys
from dataclasses import asdict
from pathlib import Path

from pydantic import JsonValue
from sve_carddb.contracts import snapshot
from sve_carddb.contracts.generate_schema import generate
from sve_carddb.contracts.profiles import PROFILES, Profile
from sve_carddb.core.json import canonical, digest, object_value, parse

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = Path(snapshot.__file__).resolve().parents[1]
LOCK_PATH = ".github/ci/snapshot-format.lock"
GUIDANCE = "See snapshot-transport.md §1.1."


def schema_contract(value: JsonValue) -> JsonValue:
    """Discard schema annotations without discarding identically named wire fields."""
    if not isinstance(value, dict):
        return value
    result: dict[str, JsonValue] = {}
    for key, item in value.items():
        if key in {"$comment", "title", "description", "examples"}:
            continue
        if key in {"$defs", "properties", "patternProperties", "dependentSchemas"}:
            result[key] = {
                name: schema_contract(child)
                for name, child in object_value(item).items()
            }
        elif key in {"allOf", "anyOf", "oneOf", "prefixItems"}:
            if not isinstance(item, list):
                raise TypeError("Expected schema array")
            children = [schema_contract(child) for child in item]
            result[key] = (
                children if key == "prefixItems" else sorted(children, key=canonical)
            )
        elif key in {"enum", "required", "type"} and isinstance(item, list):
            result[key] = sorted(item, key=canonical)
        elif key in {
            "items",
            "additionalProperties",
            "unevaluatedProperties",
            "unevaluatedItems",
            "contains",
            "propertyNames",
            "not",
            "if",
            "then",
            "else",
        }:
            result[key] = schema_contract(item)
        else:
            # const and x-* contain literal wire descriptors, not schema keywords.
            result[key] = item
    return result


def profile_contract(selected: Profile) -> dict[str, JsonValue]:
    """Serialize the producer's actual constants without inspecting Python syntax."""
    return object_value(parse(json.dumps(asdict(selected)).encode("utf-8")))


def fingerprint(
    schema: JsonValue,
    profile_values: JsonValue = None,
) -> str:
    """Hash only the formal schema and producer profile values."""
    return digest(
        canonical(
            {
                "schema": schema_contract(schema),
                "profile": profile_values,
            }
        )
    )


def _validate_entries(entries: dict[str, JsonValue]) -> None:
    for version, raw in entries.items():
        entry = object_value(raw)
        if (
            re.fullmatch(
                r"(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)", version
            )
            is None
            or set(entry) != {"fingerprint", "frozen"}
            or type(entry["frozen"]) is not bool
            or not isinstance(entry["fingerprint"], str)
            or re.fullmatch(r"sha256:[0-9a-f]{64}", entry["fingerprint"]) is None
        ):
            raise ValueError(
                f"Invalid format lock entry {version}; expected fingerprint and boolean frozen"
            )


def check_lock(
    locked: dict[str, JsonValue],
    current: dict[str, str],
    baseline: dict[str, JsonValue],
) -> None:
    """Reject changed frozen history and any current format without a matching pin."""
    _validate_entries(locked)
    _validate_entries(baseline)
    for version, raw in baseline.items():
        entry = object_value(raw)
        if entry["frozen"] is True and locked.get(version) != entry:
            raise ValueError(
                f"Frozen format {version} cannot be changed, thawed or removed. "
                f"Bump format_version and retain its frozen lock entry. {GUIDANCE}"
            )
    for version, expected in current.items():
        raw = locked.get(version)
        entry = object_value(raw) if raw is not None else {}
        if entry.get("fingerprint") == expected and type(entry.get("frozen")) is bool:
            continue
        if entry.get("frozen") is True:
            raise ValueError(
                f"Wire contract changed for frozen format {version}. "
                f"Bump format_version according to the compatibility rules. {GUIDANCE}"
            )
        raise ValueError(
            f"Wire contract changed or is not locked for unfrozen format {version}. "
            "Update the lock in this change: "
            "uv --directory carddb run python ../.github/ci/snapshot_format.py --update. "
            f"{GUIDANCE}"
        )


def update_lock(
    locked: dict[str, JsonValue],
    current: dict[str, str],
    baseline: dict[str, JsonValue],
) -> dict[str, JsonValue]:
    """Refresh only unfrozen entries, retaining historical versions and freeze status."""
    _validate_entries(locked)
    updated = locked.copy()
    for version, expected in current.items():
        entry = object_value(locked[version]) if version in locked else {}
        if entry.get("frozen") is not True:
            updated[version] = {"fingerprint": expected, "frozen": False}
    check_lock(updated, current, baseline)
    return updated


def baseline_lock(ref: str, repository: Path = ROOT) -> dict[str, JsonValue]:
    """Read the protected lock from a base commit; allow its initial introduction."""
    git = shutil.which("git")
    if git is None:
        raise ValueError("Git is required to check frozen format history")

    def read(*args: str) -> bytes:
        return subprocess.run(  # ruff: ignore[subprocess-without-shell-equals-true] -- fixed Git executable and argv, read-only operations
            [git, *args],
            cwd=repository,
            check=True,
            capture_output=True,
        ).stdout

    # Validate ref even when this is the first PR introducing the lock.
    try:
        read("rev-parse", "--verify", ref + "^{commit}")
    except subprocess.CalledProcessError as error:
        raise ValueError(
            f"Git base ref {ref!r} was not found. Run git fetch origin main, "
            "or choose an existing ref with --base-ref or SVE_FORMAT_BASE_REF."
        ) from error
    if not read("ls-tree", "--name-only", ref, "--", LOCK_PATH).strip():
        return {}
    return object_value(parse(read("show", ref + ":" + LOCK_PATH)))


def current_fingerprints(
    package: Path = PACKAGE,
) -> dict[str, str]:
    """Require generated and packaged schemas to define the same current contracts."""
    current: dict[str, str] = {}
    for selected in PROFILES:
        generated = parse(generate(selected.version))
        resource = (
            package / "contracts/schema" / selected.resource / "contract.schema.json"
        )
        if fingerprint(parse(resource.read_bytes()), {}) != fingerprint(generated, {}):
            raise ValueError(
                "Committed Schema differs from its source. Regenerate it with "
                "uv --directory carddb run python -m sve_carddb.contracts.generate_schema"
            )
        current[selected.version] = fingerprint(generated, profile_contract(selected))
    return current


def main(
    argv: list[str] | None = None,
    *,
    repository: Path = ROOT,
    package: Path = PACKAGE,
) -> int:
    """Check or explicitly refresh the checkout's snapshot format lock."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true")
    parser.add_argument(
        "--base-ref", default=os.environ.get("SVE_FORMAT_BASE_REF", "origin/main")
    )
    args = parser.parse_args(argv)
    try:
        current = current_fingerprints(package)
        path = repository / LOCK_PATH
        locked = object_value(parse(path.read_bytes())) if path.exists() else {}
        baseline = baseline_lock(args.base_ref, repository)
        if args.update:
            locked = update_lock(locked, current, baseline)
            path.write_text(json.dumps(locked, indent=2) + "\n", encoding="utf-8")
        check_lock(locked, current, baseline)
    except (ValueError, TypeError, OSError, subprocess.CalledProcessError) as error:
        sys.stderr.write(f"Snapshot format lock: {error}\n")
        return 1
    sys.stdout.write("Snapshot format lock matches the wire contract.\n")
    return 0


if __name__ == "__main__":
    sys.exit(main())

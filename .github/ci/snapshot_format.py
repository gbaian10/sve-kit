"""Lock the snapshot schema and executable reference-reader contract."""

import argparse
import ast
import importlib.util
import json
import os
import re
import shutil
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- reads the committed lock from Git without changing the checkout
import sys
from pathlib import Path
from typing import override

from pydantic import JsonValue
from sve_carddb.contracts import snapshot
from sve_carddb.contracts.generate_schema import generate
from sve_carddb.contracts.profiles import PROFILES
from sve_carddb.core.json import canonical, digest, object_value, parse

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = Path(snapshot.__file__).resolve().parents[1]
LOCK_PATH = "carddb/src/sve_carddb/contracts/schema/format.lock.json"
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


class _Executable(ast.NodeTransformer):
    @override
    def visit_Expr(self, node: ast.Expr) -> ast.AST | None:
        if isinstance(node.value, ast.Constant) and isinstance(node.value.value, str):
            return None
        return self.generic_visit(node)

    @override
    def visit_If(self, node: ast.If) -> ast.AST | None:
        if isinstance(node.test, ast.Name) and node.test.id == "TYPE_CHECKING":
            return (
                self.generic_visit(ast.Module(body=node.orelse, type_ignores=[]))
                if node.orelse
                else None
            )
        return self.generic_visit(node)


def reader_contract(source: str) -> str:
    """Normalize executable syntax independently of comments and layout."""
    return ast.dump(_Executable().visit(ast.parse(source)), include_attributes=False)


def _imports(tree: ast.AST, package: Path, context: str) -> list[str]:
    result: list[str] = []
    for node in ast.walk(tree):
        if isinstance(node, ast.Import):
            result.extend(
                alias.name
                for alias in node.names
                if alias.name.startswith("sve_carddb.")
            )
        elif isinstance(node, ast.ImportFrom):
            module = node.module or ""
            if node.level:
                module = importlib.util.resolve_name("." * node.level + module, context)
            if module != "sve_carddb" and not module.startswith("sve_carddb."):
                continue
            result.append(module)
            for alias in node.names:
                child = package.joinpath(*module.split(".")[1:], alias.name)
                if child.is_dir() or child.with_suffix(".py").is_file():
                    result.append(module + "." + alias.name)
    return result


def reader_sources(package: Path) -> dict[str, str]:
    """Follow reference-reader runtime imports, including canonical and profile rules."""
    pending = ["sve_carddb.export.reader"]
    result: dict[str, str] = {}
    while pending:
        name = pending.pop()
        # Hatch's ignored build metadata changes on commits, not on wire changes.
        if name == "sve_carddb._version":
            continue
        if name in result:
            continue
        parent = name.rpartition(".")[0]
        if parent:
            pending.append(parent)
        path = package.joinpath(*name.split(".")[1:]).with_suffix(".py")
        if not path.is_file():
            directory = path.with_suffix("")
            path = directory / "__init__.py"
            if directory.is_dir() and not path.exists():
                result[name] = ""
                continue
        source = path.read_text(encoding="utf-8")
        result[name] = source
        tree = _Executable().visit(ast.parse(source))
        context = name if path.name == "__init__.py" else parent
        pending.extend(_imports(tree, package, context))
    return result


def fingerprint(schema: JsonValue, sources: dict[str, str]) -> str:
    """Hash formal definitions; snapshot payloads and data versions are never inputs."""
    return digest(
        canonical(
            {
                "schema": schema_contract(schema),
                "reader": {
                    name: reader_contract(source) for name, source in sources.items()
                },
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
    read("rev-parse", "--verify", ref + "^{commit}")
    if not read("ls-tree", "--name-only", ref, "--", LOCK_PATH).strip():
        return {}
    return object_value(parse(read("show", ref + ":" + LOCK_PATH)))


def current_fingerprints() -> dict[str, str]:
    """Require generated and packaged schemas to define the same current contracts."""
    sources = reader_sources(PACKAGE)
    current: dict[str, str] = {}
    for selected in PROFILES:
        generated = parse(generate(selected.version))
        resource = (
            PACKAGE / "contracts/schema" / selected.resource / "contract.schema.json"
        )
        if fingerprint(parse(resource.read_bytes()), {}) != fingerprint(generated, {}):
            raise ValueError(
                "Committed Schema differs from its source. Regenerate it with "
                "uv --directory carddb run python -m sve_carddb.contracts.generate_schema"
            )
        current[selected.version] = fingerprint(generated, sources)
    return current


def main(argv: list[str] | None = None) -> int:
    """Check or explicitly refresh the checkout's snapshot format lock."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--update", action="store_true")
    parser.add_argument(
        "--base-ref", default=os.environ.get("SVE_FORMAT_BASE_REF", "origin/main")
    )
    args = parser.parse_args(argv)
    try:
        current = current_fingerprints()
        path = ROOT / LOCK_PATH
        locked = object_value(parse(path.read_bytes())) if path.exists() else {}
        baseline = baseline_lock(args.base_ref)
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

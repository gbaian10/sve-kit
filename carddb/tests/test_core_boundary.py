"""Keep shared foundations independent of every higher project layer.

Only static imports are checked; importlib.import_module and __import__ are excluded.
"""

import ast
from importlib.util import resolve_name
from pathlib import Path

import pytest

CORE = Path(__file__).resolve().parents[1] / "src" / "sve_carddb" / "core"


def project_imports(source: str, package: str) -> set[str]:
    """Include nested, conditional and relative imports without executing them."""
    modules: set[str] = set()
    for node in ast.walk(ast.parse(source)):
        if isinstance(node, ast.Import):
            modules.update(alias.name for alias in node.names)
        elif isinstance(node, ast.ImportFrom):
            module = node.module or ""
            if node.level:
                module = resolve_name("." * node.level + module, package)
            modules.add(module)
            modules.update(module + "." + alias.name for alias in node.names)
    return {
        module
        for module in modules
        if module == "sve_carddb" or module.startswith("sve_carddb.")
    }


def forbidden_imports(source: str, package: str) -> set[str]:
    """Allow only imports within core, including package-level submodule syntax."""
    return {
        module
        for module in project_imports(source, package)
        if module not in {"sve_carddb", "sve_carddb.core"}
        and not module.startswith("sve_carddb.core.")
    }


@pytest.mark.parametrize("path", sorted(CORE.rglob("*.py")), ids=lambda path: path.name)
def test_core_imports_only_shared_foundations(path: Path) -> None:
    parts = path.relative_to(CORE.parent.parent).with_suffix("").parts
    package = ".".join(parts[:-1])
    assert not forbidden_imports(path.read_text(encoding="utf-8"), package)


@pytest.mark.parametrize(
    "source",
    [
        "import sve_carddb.snapshot.reader",
        "from sve_carddb.domains.registry import records",
        "from sve_carddb import registry",
        "def call():\n    from sve_carddb.build import Database",
        "if TYPE_CHECKING:\n    from sve_carddb.build import Value",
        "from ..registry import records",
        "from .. import registry",
    ],
)
def test_boundary_finds_reverse_dependencies(source: str) -> None:
    assert forbidden_imports(source, "sve_carddb.core")


@pytest.mark.parametrize(
    "source",
    [
        "import pathlib",
        "from pydantic import BaseModel",
        "from sve_carddb.core.json import canonical",
        "from sve_carddb.core import json",
        "from sve_carddb import core",
        "from . import json",
        "from .json import canonical",
    ],
)
def test_boundary_allows_core_and_external_dependencies(source: str) -> None:
    assert not forbidden_imports(source, "sve_carddb.core")

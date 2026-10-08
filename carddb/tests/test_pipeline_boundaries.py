"""Keep completed pipeline layers below workflow and CLI coordination.

Static AST imports include conditional and function-local imports. Dynamic
importlib and __import__ calls are outside this check.
"""

from pathlib import Path

import pytest

from .test_core_boundary import project_imports

PACKAGE = Path(__file__).resolve().parents[1] / "src/sve_carddb"
COMPLETED = ("core", "contracts", "ingest", "parse", "build", "snapshot")
PATHS = sorted(
    [path for name in COMPLETED for path in (PACKAGE / name).rglob("*.py")]
    + list(PACKAGE.glob("image_*.py"))
)
PARSE_ALLOWED = (
    "sve_carddb.core",
    "sve_carddb.contracts",
    "sve_carddb.parse",
    "sve_carddb.ingest.urls",
    "sve_carddb.ingest.archive.manifest",
    "sve_carddb.ingest.archive.store",
    "sve_carddb.ingest.http.validate",
)


def within(module: str, prefix: str) -> bool:
    return module == prefix or module.startswith(prefix + ".")


def forbidden_imports(source: str, package: str) -> set[str]:
    imports = project_imports(source, package)
    forbidden = {
        module
        for module in imports
        if any(
            within(module, prefix)
            for prefix in (
                "sve_carddb.workflows",
                "sve_carddb.cli",
                "sve_carddb.cli_paths",
            )
        )
    }
    if any(
        within(package, prefix)
        for prefix in (
            "sve_carddb.ingest.http",
            "sve_carddb.ingest.archive",
            "sve_carddb.ingest.queries",
        )
    ):
        forbidden.update(
            module for module in imports if within(module, "sve_carddb.ingest.crawl")
        )
    if within(package, "sve_carddb.parse"):
        forbidden.update(
            module
            for module in imports
            if module != "sve_carddb"
            and (
                within(module, "sve_carddb.core.authored")
                or (
                    module
                    not in {
                        "sve_carddb.ingest",
                        "sve_carddb.ingest.http",
                        "sve_carddb.ingest.archive",
                    }
                    and not any(within(module, prefix) for prefix in PARSE_ALLOWED)
                )
            )
        )
    return forbidden


@pytest.mark.parametrize("path", PATHS, ids=lambda p: str(p.relative_to(PACKAGE)))
def test_completed_pipeline_dependencies(path: Path) -> None:
    parts = path.relative_to(PACKAGE.parent).with_suffix("").parts
    package = ".".join(parts[:-1])
    assert not forbidden_imports(path.read_text(encoding="utf-8"), package)


@pytest.mark.parametrize(
    ("package", "source"),
    [
        ("sve_carddb.ingest.http", "from sve_carddb.ingest import crawl"),
        ("sve_carddb.ingest.archive", "from .. import crawl"),
        ("sve_carddb.ingest.archive", "from sve_carddb.workflows import extract"),
        ("sve_carddb.build", "from sve_carddb import cli"),
        ("sve_carddb.snapshot", "def call():\n    from ..workflows import offline"),
        (
            "sve_carddb.parse.pages",
            "if TYPE_CHECKING:\n    from sve_carddb.build import Database",
        ),
        ("sve_carddb.parse", "from sve_carddb.registry import inputs"),
        ("sve_carddb.parse", "from sve_carddb.translations import importer"),
        ("sve_carddb.parse", "from sve_carddb.core import authored"),
        ("sve_carddb.parse", "from sve_carddb.ingest.http import client"),
        ("sve_carddb.parse", "from sve_carddb import cli_paths"),
    ],
)
def test_boundary_detects_reverse_imports(package: str, source: str) -> None:
    assert forbidden_imports(source, package)


@pytest.mark.parametrize(
    ("package", "source"),
    [
        ("sve_carddb.parse", "from .pages import extract_en"),
        ("sve_carddb.parse", "from sve_carddb.ingest.http import validate"),
        ("sve_carddb.parse", "from sve_carddb.ingest import urls"),
        ("sve_carddb.parse", "from sve_carddb.ingest.http.validate import decode_html"),
        ("sve_carddb.parse", "from sve_carddb.ingest.archive.manifest import Region"),
        ("sve_carddb.ingest.archive", "from sve_carddb.parse.html import parse"),
        ("sve_carddb.ingest.http", "from sve_carddb.ingest.archive import manifest"),
        ("sve_carddb.build", "from sve_carddb.contracts import snapshot"),
    ],
)
def test_boundary_allows_shared_helpers(package: str, source: str) -> None:
    assert not forbidden_imports(source, package)

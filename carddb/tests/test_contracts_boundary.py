"""Protect shared contracts from build, domain and export dependencies."""

from pathlib import Path

import pytest

from .test_core_boundary import project_imports

CONTRACTS = Path(__file__).resolve().parents[1] / "src/sve_carddb/contracts"


def forbidden_imports(source: str, package: str) -> set[str]:
    return {
        module
        for module in project_imports(source, package)
        if module != "sve_carddb"
        and not any(
            module == allowed or module.startswith(allowed + ".")
            for allowed in ("sve_carddb.core", "sve_carddb.contracts")
        )
    }


@pytest.mark.parametrize("path", sorted(CONTRACTS.rglob("*.py")), ids=lambda p: p.name)
def test_contracts_import_only_core_and_contracts(path: Path) -> None:
    parts = path.relative_to(CONTRACTS.parent.parent).with_suffix("").parts
    assert not forbidden_imports(path.read_text(encoding="utf-8"), ".".join(parts[:-1]))


@pytest.mark.parametrize(
    "source",
    [
        "import sve_carddb.snapshot.reader",
        "from sve_carddb import build_db",
        "from .. import translations",
        "if TYPE_CHECKING:\n    from sve_carddb.build_db import Database",
        "def call():\n    from ..snapshot import reader",
    ],
)
def test_contracts_detect_reverse_imports(source: str) -> None:
    assert forbidden_imports(source, "sve_carddb.contracts")


@pytest.mark.parametrize(
    "source",
    [
        "from . import snapshot",
        "from sve_carddb import contracts",
        "from sve_carddb.core import json",
        "from pydantic import BaseModel",
    ],
)
def test_contracts_allow_shared_and_external_imports(source: str) -> None:
    assert not forbidden_imports(source, "sve_carddb.contracts")

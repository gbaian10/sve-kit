"""Public SVE fixtures retain their independently pinned field oracles."""

from pathlib import Path

import pytest

from ..support.private_pages_support import (
    Case,
    PrivateInputs,
    check_projection,
    load_index,
    project,
    read_inputs,
)

ROOT = Path(__file__).resolve().parents[1]
INDEX = load_index(ROOT / "fixtures/official-pages.json")


@pytest.fixture(scope="module")
def official_inputs() -> PrivateInputs:
    return PrivateInputs(read_inputs(INDEX, ROOT))


@pytest.mark.parametrize("case", INDEX.cases, ids=[case.id for case in INDEX.cases])
def test_official_page(case: Case, official_inputs: PrivateInputs) -> None:
    check_projection(case, project(case, official_inputs[case.id]))

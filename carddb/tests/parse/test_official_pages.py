"""Public SVE fixtures retain their independently pinned field oracles."""

from pathlib import Path

import pytest

from ..support.page_fixtures_support import (
    Case,
    FixtureInputs,
    check_projection,
    load_index,
    project,
    read_inputs,
)

ROOT = Path(__file__).resolve().parents[1]
INDEX = load_index(ROOT / "fixtures/official-pages.json")


@pytest.fixture(scope="module")
def official_inputs() -> FixtureInputs:
    return FixtureInputs(read_inputs(INDEX, ROOT))


@pytest.mark.parametrize("case", INDEX.cases, ids=[case.id for case in INDEX.cases])
def test_official_page(case: Case, official_inputs: FixtureInputs) -> None:
    check_projection(case, project(case, official_inputs[case.id]))

import os
from pathlib import Path

import pytest

from ..support.page_fixtures_support import (
    DIRECTORY_ENV,
    Case,
    FixtureInputs,
    PageFixtureError,
    check_projection,
    load_index,
    project,
    read_inputs,
)

pytestmark = pytest.mark.private_pages
INDEX = load_index()


@pytest.fixture(scope="module")
def private_inputs() -> FixtureInputs:
    directory = os.environ.get(DIRECTORY_ENV)
    if not directory:
        raise PageFixtureError("required page-fixture data directory is unset")
    return FixtureInputs(read_inputs(INDEX, Path(directory)))


@pytest.mark.parametrize("case", INDEX.cases, ids=[case.id for case in INDEX.cases])
def test_private_page(case: Case, private_inputs: FixtureInputs) -> None:
    check_projection(case, project(case, private_inputs[case.id]))

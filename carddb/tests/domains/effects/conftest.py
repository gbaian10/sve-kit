"""Share immutable effect authoring resources for the pytest session."""

from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.domains.effects.author_expansion import AuthorExpander

if TYPE_CHECKING:
    from sve_carddb.domains.effects.author_inputs import AuthorSchemas


@pytest.fixture(scope="session")
def expander() -> AuthorExpander:
    return AuthorExpander(Path(__file__).resolve().parents[4] / "dsl")


@pytest.fixture(scope="session")
def schemas(expander: AuthorExpander) -> AuthorSchemas:
    return expander.schemas

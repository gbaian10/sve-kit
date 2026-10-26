"""Share immutable effect authoring resources for the pytest session."""

from pathlib import Path
from types import MappingProxyType
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_set import AuthorInputSet, load_author_set

from ...support.author_current_fixtures import (
    CARD_NUMBERS,
    CurrentSourceFixture,
    current_source_fixture,
    entry_snapshot,
)
from ...support.author_damage_fixtures import damage_fixture
from ...support.author_destroy_fixtures import destroy_fixture
from ...support.author_move_fixtures import move_fixture
from ...support.author_numeric_fixtures import numeric_fixture
from ...support.current_keyword_fixtures import current_keyword_inputs

if TYPE_CHECKING:
    from collections.abc import Mapping

    from sve_carddb.domains.effects.author_inputs import AuthorSchemas
    from sve_carddb.domains.translations.inputs import Snapshot


def pytest_configure(config: pytest.Config) -> None:
    config.addinivalue_line(
        "markers",
        "slow(reason): required end-to-end coverage exceeding the time budget",
    )


@pytest.fixture(scope="session")
def expander() -> AuthorExpander:
    return AuthorExpander(Path(__file__).resolve().parents[4] / "dsl")


@pytest.fixture(scope="session")
def schemas(expander: AuthorExpander) -> AuthorSchemas:
    return expander.schemas


@pytest.fixture(scope="session")
def damage_current_fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return damage_fixture(
        tmp_path_factory.mktemp("damage-current"), preparation=author_source_preparation
    )


@pytest.fixture(scope="session")
def numeric_current_fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return numeric_fixture(
        tmp_path_factory.mktemp("numeric-current"),
        preparation=author_source_preparation,
    )


@pytest.fixture(scope="session")
def move_current_fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return move_fixture(
        tmp_path_factory.mktemp("move-current"), preparation=author_source_preparation
    )


@pytest.fixture(scope="session")
def destroy_current_fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return destroy_fixture(
        tmp_path_factory.mktemp("destroy-current"),
        preparation=author_source_preparation,
    )


@pytest.fixture(scope="session")
def base_current_fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return current_source_fixture(
        tmp_path_factory.mktemp("base-current"), preparation=author_source_preparation
    )


@pytest.fixture(scope="session")
def keyword_current_fixture(
    tmp_path_factory: pytest.TempPathFactory,
    author_source_preparation: bytes,
    keyword_registry_snapshots: Mapping[str, Snapshot],
) -> CurrentSourceFixture:
    return current_source_fixture(
        tmp_path_factory.mktemp("keyword-current"),
        card_numbers=CARD_NUMBERS | {"BP01-138"},
        snapshot=keyword_registry_snapshots["complete"],
        preparation=author_source_preparation,
    )


@pytest.fixture(scope="session")
def draw_current_fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return current_source_fixture(
        tmp_path_factory.mktemp("draw-current"),
        card_numbers=frozenset({"CP02-068", "BP10-122", "ECP01-002"}),
        preparation=author_source_preparation,
    )


@pytest.fixture(scope="session")
def search_current_fixture(
    tmp_path_factory: pytest.TempPathFactory, author_source_preparation: bytes
) -> CurrentSourceFixture:
    return current_source_fixture(
        tmp_path_factory.mktemp("search-current"),
        card_numbers=frozenset({"BP06-048", "BP06-P15"}),
        preparation=author_source_preparation,
    )


@pytest.fixture(scope="session", params=["BP01-138", "BP19-P32", "BP19-114"])
def keyword_handoff(
    request: pytest.FixtureRequest,
    tmp_path_factory: pytest.TempPathFactory,
    keyword_current_fixture: CurrentSourceFixture,
    destroy_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
) -> tuple[str, bytes]:
    number = str(request.param)
    fixture = (
        destroy_current_fixture
        if number.startswith("BP19-")
        else keyword_current_fixture
    )
    selection = current_keyword_inputs(
        tmp_path_factory.mktemp("keyword-handoff"),
        number,
        fixture=fixture,
        expander=expander,
    )
    return number, canonical(selection)


@pytest.fixture(scope="session")
def real_author_inputs(expander: AuthorExpander) -> AuthorInputSet:
    return load_author_set(
        Path(__file__).resolve().parents[4] / "authored", expander.schemas
    )


@pytest.fixture(scope="session")
def keyword_registry_snapshots() -> Mapping[str, Snapshot]:
    return MappingProxyType(
        {
            "complete": entry_snapshot(include_keywords=True),
            **{
                missing: entry_snapshot(
                    include_keywords=True, exclude_semantics=frozenset({missing})
                )
                for missing in (
                    "card_keywords.v2",
                    "metadata.layout.v1",
                    "evolve_entry.v1",
                )
            },
        }
    )

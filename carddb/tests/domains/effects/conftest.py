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
    CurrentSourceInputs,
    current_source_fixture,
    entry_snapshot,
)
from ...support.author_damage_fixtures import damage_fixture
from ...support.author_destroy_fixtures import destroy_fixture
from ...support.author_move_fixtures import move_fixture
from ...support.author_numeric_fixtures import numeric_fixture
from ...support.current_keyword_fixtures import (
    COMBAT_CARDS,
    current_combat_keyword_inputs,
    current_keyword_inputs,
)
from ...support.shared_current_fixtures import (
    CurrentFixtureCache,
    current_fixture_cache,
)
from ...support.time_budget import slow_fixture

if TYPE_CHECKING:
    from collections.abc import Iterator, Mapping

    from sve_carddb.domains.effects.author_inputs import AuthorSchemas
    from sve_carddb.domains.translations.inputs import Snapshot


@pytest.fixture(scope="session")
def expander() -> AuthorExpander:
    return AuthorExpander(Path(__file__).resolve().parents[4] / "dsl")


@pytest.fixture(scope="session")
def schemas(expander: AuthorExpander) -> AuthorSchemas:
    return expander.schemas


@pytest.fixture(scope="session")
def current_templates(
    tmp_path_factory: pytest.TempPathFactory,
    worker_id: str,
    current_source_preparation: CurrentSourceInputs,
) -> Iterator[CurrentFixtureCache]:
    root = tmp_path_factory.getbasetemp()
    if worker_id != "master":
        root = root.parent
    cache = root / "current-templates"
    cache.mkdir(exist_ok=True)
    with current_fixture_cache(cache, current_source_preparation) as templates:
        yield templates


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the damage real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def damage_current_fixture(
    current_templates: CurrentFixtureCache,
) -> CurrentSourceFixture:
    return current_templates.get(
        "damage",
        lambda store: damage_fixture(store, preparation=current_templates.inputs),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the numeric real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def numeric_current_fixture(
    current_templates: CurrentFixtureCache,
) -> CurrentSourceFixture:
    return current_templates.get(
        "numeric",
        lambda store: numeric_fixture(store, preparation=current_templates.inputs),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the move real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def move_current_fixture(
    current_templates: CurrentFixtureCache,
) -> CurrentSourceFixture:
    return current_templates.get(
        "move",
        lambda store: move_fixture(store, preparation=current_templates.inputs),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the destroy real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def destroy_current_fixture(
    current_templates: CurrentFixtureCache,
) -> CurrentSourceFixture:
    return current_templates.get(
        "destroy",
        lambda store: destroy_fixture(store, preparation=current_templates.inputs),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the base real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def base_current_fixture(
    current_templates: CurrentFixtureCache,
) -> CurrentSourceFixture:
    return current_templates.get(
        "base",
        lambda store: current_source_fixture(
            store, preparation=current_templates.inputs
        ),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the keyword real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def keyword_current_fixture(
    current_templates: CurrentFixtureCache,
    keyword_registry_snapshots: Mapping[str, Snapshot],
) -> CurrentSourceFixture:
    return current_templates.get(
        "keyword",
        lambda store: current_source_fixture(
            store,
            card_numbers=CARD_NUMBERS | {"BP01-138"},
            snapshot=keyword_registry_snapshots["complete"],
            preparation=current_templates.inputs,
        ),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the draw real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def draw_current_fixture(
    current_templates: CurrentFixtureCache,
) -> CurrentSourceFixture:
    return current_templates.get(
        "draw",
        lambda store: current_source_fixture(
            store,
            card_numbers=frozenset({"CP02-068", "BP10-122", "ECP01-002"}),
            preparation=current_templates.inputs,
        ),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the search real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def search_current_fixture(
    current_templates: CurrentFixtureCache,
) -> CurrentSourceFixture:
    return current_templates.get(
        "search",
        lambda store: current_source_fixture(
            store,
            card_numbers=frozenset({"BP06-048", "BP06-P15"}),
            preparation=current_templates.inputs,
        ),
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


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Build the combat real-current database template once across xdist workers from fixed public inputs; each test uses an independent database copy."
)
def combat_current_fixture(
    current_templates: CurrentFixtureCache,
    keyword_registry_snapshots: Mapping[str, Snapshot],
) -> CurrentSourceFixture:
    return current_templates.get(
        "combat",
        lambda store: current_source_fixture(
            store,
            card_numbers=frozenset(COMBAT_CARDS),
            snapshot=keyword_registry_snapshots["complete"],
            preparation=current_templates.inputs,
        ),
    )


@pytest.fixture(scope="session")
@slow_fixture(
    reason="Reconstruct all eleven real combat printings and their complete sibling handoffs once per session."
)
def combat_keyword_handoffs(
    tmp_path_factory: pytest.TempPathFactory,
    combat_current_fixture: CurrentSourceFixture,
    expander: AuthorExpander,
) -> bytes:
    return canonical(
        current_combat_keyword_inputs(
            tmp_path_factory.mktemp("combat-handoffs"),
            fixture=combat_current_fixture,
            expander=expander,
        )
    )

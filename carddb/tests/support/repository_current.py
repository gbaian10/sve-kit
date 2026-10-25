"""Read migrated repository cards against public inputs with the production parser."""

import sys
from pathlib import Path

from pydantic import JsonValue

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_current import CurrentAuthorSources
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_set import load_author_set

from .author_current_fixtures import REPOSITORY
from .author_damage_fixtures import damage_fixture


def selection_input(root: Path) -> JsonValue:
    expander = AuthorExpander(REPOSITORY / "dsl")
    inputs = load_author_set(REPOSITORY / "authored", expander.schemas)
    numbers: set[str] = set()
    for file in inputs.author_files:
        numbers.update(
            object_value(object_value(expander.schemas.author_tree(file.raw))["cards"])
        )
    # Identity covers every printing; limiting text sources to author cards keeps
    # unrelated prototype cards' scoped source corrections (pinned images) out.
    fixture = damage_fixture(
        root / "sources",
        card_numbers=frozenset(numbers),
        parser_version="official-jp-exact-v1",
    )
    with fixture.template.copy() as db:
        current = CurrentAuthorSources(
            db,
            fixture.sources,
            fixture.snapshot,
            fixture.vocabulary,
            fixture.rules,
            fixture.jp_batch_id,
        )
        return current.selection_input(REPOSITORY / "authored", expander)


if __name__ == "__main__":
    output = Path(sys.argv[1])
    output.write_bytes(canonical(selection_input(output.parent)))

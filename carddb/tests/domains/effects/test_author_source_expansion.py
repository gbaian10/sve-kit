"""Expanded-source proof requires reconstruction, even for valid kernel candidates."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_expansion import AuthorExpander, ExpansionError
from tests.support.author_source_fixtures import SourceCandidate, source_context

from .test_author_entries import SCHEMAS, ability, document, shard
from .test_author_expansion import generated, registered
from .test_author_sources import entry_case

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_sources import SourceContext
    from sve_carddb.domains.effects.author_validation import PathParts

ABILITY_PATH = ("cards", "FIX-001", "abilities", 0)


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


def entry_input(
    *, companion: bool
) -> tuple[dict[str, JsonValue], list[bytes], SourceCandidate]:
    candidate, use = entry_case()
    resolved = source_context(candidate, use)
    head: dict[str, JsonValue] = {"evolve": {"cost": {"pp": 99}}}
    inputs: list[bytes] = []
    if companion:
        record = registered(head)
        record["frame_id"] = resolved.frame.id
        entry = generated(record)
        inputs.append(canonical(shard(record)))
    else:
        entry = ability(head)
    entry["sources"] = [use.model_dump(mode="json")]
    return entry, inputs, candidate


@pytest.mark.parametrize("mapping", ["missing", "extra", "wrong_path"])
def test_source_candidate_paths_are_an_exact_set(
    expander: AuthorExpander, mapping: str
) -> None:
    entry, inputs, candidate = entry_input(companion=False)
    candidates: dict[PathParts, SourceContext] = {
        ABILITY_PATH: source_context(candidate)
    }
    unknown = ("cards", "FIX-001", "abilities", 1)
    if mapping != "missing":
        candidates[unknown] = source_context(candidate)
    if mapping != "extra":
        del candidates[ABILITY_PATH]
    with pytest.raises(ExpansionError, match="source_candidates_mismatch"):
        expander.expand_contexts(canonical(document(entry)), inputs, candidates)


def test_nonempty_author_exports_have_no_source_mapping(
    expander: AuthorExpander,
) -> None:
    candidate, use = entry_case()
    entry = ability({"spell": {"do": [{"draw": {"n": 1, "result": "drawn"}}]}})
    entry["sources"] = [use.model_dump(mode="json")]
    entry["ports"] = {
        "imports": {},
        "exports": {"receipt": {"path": ["spell", "do", 0, "draw"], "bind": "drawn"}},
    }
    raw = canonical(document(entry))
    assert expander.expand(raw, []).sidecar[0].source_proof == "none"
    with pytest.raises(ExpansionError, match="port"):
        expander.expand_contexts(
            raw, [], {ABILITY_PATH: source_context(candidate, use)}
        )

"""Expanded-source proof requires reconstruction, even for valid kernel candidates."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.domains.effects.author_expansion import (
    AuthorExpander,
    ExpansionError,
    ability_nodes,
    body_version,
    equivalent_ability,
)
from sve_carddb.domains.effects.author_sites import SITE_RULES
from sve_carddb.domains.effects.author_sources import (
    SOURCE_SITES,
    SourceError,
    resolve_source,
)

from .author_source_cases import synthetic_offset
from .test_author_entries import SCHEMAS, ability, document, shard
from .test_author_expansion import generated, registered
from .test_author_sources import entry_case

if TYPE_CHECKING:
    from pydantic import JsonValue

    from sve_carddb.domains.effects.author_sources import SourceCandidate
    from sve_carddb.domains.effects.author_validation import PathParts

ABILITY_PATH = ("cards", "FIX-001", "abilities", 0)


@pytest.fixture(scope="module")
def expander() -> AuthorExpander:
    return AuthorExpander(SCHEMAS)


def entry_input(
    *, companion: bool
) -> tuple[dict[str, JsonValue], list[bytes], SourceCandidate]:
    candidate, use = entry_case()
    resolved = resolve_source(candidate, use)
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


@pytest.mark.parametrize("companion", [False, True])
def test_resolved_entry_proof_is_separate_from_expanded_tree(
    expander: AuthorExpander, *, companion: bool
) -> None:
    entry, inputs, candidate = entry_input(companion=companion)
    raw = canonical(document(entry))
    kernel = expander.expand(raw, inputs)
    resolved = expander.expand_sources(raw, inputs, {ABILITY_PATH: candidate})
    assert resolved.tree == kernel.tree
    assert kernel.sidecar[0].source_proof == "none"
    assert resolved.sidecar[0].source_proof == "derived_self_consistent"
    assert resolved.sidecar[0].source_input_hash == digest(
        candidate.raw.encode("utf-8")
    )
    assert resolved.sidecar[0].path == ABILITY_PATH
    assert resolved.sidecar[0].frame_id is not None
    assert resolved.sidecar[0].interface_key is not None
    assert resolved.sidecar[0].adapter_recipe == expander.adapter_recipe
    assert resolved.sidecar[0].body_version == kernel.sidecar[0].body_version
    assert equivalent_ability(next(ability_nodes(resolved.tree))[1]) == (
        b'{"evolve":{"cost":{"pp":99}},"ports":{"exports":{},"imports":{}}}'
    )
    assert "source_proof" not in next(ability_nodes(resolved.tree))[1]


def test_resolved_inline_and_companion_use_same_source_interface(
    expander: AuthorExpander,
) -> None:
    results = []
    for companion in (False, True):
        entry, inputs, candidate = entry_input(companion=companion)
        results.append(
            expander.expand_sources(
                canonical(document(entry)), inputs, {ABILITY_PATH: candidate}
            )
        )
    assert results[0].tree == results[1].tree
    assert results[0].sidecar[0].frame_id == results[1].sidecar[0].frame_id
    assert results[0].sidecar[0].interface_key == results[1].sidecar[0].interface_key
    assert results[0].sidecar[0].body_version is None
    assert results[1].sidecar[0].body_version is not None


@pytest.mark.parametrize("mapping", ["missing", "extra", "wrong_path"])
def test_source_candidate_paths_are_an_exact_set(
    expander: AuthorExpander, mapping: str
) -> None:
    entry, inputs, candidate = entry_input(companion=False)
    candidates: dict[PathParts, SourceCandidate] = {ABILITY_PATH: candidate}
    unknown = ("cards", "FIX-001", "abilities", 1)
    if mapping != "missing":
        candidates[unknown] = candidate
    if mapping != "extra":
        del candidates[ABILITY_PATH]
    with pytest.raises(ExpansionError, match="source_candidates_mismatch"):
        expander.expand_sources(canonical(document(entry)), inputs, candidates)


@pytest.mark.parametrize("companion", [False, True])
def test_source_scope_cannot_be_fabricated_by_author_metadata(
    expander: AuthorExpander, *, companion: bool
) -> None:
    entry, _, candidate = entry_input(companion=False)
    object_value(array(entry["sources"])[0])["ability_scope"] = "missing"
    inputs: list[bytes] = []
    if companion:
        record = registered({"evolve": {"cost": {"pp": 99}}})
        record["ability_scope"] = "missing"
        record["body_version"] = body_version(record)
        generated_entry = generated(record)
        generated_entry["sources"] = entry["sources"]
        entry = generated_entry
        inputs = [canonical(shard(record))]
    raw = canonical(document(entry))
    assert expander.expand(raw, inputs).sidecar[0].source_proof == "none"
    with pytest.raises(SourceError, match="source_ability_scope_mismatch"):
        expander.expand_sources(raw, inputs, {ABILITY_PATH: candidate})


def test_selected_companion_frame_must_match_reconstructed_frame(
    expander: AuthorExpander,
) -> None:
    candidate, use = entry_case()
    record = registered({"evolve": {"cost": {"pp": 99}}})
    record["frame_id"] = "frame:" + "f" * 64
    entry = generated(record)
    entry["sources"] = [use.model_dump(mode="json")]
    raw, inputs = canonical(document(entry)), [canonical(shard(record))]
    assert expander.expand(raw, inputs).sidecar[0].source_proof == "none"
    with pytest.raises(ExpansionError, match="source_frame_mismatch"):
        expander.expand_sources(raw, inputs, {ABILITY_PATH: candidate})


def test_source_binding_is_exact_even_without_parameters(
    expander: AuthorExpander,
) -> None:
    entry, inputs, candidate = entry_input(companion=False)
    object_value(array(entry["sources"])[0])["binding_id"] = "bind:" + "0" * 64
    with pytest.raises(SourceError, match="source_occurrence_mismatch"):
        expander.expand_sources(
            canonical(document(entry)), inputs, {ABILITY_PATH: candidate}
        )


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
        expander.expand_sources(raw, [], {ABILITY_PATH: candidate})


def test_synthetic_resolved_contract_cannot_supply_expansion_proof(
    expander: AuthorExpander,
) -> None:
    sample = synthetic_offset()
    assert sample.compatible().source_proof == "none"
    record = registered(
        {
            "spell": {
                "define": {
                    "X": {
                        "minus": [
                            {"count": {"who": "self", "zone": "deck"}},
                            {"param": "offset"},
                        ]
                    }
                },
                "do": [{"draw": {"n": "X"}}],
            }
        }
    )
    record["frame_id"] = sample.frame.id
    record["params"] = {"offset": sample.declaration().model_dump(mode="json")}
    record["body_version"] = body_version(record)
    entry = generated(record, {"offset": {"type": "Int", "value": 5}})
    entry["sources"] = [sample.use().model_dump(mode="json")]
    raw, inputs = canonical(document(entry)), [canonical(shard(record))]
    assert expander.expand(raw, inputs).sidecar[0].source_proof == "none"
    with pytest.raises(SourceError, match="source_pending"):
        expander.expand_sources(raw, inputs, {ABILITY_PATH: sample.candidate})


def test_source_signatures_match_kernel_declarations() -> None:
    expected = {
        (rule.owner, rule.path): (
            rule.type,
            rule.source_type,
            rule.role,
            rule.component,
        )
        for rule in SITE_RULES
        if rule.type not in {None, "Concept", "CardKind", "Phase"}
        and rule.component != "negated_magnitude"
    }
    assert expected == SOURCE_SITES

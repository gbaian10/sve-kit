"""Printed Lastword has a closed adapter, separate from current-card proof."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.contracts.author_leaves import ParamDecl, ParamSource
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    entry_payload,
    parameter_expected,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_damage import KEY
from sve_carddb.domains.translations.four_layer_derivation import derive_source

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_damage import EXTRA, TERM
from .test_author_draw_admission import frozen_context


@pytest.mark.parametrize("card_no", ["BP01-P25", "BP01-121"])
def test_printed_lastword_resolves_without_current_proof(card_no: str) -> None:
    raw, context = frozen_context(
        card_no, "3aeefc3819be4412d9e8891136fd813bbb27ce55011343f51d38289d4ee1d196"
    )
    assert (context.card_kind, context.phase, context.section_count) == (
        "follower",
        "normal",
        0,
    )
    context = replace(context, special_kinds=frozenset())
    engine = classifier((TERM,), EXTRA)
    derived = derive_source(raw, context.source, engine, context=context)
    assert len(derived.recognized) == 1
    frame, binding = derived.bind(0)
    assert frame.semantic_variant.key == KEY
    assert binding.values["leaf_1"] == 2
    use = SourceUse(
        binding_id=binding.id,
        occurrence=binding.occurrence_key(),
        ability_scope="ability_0",
    )
    checked = resolve_source(SourceCandidate(raw, context.source, engine, context), use)
    # Frozen wording and synthetic identities cannot prove current whole-card admission.
    assert checked.source_proof == "derived_self_consistent"
    assert entry_payload(checked, "ability_0") == {
        "lastword": {"do": [{"damage": {"to": {"leaders": "all"}, "n": 2}}]},
        "ports": {"imports": {}, "exports": {}},
    }
    declaration = ParamDecl(
        type="Nat", source=ParamSource(slot="leaf_1", component="value")
    )
    assert (
        parameter_expected(
            checked, declaration, "A.act.damage", ("n",), {"n": {"param": "n"}}
        )
        == 2
    )

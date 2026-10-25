"""Real Advance destroy sources resolve to the fixed author Fanfare payload."""

from dataclasses import replace

import pytest

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.domains.effects.author_sources import (
    SourceCandidate,
    entry_payload,
    resolve_source,
)
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_destroy import KEY

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_destroy import EXTRA, TERMS
from .test_author_draw_admission import frozen_context


@pytest.mark.parametrize("card_no", ["BP19-P32", "BP19-114"])
def test_real_advance_fanfare_resolves_to_fixed_payload(card_no: str) -> None:
    raw, context = frozen_context(
        card_no, "ac6120f93e44b52c4e69f413ed4717b87c86f58b1105b9eb81f038ebd28522ad"
    )
    assert (context.card_kind, context.phase, context.section_count) == (
        "follower",
        "advance",
        0,
    )
    context = replace(context, special_kinds=frozenset({"advance"}))
    engine = classifier(TERMS, EXTRA)
    derived = derive_source(raw, context.source, engine, context=context)
    assert [derived.bind(n)[0].semantic_variant.key for n in range(2)] == [
        "card_keywords.v2",
        "metadata.layout.v1",
    ]
    frame, binding = derived.bind(2)
    assert frame.semantic_variant.key == KEY
    use = SourceUse(
        binding_id=binding.id,
        occurrence=binding.occurrence_key(),
        ability_scope="ability_0",
    )
    checked = resolve_source(SourceCandidate(raw, context.source, engine, context), use)
    assert checked.source_proof == "derived_self_consistent"
    assert entry_payload(checked, "ability_0") == {
        "fanfare": {
            "do": [
                {
                    "destroy": {
                        "ref": {"who": "opp", "zone": "field", "kind": "follower"}
                    }
                }
            ]
        },
        "ports": {"imports": {}, "exports": {}},
    }

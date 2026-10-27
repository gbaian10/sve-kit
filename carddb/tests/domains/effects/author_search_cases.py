"""Closed named-search bodies and explicitly synthetic source identities."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_expansion import body_version
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_fixed_actions import SEARCH
from tests.support.author_source_fixtures import SourceCandidate

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_fixed_actions import EXTRA, SEARCH_RAW, TERMS, facts
from .test_author_entries import ability, shard
from .test_author_expansion import generated, registered

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame

PATH = ("cards", "FIX-001", "abilities", 0)


def head(count: JsonValue = 1) -> dict[str, JsonValue]:
    return {
        "lastword": {
            "do": [
                {
                    "search": {
                        "sel": {"who": "self", "zone": "deck", "name": "式神の使役"},
                        "n": count,
                        "to": "hand",
                    }
                }
            ]
        }
    }


def search_source(raw: str = SEARCH_RAW) -> tuple[SourceCandidate, SourceUse, Frame]:
    context = facts(raw, SEARCH)
    engine = classifier(TERMS, EXTRA)
    derived = derive_source(raw, context.source, engine, context=context)
    frame, binding = derived.bind(0)
    return (
        SourceCandidate(raw, context.source, engine, context),
        SourceUse(
            binding_id=binding.id,
            occurrence=binding.occurrence_key(),
            ability_scope="ability_0",
        ),
        frame,
    )


def search_ability(
    frame: Frame, use: SourceUse, *, companion: bool, count: int = 1
) -> tuple[dict[str, JsonValue], list[bytes]]:
    if not companion:
        return ability(head(count)) | {"sources": [use.model_dump(mode="json")]}, []
    record = registered(head({"param": "n"}))
    record["frame_id"] = frame.id
    record["params"] = {
        "n": {"type": "Nat", "source": {"slot": "leaf_4", "component": "constant"}}
    }
    record["body_version"] = body_version(record)
    entry = generated(record, {"n": {"type": "Nat", "value": count}})
    entry["sources"] = [use.model_dump(mode="json")]
    return entry, [canonical(shard(record))]

"""A closed PP1 activation keeps its unresolved leave-field sibling."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_fixed_actions import MOVE
from tests.support.author_source_fixtures import SourceCandidate

from ..translations.test_four_layer_classification import classifier
from ..translations.test_four_layer_fixed_actions import EXTRA, MOVE_RAW, TERMS, facts
from .test_author_entries import ability, shard
from .test_author_expansion import generated, registered

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame

PATH = ("cards", "FIX-001", "abilities", 0)


def head() -> dict[str, JsonValue]:
    return {
        "activate": {
            "zone": "field",
            "cost": {"pp": 1},
            "do": [{"move": {"ref": "this", "to": "cemetery"}}],
        }
    }


def move_source(raw: str = MOVE_RAW) -> tuple[SourceCandidate, SourceUse, Frame]:
    context = facts(raw, MOVE)
    engine = classifier(TERMS, EXTRA)
    derived = derive_source(raw, context.source, engine, context=context)
    index = len(derived.field.parts) - 1
    frame, binding = derived.bind(index)
    return (
        SourceCandidate(raw, context.source, engine, context),
        SourceUse(
            binding_id=binding.id,
            occurrence=binding.occurrence_key(),
            ability_scope="ability_0",
        ),
        frame,
    )


def move_ability(
    frame: Frame,
    use: SourceUse,
    *,
    companion: bool,
    payload: dict[str, JsonValue] | None = None,
) -> tuple[dict[str, JsonValue], list[bytes]]:
    payload = head() if payload is None else payload
    if companion:
        record = registered(payload)
        record["frame_id"] = frame.id
        entry, bodies = generated(record), [canonical(shard(record))]
    else:
        entry, bodies = ability(payload), []
    entry["line"] = 2
    entry["sources"] = [use.model_dump(mode="json")]
    return entry, bodies

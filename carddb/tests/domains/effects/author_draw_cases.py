"""Closed spell bodies and source candidates for author admission tests."""

from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.contracts.author_evidence import SourceUse
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_expansion import body_version
from sve_carddb.domains.effects.author_sources import SourceCandidate
from sve_carddb.domains.translations.four_layer_derivation import derive_source
from sve_carddb.domains.translations.four_layer_semantics import CardContext

from ..translations.test_four_layer_classification import classifier, source
from .test_author_entries import ability, shard
from .test_author_expansion import generated, registered

if TYPE_CHECKING:
    from sve_carddb.contracts.four_layer import Frame

PATH = ("cards", "FIX-001", "abilities", 0)


def head(count: JsonValue = 3) -> dict[str, JsonValue]:
    return {"spell": {"do": [{"draw": {"who": "self", "n": count}}]}}


def spell_source(raw: str = "3枚引く。") -> tuple[SourceCandidate, SourceUse, Frame]:
    descriptor = source(raw)
    engine = classifier()
    context = CardContext(
        descriptor, "c:synthetic", "f:synthetic", "normal", "spell", True, 0
    )
    candidate = SourceCandidate(raw, descriptor, engine, context)
    derived = derive_source(raw, descriptor, engine, context=context)
    frame, binding = derived.bind(0)
    return (
        candidate,
        SourceUse(
            binding_id=binding.id,
            occurrence=binding.occurrence_key(),
            ability_scope="ability_0",
        ),
        frame,
    )


def spell_ability(
    frame: Frame, use: SourceUse, *, companion: bool, count: int = 3
) -> tuple[dict[str, JsonValue], list[bytes]]:
    if not companion:
        return ability(head(count)) | {"sources": [use.model_dump(mode="json")]}, []
    record = registered(head({"param": "n"}))
    record["frame_id"] = frame.id
    record["params"] = {
        "n": {"type": "Nat", "source": {"slot": "leaf_0", "component": "value"}}
    }
    record["body_version"] = body_version(record)
    entry = generated(record, {"n": {"type": "Nat", "value": count}})
    entry["sources"] = [use.model_dump(mode="json")]
    return entry, [canonical(shard(record))]

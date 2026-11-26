"""Python types for the stored author index and body parameter declarations."""

from typing import Annotated, Literal

from pydantic import BeforeValidator, Field

from sve_carddb.contracts.four_layer import Code
from sve_carddb.core.json import integer
from sve_carddb.core.models import RecordData, Text

ParamType = Literal[
    "Int", "Nat", "CardName", "Concept", "CardKind", "Player", "Zone", "Phase"
]


class ParamSource(RecordData):
    slot: Code
    component: Literal["value", "constant", "negated_magnitude"]


CardId = Annotated[str, Field(pattern=r"^c:[0-9a-f]{32}$(?![\s\S])")]
RelativeFile = Annotated[
    str,
    Field(
        pattern=r"^(?!/)(?![\s\S]*\\)(?![\s\S]*[\x00-\x1f\x7f])(?!(?:[^/]+/)*\.{1,2}(?:/|$))[^/]+(?:/[^/]+)*$(?![\s\S])"
    ),
]
Scope = Literal["shared", "en_override"]


class PrototypeIndexRecord(RecordData):
    card_id: CardId
    scope: Literal["shared"]
    file: RelativeFile
    kind: Literal["effect_set"]
    record_key: Text


class AuthorIndexRecord(RecordData):
    card_id: CardId
    scope: Scope
    file: RelativeFile
    kind: Literal["author_effect_set"]
    record_key: Text


type IndexRecord = PrototypeIndexRecord | AuthorIndexRecord


class EffectsIndex(RecordData):
    format: Annotated[Literal[1], BeforeValidator(integer)]
    kind: Literal["effects_index"]
    records: tuple[IndexRecord, ...]

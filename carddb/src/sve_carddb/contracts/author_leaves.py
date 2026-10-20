"""Python leaf types corresponding to the author schema; not execution admission."""

from typing import Annotated, Literal

from pydantic import Field

from sve_carddb.contracts.four_layer import Code
from sve_carddb.core.models import RecordData, Text, UInt

Int = Annotated[int, Field(ge=-9007199254740991, le=9007199254740991)]
ParamName = Annotated[str, Field(pattern=r"^[A-Za-z][A-Za-z0-9_]*$(?![\s\S])")]
Player = Literal["self", "opp"]
Zone = Literal["field", "hand", "deck", "evolve_deck", "cemetery", "ex"]
Phase = Literal["start", "main", "end"]
ParamType = Literal[
    "Int", "Nat", "CardName", "Concept", "CardKind", "Player", "Zone", "Phase"
]


class ParamSource(RecordData):
    slot: Code
    component: Literal["value", "constant", "negated_magnitude"]


class ParamDecl(RecordData):
    type: ParamType
    source: ParamSource


class ParamRef(RecordData):
    param: ParamName


class IntParamValue(RecordData):
    type: Literal["Int"]
    value: Int


class NatParamValue(RecordData):
    type: Literal["Nat"]
    value: UInt


class CardNameParamValue(RecordData):
    type: Literal["CardName"]
    value: Text


class ConceptParamValue(RecordData):
    type: Literal["Concept"]
    value: Text


class CardKindParamValue(RecordData):
    type: Literal["CardKind"]
    value: Code


class PlayerParamValue(RecordData):
    type: Literal["Player"]
    value: Player


class ZoneParamValue(RecordData):
    type: Literal["Zone"]
    value: Zone


class PhaseParamValue(RecordData):
    type: Literal["Phase"]
    value: Phase


type ParamValue = (
    IntParamValue
    | NatParamValue
    | CardNameParamValue
    | ConceptParamValue
    | CardKindParamValue
    | PlayerParamValue
    | ZoneParamValue
    | PhaseParamValue
)
type ParamDeclarations = dict[ParamName, ParamDecl]
type ParamValues = dict[ParamName, ParamValue]

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
    format: Literal[1]
    kind: Literal["effects_index"]
    records: tuple[IndexRecord, ...]

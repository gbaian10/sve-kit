"""Quantity positions share operations while keeping literal and parameter domains."""

from typing import Annotated

from pydantic import Field

from sve_carddb.contracts.author_expressions import NumericOperation
from sve_carddb.contracts.author_leaves import ParamRef
from sve_carddb.contracts.author_references import PlayerRef, PlayerSet
from sve_carddb.core.models import RecordData, UInt

type Quantity = Annotated[UInt, Field(strict=True)] | NumericOperation
type SelectionQuantity = Quantity | ParamRef


class DeckSlice(RecordData):
    who: PlayerRef | PlayerSet
    n: Quantity

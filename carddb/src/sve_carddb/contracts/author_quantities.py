"""Quantity positions share operations while keeping literal and parameter domains."""

from sve_carddb.contracts.author_leaves import ParamRef
from sve_carddb.contracts.author_queries import Quantity
from sve_carddb.contracts.author_references import PlayerRef, PlayerSet
from sve_carddb.core.models import RecordData

type SelectionQuantity = Quantity | ParamRef


class DeckSlice(RecordData):
    who: PlayerRef | PlayerSet
    n: Quantity

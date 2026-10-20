"""Receipt snapshots are immutable leaves of the recursive author query graph."""

from typing import Literal

from sve_carddb.contracts.author_references import ReceiptRef
from sve_carddb.core.models import RecordData


class ExprObjectsOf(RecordData):
    objects_of: ReceiptRef
    view: Literal["before", "after"]


type SnapshotRead = ExprObjectsOf


class SameAs(RecordData):
    same_as: SnapshotRead

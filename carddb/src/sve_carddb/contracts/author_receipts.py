"""Receipt read shapes; binding types and success predicates require lowering."""

from typing import Annotated, Literal, TypedDict

from pydantic import AfterValidator, ConfigDict, with_config

from sve_carddb.contracts.author_leaves import Zone  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_references import ReceiptRef
from sve_carddb.core.models import RecordData, Text, UInt


class ExprObjectsOf(RecordData):
    objects_of: ReceiptRef
    view: Literal["before", "after"]


type SnapshotRead = ExprObjectsOf


class SameAs(RecordData):
    same_as: SnapshotRead


@with_config(
    ConfigDict(
        extra="forbid",
        strict=True,
        regex_engine="python-re",
        json_schema_extra={"minProperties": 1},
    )
)
class MaterialFilterFields(TypedDict, total=False):
    name: Text
    zone: Zone
    orig_cost_eq: UInt


def _nonempty_material(value: MaterialFilterFields) -> MaterialFilterFields:
    if not value:
        raise ValueError("Material filter requires an attribute")
    return value


# Optional attributes must stay absent, not become nullable model defaults.
type MaterialFilter = Annotated[
    MaterialFilterFields, AfterValidator(_nonempty_material)
]


class CountOfFiltered(RecordData):
    receipt: ReceiptRef
    where: MaterialFilter
    view: Literal["before", "after"]


class ExprCountOf(RecordData):
    count_of: ReceiptRef | CountOfFiltered


class DidFiltered(RecordData):
    result: ReceiptRef
    where: MaterialFilter


class CondDid(RecordData):
    did: ReceiptRef | DidFiltered

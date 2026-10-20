"""Object selection fragments; recipient selections and admission stay separate."""

from typing import Annotated, NotRequired, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_expressions import NumericCondition
from sve_carddb.contracts.author_leaves import ParamRef, Zone  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_object_filters import (
    FILTER_CONSTRAINTS,
    ObjectFilterFields,
    validate_object_filters,
)
from sve_carddb.contracts.author_receipts import SnapshotRead
from sve_carddb.contracts.author_references import (
    BindingName,
    PlayerRef,
    PlayerSet,
    Ref,
)

SELECTION_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    serialize_by_alias=True,
    json_schema_extra={
        **FILTER_CONSTRAINTS,
        "dependentRequired": {"as": ["where"]},
    },
)


_SelectionScope = TypedDict(
    "_SelectionScope",
    {"except": "ObjectSel", "where": NumericCondition, "as": BindingName},
    total=False,
)
_InSource = TypedDict("_InSource", {"in": Ref | SnapshotRead})


@with_config(SELECTION_CONFIG)
class ObjectSelectionFields(ObjectFilterFields, _SelectionScope):
    pass


@with_config(SELECTION_CONFIG)
class ZoneObjectsFields(ObjectSelectionFields):
    who: PlayerRef | PlayerSet | ParamRef
    zone: NotRequired[
        Zone | Annotated[tuple[Zone, ...], Field(min_length=1)] | ParamRef
    ]


@with_config(SELECTION_CONFIG)
class InObjectsFields(ObjectSelectionFields, _InSource):
    pass


def _validate_selection[T: ObjectSelectionFields](value: T) -> T:
    validate_object_filters(value)
    if "as" in value and "where" not in value:
        raise ValueError("A local alias requires where")
    return value


type ZoneObjects = Annotated[ZoneObjectsFields, AfterValidator(_validate_selection)]
type InObjects = Annotated[InObjectsFields, AfterValidator(_validate_selection)]
type ObjectAtom = ZoneObjects | InObjects


@with_config(ConfigDict(extra="forbid", strict=True, regex_engine="python-re"))
class ObjectUnionFields(TypedDict):
    any_of: Annotated[tuple[ObjectAtom, ...], Field(min_length=1)]


type ObjectUnion = ObjectUnionFields
type ObjectSel = ObjectAtom | ObjectUnion

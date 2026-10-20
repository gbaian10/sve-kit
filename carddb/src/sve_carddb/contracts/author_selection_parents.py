"""Closed selection parents, without full author scope admission."""

from collections.abc import Mapping
from typing import Annotated, Literal, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import TrueFlag  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_quantities import SelectionQuantity  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_queries import (
    FILTER_CONSTRAINTS,
    CountFilterFields,
    ObjectSel,
    Quantity,
    RecipientSel,
    validate_object_filters,
)
from sve_carddb.contracts.author_references import BindingName
from sve_carddb.contracts.author_selections import TargetRoles  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.

CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")
TARGET_CONFIG = ConfigDict(
    **CONFIG,
    json_schema_extra={"not": {"required": ["distinct_by", "distinct_names"]}},
)
type DistinctBy = Literal["orig_cost", "class", "name"]

_TargetSource = TypedDict("_TargetSource", {"bind": BindingName, "from": RecipientSel})
_ChooseSource = TypedDict("_ChooseSource", {"bind": BindingName, "from": ObjectSel})


@with_config(TARGET_CONFIG)
class TargetMetadata(TypedDict, total=False):
    roles: TargetRoles
    option: Annotated[int, Field(ge=1, le=9007199254740991)]
    cost_sum_le: Quantity
    distinct_by: DistinctBy
    distinct_names: TrueFlag


@with_config(TARGET_CONFIG)
class TargetFields(_TargetSource, TargetMetadata):
    pass


@with_config(TARGET_CONFIG)
class TargetExactFields(TargetFields):
    n: SelectionQuantity


@with_config(TARGET_CONFIG)
class TargetUpToFields(TargetFields):
    upto: SelectionQuantity


@with_config(TARGET_CONFIG)
class TargetAnyFields(TargetFields):
    any: TrueFlag


def _validate_target[T: Mapping[str, object]](value: T) -> T:
    if "distinct_by" in value and "distinct_names" in value:
        raise ValueError("distinct_by and distinct_names are mutually exclusive")
    return value


type TargetExact = Annotated[TargetExactFields, AfterValidator(_validate_target)]
type TargetUpTo = Annotated[TargetUpToFields, AfterValidator(_validate_target)]
type TargetAny = Annotated[TargetAnyFields, AfterValidator(_validate_target)]


@with_config(
    ConfigDict(**CONFIG, json_schema_extra={**FILTER_CONSTRAINTS, "minProperties": 1})
)
class TargetGroupFilterFields(CountFilterFields):
    pass


def _validate_group(value: TargetGroupFilterFields) -> TargetGroupFilterFields:
    validate_object_filters(value)
    if not value:
        raise ValueError("A target group requires an attribute filter")
    return value


type TargetGroupFilter = Annotated[
    TargetGroupFilterFields, AfterValidator(_validate_group)
]


@with_config(TARGET_CONFIG)
class TargetGroupedFields(_ChooseSource, TargetMetadata):
    groups: Annotated[tuple[TargetGroupFilter, ...], Field(min_length=2)]


@with_config(TARGET_CONFIG)
class TargetGroupedExactFields(TargetGroupedFields):
    n: SelectionQuantity


@with_config(TARGET_CONFIG)
class TargetGroupedUpToFields(TargetGroupedFields):
    upto: SelectionQuantity


@with_config(TARGET_CONFIG)
class TargetGroupedAnyFields(TargetGroupedFields):
    any: TrueFlag


type TargetGroupedExact = Annotated[
    TargetGroupedExactFields, AfterValidator(_validate_target)
]
type TargetGroupedUpTo = Annotated[
    TargetGroupedUpToFields, AfterValidator(_validate_target)
]
type TargetGroupedAny = Annotated[
    TargetGroupedAnyFields, AfterValidator(_validate_target)
]
type Target = (
    TargetExact
    | TargetUpTo
    | TargetAny
    | TargetGroupedExact
    | TargetGroupedUpTo
    | TargetGroupedAny
)
type Targets = Annotated[tuple[Target, ...], Field(min_length=1)]

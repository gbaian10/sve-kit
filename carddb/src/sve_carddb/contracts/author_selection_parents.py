"""Closed single-group selection parents, without full author scope admission."""

from typing import Annotated, Literal, TypedDict

from pydantic import AfterValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import ParamRef, TrueFlag, Zone  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_quantities import SelectionQuantity  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_queries import ObjectSel, Quantity, RecipientSel
from sve_carddb.contracts.author_references import BindingName, PlayerRef
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
class TargetFields(_TargetSource, total=False):
    roles: TargetRoles
    option: Annotated[int, Field(ge=1, le=9007199254740991)]
    cost_sum_le: Quantity
    distinct_by: DistinctBy
    distinct_names: TrueFlag


@with_config(TARGET_CONFIG)
class TargetExactFields(TargetFields):
    n: SelectionQuantity


@with_config(TARGET_CONFIG)
class TargetUpToFields(TargetFields):
    upto: SelectionQuantity


@with_config(TARGET_CONFIG)
class TargetAnyFields(TargetFields):
    any: TrueFlag


def _validate_target[T: TargetFields](value: T) -> T:
    if "distinct_by" in value and "distinct_names" in value:
        raise ValueError("distinct_by and distinct_names are mutually exclusive")
    return value


type TargetExact = Annotated[TargetExactFields, AfterValidator(_validate_target)]
type TargetUpTo = Annotated[TargetUpToFields, AfterValidator(_validate_target)]
type TargetAny = Annotated[TargetAnyFields, AfterValidator(_validate_target)]
type Target = TargetExact | TargetUpTo | TargetAny


@with_config(CONFIG)
class ChooseFields(_ChooseSource, total=False):
    by: PlayerRef
    may: bool
    reveal: bool
    order: bool
    cost_sum_le: Quantity
    distinct_by: DistinctBy


@with_config(CONFIG)
class ChooseExactFields(ChooseFields):
    n: SelectionQuantity


@with_config(CONFIG)
class ChooseUpToFields(ChooseFields):
    upto: SelectionQuantity


@with_config(CONFIG)
class ChooseAnyFields(ChooseFields):
    any: TrueFlag


type Choose = ChooseExact | ChooseUpTo | ChooseAny


class _SearchSource(TypedDict):
    sel: ObjectSel
    to: Zone | ParamRef


@with_config(CONFIG)
class SearchFields(_SearchSource, total=False):
    bind: BindingName
    result: BindingName
    cost_sum_le: Quantity
    distinct_by: DistinctBy


@with_config(CONFIG)
class SearchExactFields(SearchFields):
    n: SelectionQuantity


@with_config(CONFIG)
class SearchUpToFields(SearchFields):
    upto: SelectionQuantity


type Search = SearchExact | SearchUpTo

# Named adapters carry regex configuration across recursive quantity definitions.
type ChooseExact = ChooseExactFields
type ChooseUpTo = ChooseUpToFields
type ChooseAny = ChooseAnyFields
type SearchExact = SearchExactFields
type SearchUpTo = SearchUpToFields

"""Shared object attributes; source roles belong to the containing query."""

from collections.abc import Mapping
from typing import TYPE_CHECKING, Annotated, Literal, TypedDict

from pydantic import BeforeValidator, ConfigDict, Field, with_config

from sve_carddb.contracts.author_leaves import ParamRef
from sve_carddb.contracts.author_receipts import SameAs  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.four_layer import Code  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.core.models import Text, UInt

if TYPE_CHECKING:
    from pydantic.config import JsonDict


def _true(value: object) -> Literal[True]:
    # Literal[True] alone also accepts the integer 1.
    if value is not True:
        raise ValueError("Expected true")
    return True


TrueFlag = Annotated[Literal[True], BeforeValidator(_true)]
FILTER_CONSTRAINTS: JsonDict = {
    "not": {
        "anyOf": [
            {"required": ["token", "not_token"]},
            {"required": ["name", "names"]},
            {"required": ["name", "name_contains"]},
            {"required": ["names", "name_contains"]},
        ]
    }
}


FILTER_CONFIG = ConfigDict(
    extra="forbid",
    strict=True,
    regex_engine="python-re",
    serialize_by_alias=True,
    json_schema_extra=FILTER_CONSTRAINTS,
)


_ClassFilter = TypedDict("_ClassFilter", {"class": Text | ParamRef}, total=False)
_CountClassFilter = TypedDict("_CountClassFilter", {"class": Text}, total=False)


class _SharedFilterFields(TypedDict, total=False):
    names: Annotated[tuple[Text, ...], Field(min_length=1)]
    name_contains: Text
    other: TrueFlag
    orig_cost_eq: UInt
    orig_cost_le: UInt
    orig_cost_ge: UInt
    hp_le: UInt
    acted: bool
    racing: bool
    token: TrueFlag
    not_token: TrueFlag


@with_config(FILTER_CONFIG)
class ObjectFilterFields(_SharedFilterFields, _ClassFilter, total=False):
    kind: Code | ParamRef | SameAs
    trait: Text | ParamRef
    traits_all: Annotated[tuple[Text | ParamRef, ...], Field(min_length=1)]
    name: Text | ParamRef | SameAs


@with_config(FILTER_CONFIG)
class CountFilterFields(_SharedFilterFields, _CountClassFilter, total=False):
    # Shared attribute names do not grant Sel's parameter-source permissions.
    kind: Code | SameAs
    trait: Text
    traits_all: Annotated[tuple[Text, ...], Field(min_length=1)]
    name: Text | SameAs


def validate_object_filters[T: Mapping[str, object]](value: T) -> T:
    """Reject contradictory or overlapping author filter spellings."""
    if "token" in value and "not_token" in value:
        raise ValueError("Token filters are exclusive")
    if sum(key in value for key in ("name", "names", "name_contains")) > 1:
        raise ValueError("Name filters are exclusive")
    return value

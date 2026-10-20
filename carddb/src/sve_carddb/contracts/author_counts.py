"""Collection count shapes with counted-owner and counted-zone source roles."""

from typing import Annotated

from pydantic import AfterValidator, Field, with_config

from sve_carddb.contracts.author_leaves import ParamRef, Zone  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_object_filters import (
    FILTER_CONFIG,
    CountFilterFields,
    validate_object_filters,
)
from sve_carddb.contracts.author_references import PlayerRef, PlayerSet  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.core.models import RecordData


@with_config(FILTER_CONFIG)
class CountObjectsFields(CountFilterFields):
    who: PlayerRef | PlayerSet | ParamRef
    zone: Zone | Annotated[tuple[Zone, ...], Field(min_length=1)] | ParamRef


type CountObjects = Annotated[
    CountObjectsFields, AfterValidator(validate_object_filters)
]


class CountUnion(RecordData):
    any_of: Annotated[tuple[CountObjects, ...], Field(min_length=1)]


type CountSource = CountObjects | CountUnion


class ExprCount(RecordData):
    count: CountSource

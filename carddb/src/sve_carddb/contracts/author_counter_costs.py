"""Counter costs keep counter quantity and holder quantity in separate positions."""

from typing import Literal, NotRequired, TypedDict

from pydantic import ConfigDict, with_config

from sve_carddb.contracts.author_leaves import CounterName, TrueFlag  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_queries import (
    ObjectSel,  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
    Quantity,  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
)
from sve_carddb.contracts.author_references import BindingName  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.

CONFIG = ConfigDict(extra="forbid", strict=True, regex_engine="python-re")


@with_config(CONFIG)
class CounterCostFields(TypedDict):
    name: CounterName
    result: NotRequired[BindingName]


@with_config(CONFIG)
class CounterCostSelfFields(CounterCostFields):
    on: Literal["this"]


@with_config(CONFIG)
class CounterCostSelectedFields(CounterCostFields):
    on: ObjectSel
    holders: Quantity


@with_config(CONFIG)
class CounterCostSelfExactFields(CounterCostSelfFields):
    n: Quantity


@with_config(CONFIG)
class CounterCostSelfUpToFields(CounterCostSelfFields):
    upto: Quantity


@with_config(CONFIG)
class CounterCostSelfAnyFields(CounterCostSelfFields):
    any: TrueFlag


@with_config(CONFIG)
class CounterCostSelectedExactFields(CounterCostSelectedFields):
    n: Quantity


@with_config(CONFIG)
class CounterCostSelectedUpToFields(CounterCostSelectedFields):
    upto: Quantity


@with_config(CONFIG)
class CounterCostSelectedAnyFields(CounterCostSelectedFields):
    any: TrueFlag


type CounterCostSelf = CounterCostSelfExact | CounterCostSelfUpTo | CounterCostSelfAny
type CounterCostSelected = (
    CounterCostSelectedExact | CounterCostSelectedUpTo | CounterCostSelectedAny
)
type CounterCost = CounterCostSelf | CounterCostSelected

# Named adapters carry regex configuration across recursive quantity definitions.
type CounterCostSelfExact = CounterCostSelfExactFields
type CounterCostSelfUpTo = CounterCostSelfUpToFields
type CounterCostSelfAny = CounterCostSelfAnyFields
type CounterCostSelectedExact = CounterCostSelectedExactFields
type CounterCostSelectedUpTo = CounterCostSelectedUpToFields
type CounterCostSelectedAny = CounterCostSelectedAnyFields

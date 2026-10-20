"""Counter costs keep counter quantity and holder quantity in separate positions."""

from typing import Literal, NotRequired, TypedDict

from pydantic import ConfigDict, with_config

from sve_carddb.contracts.author_object_filters import TrueFlag  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_quantities import Quantity  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_references import BindingName  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.contracts.author_selectors import ObjectSel  # ruff: ignore[typing-only-first-party-import] -- Pydantic resolves TypedDict fields at runtime.
from sve_carddb.core.models import Text

type CounterName = Text
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

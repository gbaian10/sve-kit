"""Share adapter construction while keeping explicit wire-to-Python correspondence."""

from typing import TYPE_CHECKING

from pydantic import ConfigDict, TypeAdapter
from typing_extensions import TypeForm

if TYPE_CHECKING:
    from collections.abc import Mapping


def adapters(
    types: Mapping[str, tuple[TypeForm[object], str]],
) -> dict[str, TypeAdapter[object]]:
    return {
        name: TypeAdapter(
            value,
            config=None
            if isinstance(value, type)
            else ConfigDict(regex_engine="python-re"),
        )
        for name, (value, _) in types.items()
    }


def type_names(types: Mapping[str, tuple[TypeForm[object], str]]) -> dict[str, str]:
    return {key: name for key, (_, name) in types.items()}

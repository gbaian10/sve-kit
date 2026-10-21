"""Local author invariants over decoded types; no registry or source admission."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from pydantic import BaseModel, RootModel

from sve_carddb.contracts.author_entries import GeneratedBy
from sve_carddb.contracts.author_leaves import ParamRef

if TYPE_CHECKING:
    from collections.abc import Iterator

type PathParts = tuple[str | int, ...]


@dataclass(frozen=True)
class Violation:
    reason: str
    path: PathParts


def typed_nodes(
    value: object, path: PathParts = ()
) -> Iterator[tuple[PathParts, object]]:
    """Walk decoded nodes using wire aliases and transparent root wrappers."""
    if isinstance(value, RootModel):
        yield from typed_nodes(value.root, path)
        return
    yield path, value
    if isinstance(value, BaseModel):
        for name, field in type(value).model_fields.items():
            child: object = getattr(value, name)
            if field.exclude or (field.exclude_if and field.exclude_if(child)):
                continue
            key = field.serialization_alias or field.alias or name
            yield from typed_nodes(child, (*path, key))
    elif isinstance(value, dict):
        for key, child in value.items():
            if isinstance(key, str):
                yield from typed_nodes(child, (*path, key))
    elif isinstance(value, tuple | list):
        for index, child in enumerate(value):
            yield from typed_nodes(child, (*path, index))


def inline_parameters(value: object, path: PathParts) -> Iterator[Violation]:
    """Only a decoded ParamRef is a parameter; metadata keys are not syntax."""
    for location, node in typed_nodes(value, path):
        if isinstance(node, ParamRef):
            yield Violation("inline_param_reference", location)


def expanded_residuals(value: object) -> Iterator[Violation]:
    """Independently reject typed syntax that expansion must have eliminated."""
    for path, node in typed_nodes(value):
        if isinstance(node, ParamRef):
            yield Violation("expanded_param_reference", path)
        elif isinstance(node, GeneratedBy):
            yield Violation("expanded_generated_by", path)

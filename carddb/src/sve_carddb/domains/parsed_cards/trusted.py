"""Deserialize already validated repository data without running model validators."""

import dataclasses
from functools import cache
from types import UnionType
from typing import Annotated, Literal, Union, cast, get_args, get_origin, get_type_hints

import orjson
from pydantic import BaseModel


@cache
def _fields(model: type) -> dict[str, object]:
    if issubclass(model, BaseModel):
        return {name: field.annotation for name, field in model.model_fields.items()}
    return get_type_hints(model, include_extras=True)


def _union(annotation: object, value: object) -> object:
    choices = get_args(annotation)
    if value is None:
        return None
    for variant in choices:
        choice = variant
        if choice is type(None):
            continue
        if get_origin(choice) is Annotated:
            choice = get_args(choice)[0]
        if isinstance(choice, type) and issubclass(choice, BaseModel):
            data = cast("dict[str, object]", value)
            tags = {
                name: get_args(field)
                for name, field in _fields(choice).items()
                if get_origin(field) is Literal
            }
            if not all(data.get(name) in values for name, values in tags.items()):
                continue
        return _value(choice, value)
    raise TypeError("Unsupported trusted input variant")


def _value(annotation: object, value: object) -> object:  # ruff: ignore[too-many-return-statements] -- each type constructor is an independent deserialization branch
    origin = get_origin(annotation)
    args = get_args(annotation)
    if origin is Annotated:
        return _value(args[0], value)
    if origin in {UnionType, Union}:
        return _union(annotation, value)
    if origin in {list, tuple}:
        items = [_value(args[0], item) for item in cast("list[object]", value)]
        return tuple(items) if origin is tuple else items
    if origin is dict:
        return {
            k: _value(args[1], v) for k, v in cast("dict[str, object]", value).items()
        }
    if isinstance(annotation, type) and (
        issubclass(annotation, BaseModel) or dataclasses.is_dataclass(annotation)
    ):
        if isinstance(value, annotation):
            return value
        data = cast("dict[str, object]", value)
        fields = _fields(annotation)
        kwargs = {name: _value(fields[name], item) for name, item in data.items()}
        if issubclass(annotation, BaseModel):
            return annotation.model_construct(_fields_set=None, **kwargs)
        return annotation(**kwargs)
    return value


def construct[T](model: type[T], data: object) -> T:
    """Restore nested Python types; schema and semantic checks belong to admission."""
    return cast("T", _value(model, data))


def decode[T](model: type[T], raw: bytes) -> T:
    """Parse trusted JSON without canonicalization, hashes or duplicate-key scans."""
    return construct(model, orjson.loads(raw))

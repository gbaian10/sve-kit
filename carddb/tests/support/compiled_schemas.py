"""Compile each requested DDL group set once per process.

`CompiledSchema` is a frozen dataclass of tuples, so sharing one instance is safe;
every database still gets its own connection and Rules from `create_database`.
"""

from functools import cache
from typing import TYPE_CHECKING

from sve_carddb.build.t1 import compile_build as _compile_uncached

if TYPE_CHECKING:
    from sve_carddb.build import CompiledSchema


@cache
def _compile_build(requested: tuple[str, ...]) -> CompiledSchema:
    return _compile_uncached(requested)


def compile_build(requested: tuple[str, ...] = ("t0",)) -> CompiledSchema:
    """Reuse schemas by their effective capability request."""
    return _compile_build(tuple(requested))

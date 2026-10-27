"""Small schema selections for synthetic database tests."""

from sve_carddb.build.compiler import CompiledSchema, compile_schema
from sve_carddb.build.t0 import REGISTRY
from sve_carddb.build.t0_json import schemas
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build


def compile_t0() -> CompiledSchema:
    """Compile all forty-one T0 tables and their six named JSON column schemas."""
    return compile_schema(REGISTRY, ("t0",), schemas())


def compile_minimum(*, include_en: bool = False) -> CompiledSchema:
    """Compile the minimum DDL inventory, without asserting pipeline readiness."""
    return compile_build(MINIMUM_CAPABILITIES + (("en",) if include_en else ()))

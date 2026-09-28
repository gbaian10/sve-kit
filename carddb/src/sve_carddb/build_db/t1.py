"""Implemented T1 DDL groups; none yet claim a complete import/validation pipeline."""

from sve_carddb.build_db import t0, t1_cr, t1_images
from sve_carddb.build_db.compiler import CompiledSchema, compile_schema
from sve_carddb.build_db.model import Capability
from sve_carddb.build_db.registry import Registry
from sve_carddb.build_db.t0_json import schemas

SCHEMA_VERSION = 2
TABLES = (*t1_images.TABLES, *t1_cr.TABLES)
REGISTRY = Registry(
    tables=(*t0.TABLES, *TABLES),
    capabilities=(
        Capability("t0", tuple(table.name for table in t0.TABLES)),
        Capability("images", tuple(table.name for table in t1_images.TABLES)),
        Capability("cr", tuple(table.name for table in t1_cr.TABLES)),
        *(cap for cap in t0.REGISTRY.capabilities if cap.name not in {"t0", "cr"}),
    ),
)


def compile_build(requested: tuple[str, ...] = ("t0",)) -> CompiledSchema:
    """Compile only selected DDL groups and dependencies, without enabling a pipeline."""
    return compile_schema(REGISTRY, requested, schemas(), version=SCHEMA_VERSION)

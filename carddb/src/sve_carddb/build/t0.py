"""The T0 build schema; reserved future tables are never materialized as stubs."""

from sve_carddb.build import t0_display, t0_identity, t0_rules, t0_text
from sve_carddb.build.model import Capability
from sve_carddb.build.registry import Registry

TABLES = (*t0_identity.TABLES, *t0_text.TABLES, *t0_rules.TABLES, *t0_display.TABLES)
REGISTRY = Registry(
    tables=TABLES,
    capabilities=(
        Capability("t0", tuple(table.name for table in TABLES)),
        Capability("art", ("art",), implemented=False),
        Capability("cr", ("cr_version",), implemented=False),
        Capability("dsl", ("dsl_document", "dsl_load"), implemented=False),
        Capability("keyword", ("keyword",), implemented=False),
    ),
)

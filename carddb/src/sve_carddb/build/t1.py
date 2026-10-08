"""Implemented T1 DDL groups; none yet claim a complete import/validation pipeline."""

from sve_carddb.build import (
    t0,
    t1_correction,
    t1_cr,
    t1_en,
    t1_errata,
    t1_images,
    t1_json,
    t1_qa,
    t1_related,
    t2_translation,
    templates,
    translation_evidence,
)
from sve_carddb.build.compiler import CompiledSchema, compile_schema
from sve_carddb.build.model import Capability
from sve_carddb.build.registry import Registry
from sve_carddb.build.t0_json import schemas

SCHEMA_VERSION = 6
MINIMUM_CAPABILITIES = ("t0", "images", "cr", "errata", "correction", "qa", "related")
TABLES = (
    *t1_images.TABLES,
    *t1_cr.TABLES,
    *t1_errata.TABLES,
    *t1_correction.TABLES,
    *t1_qa.TABLES,
    *t1_related.TABLES,
    *t1_en.TABLES,
    *translation_evidence.TABLES,
    *t2_translation.TABLES,
    *templates.TABLES,
)
REGISTRY = Registry(
    tables=(*t0.TABLES, *TABLES),
    capabilities=(
        Capability("t0", tuple(table.name for table in t0.TABLES)),
        Capability("images", tuple(table.name for table in t1_images.TABLES)),
        Capability("cr", tuple(table.name for table in t1_cr.TABLES)),
        Capability("errata", tuple(table.name for table in t1_errata.TABLES)),
        Capability("correction", tuple(table.name for table in t1_correction.TABLES)),
        Capability("qa", tuple(table.name for table in t1_qa.TABLES)),
        Capability("related", tuple(table.name for table in t1_related.TABLES)),
        Capability(
            "translation_evidence",
            tuple(table.name for table in translation_evidence.TABLES),
        ),
        Capability(
            "translation_names",
            tuple(table.name for table in t2_translation.TABLES),
            requires=("t0", "translation_evidence"),
        ),
        Capability(
            "translation_templates",
            tuple(table.name for table in templates.TABLES),
            requires=("translation_names",),
        ),
        Capability("art", ("art",)),
        Capability(
            "en",
            tuple(table.name for table in t1_en.TABLES if table.name != "art"),
            requires=("art",),
        ),
        *(
            cap
            for cap in t0.REGISTRY.capabilities
            if cap.name not in {"t0", "cr", "art"}
        ),
    ),
)


def compile_build(requested: tuple[str, ...] = ("t0",)) -> CompiledSchema:
    """Compile only selected DDL groups and dependencies, without enabling a pipeline."""
    return compile_schema(
        REGISTRY,
        requested,
        schemas()
        | t1_json.schemas()
        | templates.schemas()
        | {"TranslationTokens": {"type": "null"}},
        version=SCHEMA_VERSION,
    )


def compile_minimum(*, include_en: bool = False) -> CompiledSchema:
    """Compile the minimum DDL inventory, without asserting pipeline readiness."""
    return compile_build(MINIMUM_CAPABILITIES + (("en",) if include_en else ()))

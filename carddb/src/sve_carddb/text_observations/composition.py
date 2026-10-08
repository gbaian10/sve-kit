"""Compose complete diagnostic staging and a pinned inseparable build bundle."""

from typing import TYPE_CHECKING

from sve_carddb.catalog.rules_names import populate_rules_names
from sve_carddb.core.provenance import input_record
from sve_carddb.products import populate_product_preview, product_preview_uses
from sve_carddb.text_observations.importer import populate_text_observations

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.catalog.models import Catalog
    from sve_carddb.core.provenance import BuildContext, InputRecord, SourceUse
    from sve_carddb.products import Language
    from sve_carddb.products.loader import ProductSnapshot
    from sve_carddb.products.models import LocalizedText
    from sve_carddb.products.plan import OfficialProducts
    from sve_carddb.text_observations.plan import TextPlan
    from sve_carddb.text_observations.vocabulary import Vocabulary


def text_preview_uses(
    catalog: ProductSnapshot,
    plan: TextPlan,
    stores: Mapping[str, Path],
    *,
    official: OfficialProducts | None = None,
) -> tuple[SourceUse, ...]:
    """Declare complete input use closure independently of actual database insertion."""
    return (
        *product_preview_uses(catalog, plan.identity, stores, official=official),
        *plan.source_uses(),
    )


def populate_text_preview(  # ruff: ignore[too-many-arguments] -- one transaction binds the catalog, frozen plan and explicit build configuration
    db: Database,
    catalog: ProductSnapshot,
    plan: TextPlan,
    *,
    authored_revision: str,
    build: BuildContext,
    vocabulary: Vocabulary,
    published: tuple[LocalizedText, ...],
    languages: tuple[Language, ...],
    stores: Mapping[str, Path],
    official: OfficialProducts | None = None,
    catalog_config: Catalog | None = None,
) -> InputRecord:
    """Retain diagnostic parents for quarantined observations inside the bundle transaction."""
    identity = populate_product_preview(
        db,
        catalog,
        plan.identity,
        authored_revision=authored_revision,
        build=build,
        languages=languages,
        stores=stores,
        official=official,
    )
    texts = populate_text_observations(
        db, plan, build=build, vocabulary=vocabulary, published=published
    )
    populate_rules_names(db)
    if catalog_config is not None:
        from sve_carddb.catalog.importer import populate_catalog  # ruff: ignore[import-outside-top-level] -- the optional catalog importer shares the text interner

        populate_catalog(db, catalog_config, build=build, published=published)
    return input_record(build, (*identity.uses, *texts.uses))

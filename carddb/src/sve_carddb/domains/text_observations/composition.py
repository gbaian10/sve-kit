"""Compose complete diagnostic staging and a pinned inseparable build bundle."""

from dataclasses import dataclass
from typing import TYPE_CHECKING

from sve_carddb.core.provenance import input_record
from sve_carddb.domains.catalog.rules_names import populate_rules_names
from sve_carddb.domains.products import populate_product_preview, product_preview_uses
from sve_carddb.domains.text_observations.configuration import text_configuration
from sve_carddb.domains.text_observations.importer import populate_text_observations
from sve_carddb.domains.text_observations.plan import plan_text_observations
from sve_carddb.domains.text_observations.wording import (
    printing_observed_texts,
    wording_views,
)

if TYPE_CHECKING:
    from collections.abc import Mapping
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build import Database
    from sve_carddb.core.provenance import BuildContext, InputRecord, SourceUse
    from sve_carddb.domains.catalog.models import Catalog
    from sve_carddb.domains.products import Language
    from sve_carddb.domains.products.loader import ProductSnapshot
    from sve_carddb.domains.products.models import LocalizedText
    from sve_carddb.domains.products.plan import OfficialProducts
    from sve_carddb.domains.registry.preview import PreviewPlan
    from sve_carddb.domains.source_corrections.images import ImageProvider
    from sve_carddb.domains.text_observations.models import TextProvider
    from sve_carddb.domains.text_observations.plan import TextPlan
    from sve_carddb.domains.text_observations.vocabulary import Vocabulary
    from sve_carddb.domains.text_observations.wording import ObservedText, WordingView
    from sve_carddb.ingest.archive.frozen_sources import FrozenBatches


def text_preview_uses(
    catalog: ProductSnapshot,
    plan: TextPlan,
    stores: Mapping[str, Path],
    *,
    batches: FrozenBatches | None = None,
    official: OfficialProducts | None = None,
) -> tuple[SourceUse, ...]:
    """Declare complete input use closure independently of actual database insertion."""
    return (
        *product_preview_uses(
            catalog, plan.identity, stores, official=official, batches=batches
        ),
        *plan.source_uses(),
    )


def populate_text_preview(  # ruff: ignore[too-many-arguments] -- one transaction binds the catalog, frozen plan and explicit build configuration
    db: Database,
    catalog: ProductSnapshot,
    observations: TextObservations,
    *,
    authored_revision: str,
    build: BuildContext,
    vocabulary: Vocabulary,
    published: tuple[LocalizedText, ...],
    languages: tuple[Language, ...],
    stores: Mapping[str, Path],
    batches: FrozenBatches | None = None,
    official: OfficialProducts | None = None,
    catalog_config: Catalog | None = None,
) -> InputRecord:
    """Retain diagnostic parents for quarantined observations inside the bundle transaction."""
    plan = observations.plan
    identity = populate_product_preview(
        db,
        catalog,
        plan.identity,
        authored_revision=authored_revision,
        build=build,
        languages=languages,
        stores=stores,
        batches=batches,
        official=official,
    )
    texts = populate_text_observations(
        db, observations, build=build, vocabulary=vocabulary, published=published
    )
    populate_rules_names(db)
    if catalog_config is not None:
        from sve_carddb.domains.catalog.importer import populate_catalog  # ruff: ignore[import-outside-top-level] -- the optional catalog importer shares the text interner

        populate_catalog(db, catalog_config, build=build, published=published)
    return input_record(build, (*identity.uses, *texts.uses))


@dataclass(frozen=True, init=False)
class TextObservations:
    """Compose source reads, selection, import and views from one owned plan."""

    plan: TextPlan

    def __init__(
        self,
        preview: PreviewPlan,
        provider: TextProvider,
        *,
        images: ImageProvider | None = None,
    ) -> None:
        object.__setattr__(
            self, "plan", plan_text_observations(preview, provider, images=images)
        )

    def configuration(
        self, vocabulary: Vocabulary, published: tuple[LocalizedText, ...]
    ) -> dict[str, JsonValue]:
        """Pin the processed source inputs and explicit external vocabulary/history."""
        return text_configuration(self.plan, vocabulary, published)

    def populate(
        self,
        db: Database,
        *,
        build: BuildContext,
        vocabulary: Vocabulary,
        published: tuple[LocalizedText, ...],
    ) -> InputRecord:
        """Import the owned plan in the caller's transaction, checking DB dependencies."""
        return populate_text_observations(
            db, self, build=build, vocabulary=vocabulary, published=published
        )

    def import_into(
        self,
        db: Database,
        *,
        build: BuildContext,
        vocabulary: Vocabulary,
        published: tuple[LocalizedText, ...],
    ) -> InputRecord:
        """Roll back all staging writes if external configuration or DB parents disagree."""
        with db.transaction():
            return self.populate(
                db, build=build, vocabulary=vocabulary, published=published
            )

    def views(self, db: Database) -> TextViews:
        """Project both views from the same processed inputs and current DB rows."""
        return TextViews(
            wording_views(db, self.plan), printing_observed_texts(db, self.plan)
        )


@dataclass(frozen=True)
class TextViews:
    wording: dict[str, tuple[WordingView, ...]]
    observed: dict[tuple[str, str], tuple[ObservedText, ...]]

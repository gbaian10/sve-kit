"""Adopt glossary values at an atomic database boundary."""

from typing import TYPE_CHECKING

from sve_carddb.domains.translations.glossary.populate import populate

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.core.provenance import BuildContext, InputRecord
    from sve_carddb.domains.translations.inputs import Inputs
    from sve_carddb.domains.translations.sources import Sources


def populate_glossary(
    db: Database,
    inputs: Inputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    sources: Sources | None = None,
) -> InputRecord:
    """Compose current values with verified publication identity and frozen sources."""
    return populate(
        db, inputs, inputs.load(), build=build, stores=stores, sources=sources
    )


def import_glossary(
    db: Database,
    inputs: Inputs,
    *,
    build: BuildContext,
    stores: dict[str, Path],
    sources: Sources | None = None,
) -> InputRecord:
    """Own an atomic transaction for glossary rows and their entire provenance."""
    with db.transaction():
        return populate_glossary(
            db, inputs, build=build, stores=stores, sources=sources
        )

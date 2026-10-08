"""Exercise native current catalog composition using synthetic frozen sources."""

from typing import TYPE_CHECKING

from sve_carddb.domains.translations.sources import Sources
from sve_carddb.workflows.offline import _populate_adoptions, _prepare_catalog

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build import Database
    from sve_carddb.core.provenance import InputRecord
    from sve_carddb.domains.catalog.loader import Prepared

    from .adoption_fixtures import Case


def prepare_case(case: Case, stores: dict[str, Path]) -> Prepared:
    return _prepare_catalog(case.inputs(), case.build(), stores, None)


def populate_case(db: Database, case: Case, stores: dict[str, Path]) -> InputRecord:
    with db.transaction():
        return _populate_adoptions(
            db,
            case.inputs(),
            build=case.build(),
            stores=stores,
            prepared=prepare_case(case, stores),
            sources=Sources(stores, case.repository, case.build()),
        )

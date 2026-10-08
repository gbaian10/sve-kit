"""Exercise native current catalog composition using synthetic frozen sources."""

from typing import TYPE_CHECKING

from sve_carddb.snapshot.offline import _populate_adoptions, _prepare_catalog
from sve_carddb.translations.sources import Sources

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.catalog.current import Prepared
    from sve_carddb.core.provenance import InputRecord

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

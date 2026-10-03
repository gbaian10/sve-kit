"""Printing-specific flavor reads never borrow current effect/name sources."""

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.snapshot.values import digest
from sve_carddb.template_translations.flavor_models import FlavorOwner
from sve_carddb.template_translations.flavor_owners import verify_owner

from .name_build_fixtures import template

if TYPE_CHECKING:
    from .database_fixtures import DatabaseTemplate


@pytest.fixture(scope="module")
def database() -> DatabaseTemplate:
    return template()


@pytest.mark.parametrize("pending", [False, True])
def test_owner_uses_own_flavor_even_when_effects_are_pending(
    database: DatabaseTemplate, pending: bool
) -> None:
    with database.copy() as db:
        with db.transaction():
            db.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"flavor_unit_id": "printed-name"},
            )
            if pending:
                db.update(
                    "printing_face",
                    {"printing_id": "printing", "face_id": "face"},
                    {"printed_text_state": "unknown", "printed_name_unit_id": None},
                )
            verify_owner(
                db,
                FlavorOwner("printing", "face", "printed-name"),
                source_hash=digest(b"Synthetic old name"),
                context_source_unit_id="printed-name",
            )
            assert not db.rows("translation_use")


@pytest.mark.parametrize(
    ("guard", "message"),
    [
        ("missing", "Flavor printing face owner is absent"),
        ("other_face", "Flavor printing face owner is absent"),
        ("other_unit", "Flavor context must use its physical owner's flavor unit"),
        ("context", "Flavor context must use its physical owner's flavor unit"),
        ("hash", "Flavor physical text unit exact hash or language mismatch"),
        ("unknown", "Flavor context must use its physical owner's flavor unit"),
        ("identity", "Flavor owner requires confirmed card identity"),
    ],
)
def test_owner_source_and_context_cannot_be_substituted(
    database: DatabaseTemplate, guard: str, message: str
) -> None:
    with database.copy() as db:
        with db.transaction():
            db.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"flavor_unit_id": None if guard == "unknown" else "printed-name"},
            )
            if guard == "identity":
                db.update("card", {"id": "card"}, {"identity_state": "provisional"})
            owner = FlavorOwner(
                "missing" if guard == "missing" else "printing",
                "back" if guard == "other_face" else "face",
                "other-name" if guard == "other_unit" else "printed-name",
            )
            with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
                verify_owner(
                    db,
                    owner,
                    source_hash=digest(
                        b"Wrong synthetic name"
                        if guard == "hash"
                        else b"Synthetic old name"
                    ),
                    context_source_unit_id="other-name"
                    if guard == "context"
                    else "printed-name",
                )

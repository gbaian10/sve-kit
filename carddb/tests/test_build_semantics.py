"""Stored semantic rule hashes must match the exact interned text."""

from types import SimpleNamespace
from typing import TYPE_CHECKING, cast

import pytest

from sve_carddb.build_db import Json
from sve_carddb.build_db.semantics import verify_semantics
from sve_carddb.snapshot.values import canonical, digest

if TYPE_CHECKING:
    from sve_carddb.build_db import Database


def fake_db(semantic: dict[str, object], units: dict[str, object]) -> Database:
    tables = {
        "text_unit": [{"id": key, "text": text} for key, text in units.items()],
        "face_semantics": [semantic],
    }
    return cast(
        "Database",
        SimpleNamespace(
            rows=lambda name: [SimpleNamespace(values=row) for row in tables[name]]
        ),
    )


def semantic(rule_hash: str) -> dict[str, object]:
    return {
        "rule_text_unit_id": "u1",
        "rule_sections": Json(["u2"]),
        "rule_hash": rule_hash,
    }


UNITS: dict[str, object] = {"u1": "Synthetic rule", "u2": "Synthetic section"}
GOOD = digest(
    canonical({"rule_text": "Synthetic rule", "rule_sections": ["Synthetic section"]})
)


def test_matching_hash_passes() -> None:
    verify_semantics(fake_db(semantic(GOOD), UNITS))


def test_hash_mismatch_is_refused() -> None:
    with pytest.raises(ValueError, match="rule hash"):
        verify_semantics(fake_db(semantic(digest(b"wrong")), UNITS))


def test_missing_section_text_is_refused() -> None:
    with pytest.raises(TypeError, match="dependency is missing"):
        verify_semantics(fake_db(semantic(GOOD), {"u1": "Synthetic rule"}))


def test_non_list_sections_are_refused() -> None:
    row = semantic(GOOD) | {"rule_sections": Json({"a": 1})}
    with pytest.raises(TypeError, match="Invalid semantic sections"):
        verify_semantics(fake_db(row, UNITS))

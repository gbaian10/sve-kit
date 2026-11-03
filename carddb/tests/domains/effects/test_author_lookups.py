"""Lookups reused within one command still follow later database writes and caller edits."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_inputs import parse_document
from sve_carddb.domains.effects.author_sources import SourceError

from .test_author_current_absent import synthetic_current

if TYPE_CHECKING:
    from pathlib import Path


def test_repeated_documents_return_independent_values() -> None:
    raw = canonical({"cards": {"SYN-01": {"abilities": [{"line": 1}]}}})
    first = parse_document(raw)
    object_value(first.value)["cards"] = {}
    second = parse_document(raw)
    assert second.value == {"cards": {"SYN-01": {"abilities": [{"line": 1}]}}}
    assert second.value is not first.value
    assert second.locations.locate(("cards", "SYN-01")) is not None


def test_printing_lookup_sees_rows_written_after_the_first_lookup(
    tmp_path: Path,
) -> None:
    with synthetic_current(tmp_path) as current:
        card_id = current.printing_card("SYN-01")
        with pytest.raises(SourceError):
            current.printing_card("SYN-02")
        printing = dict(current.db.rows("printing")[0].values)
        with current.db.transaction():
            current.db.insert(
                "printing",
                printing
                | {"id": "SYN-02", "card_no": "SYN-02", "card_no_state": "official"},
            )
        assert current.printing_card("SYN-02") == card_id

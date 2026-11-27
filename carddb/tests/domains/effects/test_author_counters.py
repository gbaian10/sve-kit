"""Counter caps and created objects require registered names."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.domains.effects.author_counters import check_counter_registration
from sve_carddb.domains.effects.author_current import CardAccepted
from sve_carddb.domains.effects.author_inputs import AuthorInputError

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue


@pytest.mark.parametrize(
    ("tree", "path"),
    [
        (
            {"rule": "counter_cap", "on": "this", "name": "unknown", "max": 1},
            ("name",),
        ),
        (
            {"create": {"token": "token", "to": "field", "counters": {"unknown": 1}}},
            ("create", "counters"),
        ),
    ],
)
def test_unregistered_counter_in_cap_or_token(
    tmp_path: Path, tree: dict[str, JsonValue], path: tuple[str, ...]
) -> None:
    (tmp_path / "rules").mkdir()
    (tmp_path / "rules/keyword-registry.yaml").write_text(
        "format: 1\nkind: keyword_registry\nrecords: []\n"
        "decision: {id: synthetic, state: confirmed, note: Synthetic registry}\n"
    )
    result = CardAccepted(
        AuthorIndexRecord(
            card_id="c:" + "0" * 32,
            scope="shared",
            file="rules/author-effects/SYN/001.yaml",
            kind="author_effect_set",
            record_key="SYN-01",
        ),
        tree,
        {},
    )
    with pytest.raises(AuthorInputError) as error:
        check_counter_registration(tmp_path, (result,))
    assert error.value.diagnostic.reason == "unregistered_counter"
    assert error.value.diagnostic.path == path

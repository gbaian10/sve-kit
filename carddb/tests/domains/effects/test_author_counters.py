"""Counter caps and created objects require registered names."""

from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_counters import check_counter_registration
from sve_carddb.domains.effects.author_current import CardAccepted
from sve_carddb.domains.effects.author_inputs import AuthorInputError
from sve_carddb.domains.mechanics import NamedRule, Registry

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


@pytest.mark.parametrize("operand", ["armageddon_counter", "Armageddon counter"])
def test_english_counter_is_accepted_by_code_and_name(
    tmp_path: Path, operand: str
) -> None:
    (tmp_path / "rules").mkdir()
    (tmp_path / "rules/keyword-registry.yaml").write_text(
        "format: 1\nkind: keyword_registry\nrecords: []\n"
        "decision: {id: synthetic, state: confirmed, note: Synthetic registry}\n"
        "rule_names:\n  armageddon_counter: {en: Armageddon counter, role: counter}\n"
    )
    result = CardAccepted(
        AuthorIndexRecord(
            card_id="c:" + "0" * 32,
            scope="shared",
            file="rules/author-effects/SYN/001.yaml",
            kind="author_effect_set",
            record_key="SYN-01",
        ),
        {"counter": {"on": "this", "name": operand, "delta": 1}},
        {},
    )
    check_counter_registration(tmp_path, (result,))


@pytest.mark.parametrize(
    ("rule", "message"),
    [
        ({"role": "counter", "aliases": ["Only an alias"]}, "A counter needs"),
        ({"role": "ability-label", "en": "Ability"}, "needs a Japanese rules name"),
    ],
)
def test_only_counters_accept_an_english_name_without_japanese(
    rule: dict[str, JsonValue], message: str
) -> None:
    with pytest.raises(ValidationError, match=message):
        NamedRule.model_validate_json(canonical(rule))


def test_english_counter_cannot_reuse_another_rules_name() -> None:
    with pytest.raises(
        ValidationError, match="Ambiguous rule name: Armageddon counter"
    ):
        Registry.model_validate(
            {
                "format": 1,
                "kind": "keyword_registry",
                "decision": {"id": "source", "state": "confirmed", "note": "Fixture"},
                "records": (),
                "rule_names": {
                    "existing": {
                        "ja": "カウンター",
                        "role": "counter",
                        "aliases": ("Armageddon counter",),
                    },
                    "armageddon_counter": {
                        "en": "Armageddon counter",
                        "role": "counter",
                    },
                },
            }
        )

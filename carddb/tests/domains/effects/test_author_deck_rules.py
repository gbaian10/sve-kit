"""Construction lines are checked metadata with whole-card coverage."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, object_value
from sve_carddb.domains.effects.author_direct import check_coverage, materialize
from sve_carddb.domains.effects.author_handwritten import Handwritten
from sve_carddb.domains.effects.author_inputs import parse_document
from sve_carddb.domains.effects.author_sources import SourceError

from ..text_observations.test_effect_presence import page
from .author_entry_cases import meta
from .test_author_current_absent import synthetic_current

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue


def card(rule: JsonValue) -> Handwritten:
    tree: dict[str, JsonValue] = {
        "admission": "light",
        "meta": {"written_by": meta()["written_by"]},
        "abilities": [],
    }
    if rule is not None:
        tree["deck_rule"] = rule
    return Handwritten(
        {"SYN-01": tree}, {}, {"SYN-01": parse_document(canonical(tree))}
    )


@pytest.mark.parametrize(
    ("text", "deck", "maximum"),
    [
        ("『Synthetic name』は自分のデッキに6枚まで入れることができる。", "main", 6),
        ("これはエボルヴデッキに10枚まで入れることができる。", "evolve", 10),
    ],
    ids=["main_limit", "evolve_limit"],
)
def test_printed_rule_covers_its_line_without_an_in_game_ability(
    tmp_path: Path, text: str, deck: str, maximum: int
) -> None:
    rule: JsonValue = {"deck": deck, "same_name_max": maximum}
    with synthetic_current(
        tmp_path, page("jp", f'<div class="detail">{text}</div>')
    ) as current:
        result = materialize(current, "SYN-01", card(rule))
    assert result["deck_rule"] == rule
    assert all(
        not array(object_value(face)["abilities"])
        for face in object_value(result["faces"]).values()
    )


@pytest.mark.parametrize(
    "rule",
    [
        None,
        {"deck": "main", "same_name_max": 10},
        {"deck": "evolve", "same_name_max": 9},
    ],
)
def test_missing_or_changed_declaration_cannot_consume_the_printed_rule(
    tmp_path: Path, rule: JsonValue
) -> None:
    with (
        synthetic_current(
            tmp_path,
            page(
                "jp",
                '<div class="detail">これはエボルヴデッキに10枚まで入れることができる。</div>',
            ),
        ) as current,
        pytest.raises(SourceError, match="source_deck_rule_mismatch"),
    ):
        materialize(current, "SYN-01", card(rule))


@pytest.mark.parametrize(
    "rule",
    [
        {"deck": "main", "same_name_max": 50},
        {
            "when": "self_in_main",
            "decks": ["main", "evolve"],
            "exclude_name_contains": "Synthetic",
            "same_name_max": 1,
        },
    ],
)
def test_an_unprinted_rule_cannot_be_added_to_a_blank_card(
    tmp_path: Path, rule: JsonValue
) -> None:
    with (
        synthetic_current(tmp_path) as current,
        pytest.raises(SourceError, match="source_deck_rule_mismatch"),
    ):
        materialize(current, "SYN-01", card(rule))


def test_another_cards_name_does_not_grant_a_limit(tmp_path: Path) -> None:
    with (
        synthetic_current(
            tmp_path,
            page(
                "jp",
                '<div class="detail">『Another』は自分のデッキに6枚まで入れることができる。</div>',
            ),
        ) as current,
        pytest.raises(SourceError, match="source_deck_rule_name_mismatch"),
    ):
        materialize(current, "SYN-01", card({"deck": "main", "same_name_max": 6}))


@pytest.mark.parametrize("overlap", ["ability", "ignored", "token"])
def test_construction_lines_cannot_be_consumed_twice(overlap: str) -> None:
    rule: JsonValue = {"deck": "evolve", "same_name_max": 10}
    source: dict[str, JsonValue] = {
        "face_id": "face",
        "ordinal": None,
        "line": 1,
        "scope": "deck_rule",
        "covers": [1],
    }
    payload: dict[str, JsonValue] = {
        "faces": [{"face_id": "face", "fields": [{"ordinal": None, "lines": [1]}]}],
        "deck_rules": [{"rule": rule, "source": source}],
    }
    authored: dict[str, JsonValue] = {"deck_rule": rule, "abilities": []}
    ignored = []
    if overlap == "ability":
        authored["abilities"] = [
            {"line": 1, "sources": [source], "spell": {"do": [{"draw": {"n": 1}}]}}
        ]
    elif overlap == "ignored":
        ignored = [source]
    else:
        authored["token_sources"] = [{"card_no": "TOKEN", "source": source}]
    with pytest.raises(SourceError, match="duplicate_source_ability"):
        check_coverage(payload, authored, ignored)


def test_blank_card_does_not_require_a_construction_name(tmp_path: Path) -> None:
    with synthetic_current(tmp_path) as current:
        with current.db.transaction():
            for row in current.db.rows("face_rules_name"):
                current.db.delete(
                    "face_rules_name",
                    {
                        key: row.values[key]
                        for key in ("face_id", "region", "rules_name_id", "role")
                    },
                )
        result = materialize(current, "SYN-01", card(None))
    assert "deck_rule" not in result


CONDITIONAL_TEXT = (
    "カード名に『Synthetic』を含まないカードは、"
    "メインデッキやエボルヴデッキにそれぞれ1枚までしか入れられない。"
)
CONDITIONAL_RULE: dict[str, JsonValue] = {
    "when": "self_in_main",
    "decks": ["main", "evolve"],
    "exclude_name_contains": "Synthetic",
    "same_name_max": 1,
}


def test_conditional_rule_is_checked_against_its_complete_source(
    tmp_path: Path,
) -> None:
    with synthetic_current(
        tmp_path, page("jp", f'<div class="detail">{CONDITIONAL_TEXT}</div>')
    ) as current:
        result = materialize(current, "SYN-01", card(CONDITIONAL_RULE))
    assert result["deck_rule"] == CONDITIONAL_RULE


@pytest.mark.parametrize(
    "change",
    [
        {},
        {"when": "self_in_evolve"},
        {"decks": ["main"]},
        {"exclude_name_contains": "Another"},
        {"same_name_max": 2},
    ],
)
def test_conditional_rule_cannot_change_its_printed_meaning(
    tmp_path: Path, change: dict[str, JsonValue]
) -> None:
    rule = CONDITIONAL_RULE | change if change else None
    with (
        synthetic_current(
            tmp_path, page("jp", f'<div class="detail">{CONDITIONAL_TEXT}</div>')
        ) as current,
        pytest.raises(SourceError, match="source_deck_rule_mismatch"),
    ):
        materialize(current, "SYN-01", card(rule))

"""Current wording preferences use synthetic rules and supplemental text."""

import pytest

from sve_carddb.core.wording import Wording, preferred_wordings


@pytest.mark.parametrize(
    ("plain", "reminder"),
    [
        ("【必殺】", "【必殺】（【必殺】を持つ合成ユニットの説明）"),
        ("【突進】【必殺】", "【突進】【必殺】（【突進】を持つ合成ユニットの説明）"),
        ("Bane.", "Bane. (After this follower fights, synthetic reminder.)"),
        ("Drain.", "Drain. (When a follower with Drain attacks, synthetic reminder.)"),
    ],
)
def test_absent_reminders_win_in_main_and_token_sections(
    plain: str, reminder: str
) -> None:
    for values in (
        (Wording(b"same", reminder), Wording(b"same", plain)),
        (
            Wording(b"same", "Create a token.", (reminder,)),
            Wording(b"same", "Create a token.", (plain,)),
        ),
    ):
        assert preferred_wordings(values) == ((1,), "reminder_only")
        assert preferred_wordings(tuple(reversed(values))) == ((0,), "reminder_only")


def test_supplements_win_before_reminders_and_keep_their_order() -> None:
    values = (
        Wording(b"same", "Create a token."),
        Wording(
            b"same",
            "Create a token.",
            ("Token A: 【必殺】（【必殺】を持つ合成ユニットの説明）",),
        ),
        Wording(b"same", "Create a token.", ("Token A: 【必殺】",)),
    )
    assert preferred_wordings(values) == ((2,), "supplemental_sections")
    assert preferred_wordings(
        (
            Wording(b"same", "Rule", ("A", "B")),
            Wording(b"same", "Rule", ("B", "A")),
        )
    ) == ((0, 1), "other_wording")


@pytest.mark.parametrize(
    "other",
    [
        Wording(b"same", "Draw 2 cards."),
        Wording(b"same", "Draw 1 card (only if ready)."),
        Wording(b"different stats", "Draw 1 card."),
        Wording(b"same", "Draw 1 card.", ("Unrelated supplement",)),
    ],
)
def test_substantive_differences_fall_back_to_release_order(other: Wording) -> None:
    left = Wording(b"same", "Draw 1 card.", ("Token definition",))
    assert preferred_wordings((left, other)) == ((0, 1), "other_wording")


def test_a_keyword_inside_a_condition_does_not_prove_a_reminder() -> None:
    assert preferred_wordings(
        (
            Wording(b"same", "Rule. (Cards with Aura cannot be selected.)"),
            Wording(b"same", "Rule."),
        )
    ) == ((0, 1), "other_wording")

"""Closed explicit contexts do not license bare numeric or inherited roles."""

import pytest

from sve_carddb.domains.translations.recognition.candidate_matching import recognize
from sve_carddb.domains.translations.recognition.explicit_rules import EXPLICIT
from sve_carddb.domains.translations.recognition.references import References

from .test_template_parameters import candidate, matches, partition

CASES = (
    (
        "named_counter_place",
        "これに雷カウンター２個を置く。",
        "counter_place_quantity",
        0,
    ),
    (
        "named_counter_remove",
        "これの雷カウンター２個を取る。",
        "counter_remove_quantity",
        0,
    ),
    (
        "named_counter_threshold",
        "これの雷カウンターが２個以上なら使える。",
        "counter_threshold",
        0,
    ),
    (
        "named_counter_group_size",
        "これの雷カウンター２個につき試す。",
        "counter_group_size",
        1,
    ),
    (
        "leader_person_quantity",
        "相手のリーダー２人を選ぶ。",
        "leader_person_quantity",
        0,
    ),
    (
        "player_person_quantity",
        "相手プレイヤー２人は試す。",
        "player_person_quantity",
        0,
    ),
    ("deck_top_ordinal", "デッキの上から２番目に置く。", "deck_top_ordinal", 1),
)


@pytest.mark.parametrize(("identifier", "text", "role", "minimum"), CASES)
def test_explicit_match_requires_opt_in_exact_value_and_preserves_proposal(
    identifier: str, text: str, role: str, minimum: int
) -> None:
    del minimum
    c = candidate(text)
    original = c.model_dump()
    assert recognize(text, partition(text)[0], c, References()) == ()
    rows = matches(text, identifier)
    assert len(rows) == 1
    row = rows[0]
    assert (row["rule_id"], row["recognized_role"], row["value"]) == (
        identifier,
        role,
        2,
    )
    assert c.model_dump() == original
    assert matches(text.replace("２", "2"), identifier)
    assert matches(text.replace("２", "②"), identifier) == ()
    assert matches(text.replace("２", "9007199254740992"), identifier) == ()
    for sign in ("+", "-", "−", "＋", "－"):
        assert matches(text.replace("２", sign + "２"), identifier) == ()
    damaged = c.model_copy(
        update={"slots": (c.slots[0].model_copy(update={"value": 8}),)}
    )
    assert (
        recognize(text, partition(text)[0], damaged, References(), (identifier,)) == ()
    )


@pytest.mark.parametrize(
    ("identifier", "text"),
    [
        ("named_counter_place", "これにカウンター２個を置く。"),
        ("named_counter_place", "これに未知カウンター２個を置く。"),
        ("named_counter_place", "これに超雷カウンター２個を置く。"),
        ("named_counter_place", "これの雷カウンター２個を置く。"),
        ("named_counter_place", "これに雷カウンター２個を取る。"),
        ("named_counter_place", "これに雷カウンター２個を置く名。"),
        ("named_counter_remove", "これに雷カウンター２個を取る。"),
        ("named_counter_remove", "これの雷カウンター２個を置く。"),
        ("named_counter_threshold", "これの雷カウンター２個以上なら使える。"),
        ("named_counter_threshold", "これの雷カウンターが２個を取る。"),
        ("named_counter_group_size", "これの雷カウンター０個につき試す。"),
        ("named_counter_group_size", "これの雷カウンター２個ごと試す。"),
        ("leader_person_quantity", "相手の試験者２人を選ぶ。"),
        ("leader_person_quantity", "相手のリーダー２人目を選ぶ。"),
        ("leader_person_quantity", "相手のリーダー２人数を試す。"),
        ("leader_person_quantity", "リーダー２人を選ぶ。"),
        ("player_person_quantity", "相手プレイヤー２人目を試す。"),
        ("player_person_quantity", "相手のプレイヤー２人は試す。"),
        ("deck_top_ordinal", "デッキの下から２番目に置く。"),
        ("deck_top_ordinal", "デッキの上から０番目に置く。"),
        ("deck_top_ordinal", "デッキの上から２枚目に置く。"),
        ("deck_top_ordinal", "２番目に置く。"),
        ("deck_top_ordinal", "デッキの上から２番目Xに置く。"),
    ],
)
def test_similar_nouns_units_and_incomplete_contexts_stay_unresolved(
    identifier: str, text: str
) -> None:
    assert matches(text, identifier) == ()


def test_context_proof_covers_original_fullwidth_counter_and_unit() -> None:
    row = matches("これに雷カウンター２個を置く。", "named_counter_place")[0]
    assert row["context_segments"] == [{"start": 2, "end": 14}]
    assert row["source_segments"] == [{"start": 9, "end": 10}]


def test_complete_reminder_is_supported_but_quoted_card_name_is_not() -> None:
    assert matches("（相手のリーダー２人を選ぶ）", "leader_person_quantity")
    assert matches("『相手のリーダー２人を選ぶ』", "leader_person_quantity") == ()
    assert {case[0] for case in CASES} <= set(EXPLICIT)

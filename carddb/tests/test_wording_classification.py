"""Unapproved finite proposals classify synthetic differences without adoption."""

from typing import TYPE_CHECKING

import pytest

from sve_carddb.snapshot.values import canonical
from sve_carddb.text_observations import FaceContent
from sve_carddb.wording_adoptions.classification import classify

if TYPE_CHECKING:
    from sve_carddb.registry.records import Region


def face(effect: str | None, sections: tuple[str, ...] = ()) -> FaceContent:
    return FaceContent(
        name="Synthetic name",
        effect=effect,
        sections=sections,
        class_raw="Synthetic class",
        type_raw="Synthetic type",
        stats=("1", "1", "1"),
    )


@pytest.mark.parametrize(
    ("before", "after", "expected", "rule"),
    [
        ("Synthetic\r\nparagraph", "Synthetic\nparagraph", "whitespace", "wp:eol-v1"),
        (
            "Synthetic  paragraph",
            "Synthetic paragraph",
            "whitespace",
            "wp:layout-space-v1",
        ),
        (
            "Synthetic paragraph.",
            "Synthetic paragraph。",
            "punctuation",
            "wp:punctuation-v1",
        ),
        ("Draw 1 card.", "Draw 1 cards.", "sentence_pattern", "wp:draw-plural-v1"),
        ("Draw 1 card.", "Draw 2 cards.", "uncovered", None),
        ("Synthetic complete clause", "Synthetic clause", "uncovered", None),
        (None, "", "uncovered", None),
    ],
)
def test_finite_difference_diagnostics_never_enable_equivalence(
    before: str | None, after: str, expected: str, rule: str | None
) -> None:
    result = classify(face(before), face(after), region="en")
    assert result.categories == (expected,)
    if rule is not None:
        assert rule in result.rule_ids
    else:
        assert not result.rule_ids
    report = result.report()
    assert report["automatic_equivalence"] is False
    assert report["approval_status"] == "unapproved"
    encoded = canonical(report).decode()
    assert "Synthetic" not in encoded
    assert "Draw" not in encoded


@pytest.mark.parametrize("known", [False, True])
def test_unknown_section_does_not_become_a_reminder(known: bool) -> None:
    result = classify(
        face("Synthetic main", ("Synthetic reminder A",)),
        face("Synthetic main", ("Synthetic reminder B",)),
        region="jp",
        known_reminders=frozenset({0}) if known else frozenset(),
    )
    assert result.categories == (("reminder",) if known else ("uncovered",))
    assert result.differences[0]["field"] == "sections/0"
    assert result.differences[0]["ranges"]


def test_exact_paragraph_repartition_is_only_a_layout_diagnostic() -> None:
    result = classify(
        face("Synthetic main\nSynthetic tail"),
        face("Synthetic main\n", ("Synthetic tail",)),
        region="jp",
    )
    assert result.categories == ("section_layout",)
    assert not result.report()["automatic_equivalence"]


def test_terminology_requires_an_explicit_finite_pair() -> None:
    before, after = face("Synthetic Alpha"), face("Synthetic Beta")
    assert classify(before, after, region="en").categories == ("uncovered",)
    result = classify(before, after, region="en", term_pairs=(("Alpha", "Beta"),))
    assert result.categories == ("terminology",)
    assert not result.report()["automatic_equivalence"]


@pytest.mark.parametrize("region", ["jp", "en"])
def test_sentence_proposal_has_a_regional_boundary(region: Region) -> None:
    result = classify(face("Draw 1 card."), face("Draw 1 cards."), region=region)
    assert result.categories == (
        ("sentence_pattern",) if region == "en" else ("uncovered",)
    )


@pytest.mark.parametrize(
    "field", ["name", "stats", "traits", "type_raw", "class_raw", "title"]
)
def test_current_bearing_dependency_change_is_uncovered_even_with_matched_layout(
    field: str,
) -> None:
    before = face("Synthetic\r\nmain")
    replacements = {
        "name": "Another synthetic name",
        "stats": ("2", "1", "1"),
        "traits": ("Synthetic trait",),
        "type_raw": "Another synthetic type",
        "class_raw": "Another synthetic class",
        "title": "Synthetic title",
    }
    after = before.model_copy(
        update={"effect": "Synthetic\nmain", field: replacements[field]}
    )
    result = classify(before, after, region="jp")
    assert result.categories == ("uncovered",)
    assert not result.rule_ids


def test_exact_content_is_not_a_human_or_policy_equivalence_receipt() -> None:
    before = face("Synthetic exact main")
    result = classify(before, before, region="jp")
    assert not result.categories
    assert not result.differences
    assert not result.report()["automatic_equivalence"]

"""Synthetic current lists exercise dates, structure and unreviewed observations."""

from datetime import date

import pytest

from sve_carddb.core.regions import SourceRegion
from sve_carddb.parse.html import MissingElementError
from sve_carddb.parse.pages.restriction_list import parse_restriction_list

JP = """<html><head><title>Synthetic JP</title></head><body>
<div class="sw-Lower_Container"><div class="free_Detail sw-Txtarea">
<h2>カードの使用制限に関するルール</h2>
<h3>禁止カード</h3><p>Definition, not a card.</p>
<h4>2026/1/30(金)より適用</h4>
<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-a">『合成禁止』</a></p>
<h3>殿堂入りカード</h3><h4>2024/6/28(金)より適用</h4>
<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-2">『合成制限』</a></p>
<h3>過去の更新内容</h3><h4><a href="https://example.invalid/history">Ignore history</a></h4>
<h3>「クロスオーバー」のカードの使用制限</h3><a href="https://example.invalid/crossover">Other format</a>
<h2>エラッタ</h2><a href="https://example.invalid/errata">Ignore</a>
</div></div></body></html>"""
EN = """<html><head><title>Synthetic EN</title></head><body>
<div class="sw-Lower_Container"><div class="free_Detail sw-Txtarea">
<h2>Restrictions</h2><h3>Standard</h3><p><strong>Banned</strong></p>
<h4>Effective as of September 4, to be lifted on September 25, 2026</h4>
<p><strong><a href="https://en.shadowverse-evolve.com/news/post-999999">Synthetic Reborn</a></strong></p>
<h4>Effective as of August 21, 2026</h4>
<p><a href="https://en.shadowverse-evolve.com/cards/?cardno=TEST-1EN">Synthetic Ban</a></p>
<p><strong>Restricted to 1</strong></p>
<h4><span>Effective as of August 21, 2026</span></h4>
<div><p><img src="https://example.invalid/do-not-fetch.png"></p><br>
<strong><a href="https://en.shadowverse-evolve.com/cards/?cardno= TEST-aEN"> Nested Synthetic</a></strong></div>
<h3>Cross Craft</h3><p>Banned</p><h4>Unsupported date in an excluded format</h4>
<a href="https://example.invalid/other">Not a Standard entry</a>
<h3>Gloryfinder</h3><a href="https://example.invalid/choice">Choice group</a>
<h2>Use of Non-English Cards</h2>
</div></div></body></html>"""


def test_jp_current_categories_and_explicit_exclusions() -> None:
    parsed = parse_restriction_list(JP.encode(), SourceRegion.JP)
    assert parsed.semantic_state == "unreviewed"
    assert [(item.name, item.card_no, item.max_copies) for item in parsed.entries] == [
        ("合成禁止", "TEST-a", 0),
        ("合成制限", "TEST-2", 1),
    ]
    assert [item.effective_from for item in parsed.entries] == [
        date(2026, 1, 30),
        date(2024, 6, 28),
    ]
    assert parsed.excluded_sections == (
        "過去の更新内容",
        "「クロスオーバー」のカードの使用制限",
    )
    assert (
        parsed.entries[0].locator
        == "css:.sw-Lower_Container;query=h2,h3,h4,p,a[href];ordinal=6"
    )
    assert parsed.entries[1].heading == "2024/6/28(金)より適用"


def test_en_nested_entry_and_exclusive_lift_date() -> None:
    parsed = parse_restriction_list(EN.encode(), SourceRegion.EN)
    assert [item.name for item in parsed.entries] == [
        "Synthetic Reborn",
        "Synthetic Ban",
        "Nested Synthetic",
    ]
    lifted = parsed.entries[0]
    assert lifted.card_no is None
    assert not lifted.active_on(date(2026, 9, 3))
    assert lifted.active_on(date(2026, 9, 4))
    assert lifted.active_on(date(2026, 9, 24))
    assert not lifted.active_on(date(2026, 9, 25))
    assert parsed.entries[2].card_no == "TEST-aEN"
    assert parsed.entries[2].card_link.endswith("cardno= TEST-aEN")
    assert parsed.entries[2].max_copies == 1
    assert parsed.excluded_sections == ("Cross Craft", "Gloryfinder")
    assert (
        len([entry for entry in parsed.entries if entry.active_on(date(2026, 9, 25))])
        == 2
    )


@pytest.mark.parametrize(
    ("old", "new"),
    [
        ("2026/1/30(金)より適用", "2026/2/30(月)より適用"),
        ("2026/1/30(金)より適用", "Soon"),
        ("<h4>2026/1/30(金)より適用</h4>", ""),
        ("殿堂入りカード", "Unsupported category"),
        ("?cardno=TEST-a", "?cardno=TEST-a&amp;cardno="),
        (
            "https://shadowverse-evolve.com/cardlist/?cardno=TEST-a",
            "https://example.invalid/card?cardno=TEST-a",
        ),
        ("『合成禁止』", "unquoted"),
        ("</html>", ""),
        ("sw-Lower_Container", "changed-container"),
    ],
)
def test_jp_changes_fail_instead_of_guessing(old: str, new: str) -> None:
    with pytest.raises((ValueError, MissingElementError)):
        parse_restriction_list(JP.replace(old, new).encode(), SourceRegion.JP)


@pytest.mark.parametrize(
    ("old", "new"),
    [
        (
            "September 4, to be lifted on September 25, 2026",
            "September 25, to be lifted on September 4, 2026",
        ),
        ("August 21, 2026", "UnknownMonth 21, 2026"),
        ("Standard", "Unknown format"),
        ("Restricted to 1", "Restricted to 2"),
        ("?cardno=TEST-1EN", "?other=TEST-1EN"),
    ],
)
def test_en_changes_fail_instead_of_guessing(old: str, new: str) -> None:
    with pytest.raises(ValueError, match=r"(Restriction|restriction|format|month)"):
        parse_restriction_list(EN.replace(old, new).encode(), SourceRegion.EN)


def test_duplicate_entry_is_not_silently_merged() -> None:
    entry = '<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-a">『合成禁止』</a></p>'
    with pytest.raises(ValueError, match="repeats an entry"):
        parse_restriction_list(
            JP.replace(entry, entry + entry).encode(), SourceRegion.JP
        )


@pytest.mark.parametrize(
    ("region", "original", "old", "new"),
    [
        (
            SourceRegion.JP,
            JP,
            '<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-2">『合成制限』</a></p>',
            "",
        ),
        (
            SourceRegion.JP,
            JP,
            '<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-a">『合成禁止』</a></p>',
            "",
        ),
        (
            SourceRegion.EN,
            EN,
            '<strong><a href="https://en.shadowverse-evolve.com/cards/?cardno= TEST-aEN"> Nested Synthetic</a></strong>',
            "",
        ),
        (
            SourceRegion.JP,
            JP,
            "<h4>2026/1/30(金)より適用</h4>",
            "<h4>2025/1/30(木)より適用</h4><h4>2026/1/30(金)より適用</h4>",
        ),
    ],
)
def test_empty_category_or_effective_date_cannot_silently_lose_entries(
    region: SourceRegion, original: str, old: str, new: str
) -> None:
    with pytest.raises(
        ValueError, match=r"(no entries|lacks both Standard categories)"
    ):
        parse_restriction_list(original.replace(old, new).encode(), region)


@pytest.mark.parametrize(
    "replacement",
    [
        "合成禁止",
        "<br>合成制限",
        "合成<span>制限</span>",
        "合成 <em>制限</em>",
    ],
)
def test_conflicting_or_multiline_names_are_rejected(replacement: str) -> None:
    with pytest.raises(ValueError, match=r"(conflicting entries|nested markup)"):
        parse_restriction_list(
            JP.replace("合成制限", replacement).encode(), SourceRegion.JP
        )


def test_same_card_number_with_different_names_is_rejected() -> None:
    with pytest.raises(ValueError, match="conflicting entries"):
        parse_restriction_list(
            JP.replace("cardno=TEST-2", "cardno=TEST-a").encode(), SourceRegion.JP
        )


@pytest.mark.parametrize(
    ("old", "new", "message"),
    [
        (
            "Effective as of August 21, 2026",
            "Effective later",
            "Unsupported EN restriction effective-date heading",
        ),
        ("<p><strong>Banned</strong></p>", "", "date precedes its category"),
        ("Synthetic Ban", "", "lacks category, date or name"),
        ("September 4", "UnknownMonth 4", "Unsupported EN restriction month"),
    ],
)
def test_en_missing_structure_reports_domain_errors(
    old: str, new: str, message: str
) -> None:
    with pytest.raises(ValueError, match=message):
        parse_restriction_list(EN.replace(old, new).encode(), SourceRegion.EN)


@pytest.mark.parametrize(
    ("old", "new", "message"),
    [
        ("カードの使用制限に関するルール", "Unknown title", "lacks its regional title"),
        ("『合成禁止』", "『』", "name must not be blank"),
        ("2026/1/30", "２０２６/1/30", "Unsupported JP restriction"),
    ],
)
def test_jp_missing_title_or_name_is_rejected(old: str, new: str, message: str) -> None:
    with pytest.raises(ValueError, match=message):
        parse_restriction_list(JP.replace(old, new).encode(), SourceRegion.JP)

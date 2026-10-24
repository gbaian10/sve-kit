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
    with pytest.raises(
        ValueError, match=r"(Restriction|restriction|format|month|tuple\.index)"
    ):
        parse_restriction_list(EN.replace(old, new).encode(), SourceRegion.EN)


def test_duplicate_entry_is_not_silently_merged() -> None:
    entry = '<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-a">『合成禁止』</a></p>'
    with pytest.raises(ValueError, match="repeats an entry"):
        parse_restriction_list(
            JP.replace(entry, entry + entry).encode(), SourceRegion.JP
        )

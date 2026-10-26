"""Explicit Standard lift blocks retain dates and original linked identities."""

import pytest

from sve_carddb.parse.html import MissingElementError
from sve_carddb.parse.pages.restriction_lifts import parse_jp_restriction_lifts

HTML = """<html><body><div class="news_Detail">
<div class="heading"><h1>「スタンダード」殿堂入り解除のお知らせ</h1></div>
<div class="contents"><p>■2026年10月2日(金)から「スタンダード」で禁止になるカード</p>
<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=OTHER">『別の合成カード』</a></p>
<p>■2026年10月2日(金)から殿堂入り解除になるカード</p>
<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-a">『合成解除』</a></p>
<p>■「殿堂入り解除」とは</p><p>Definition.</p>
<a href="https://example.invalid/ignore">Unrelated</a>
</div></div></body></html>"""


def test_lifts_are_confined_to_the_explicit_section() -> None:
    entries = parse_jp_restriction_lifts(HTML.encode())
    assert len(entries) == 1
    assert entries[0]["card_no"] == "TEST-a"
    assert entries[0]["name"] == "合成解除"
    assert entries[0]["effective_until"] == "2026-10-02"


@pytest.mark.parametrize(
    ("old", "new"),
    [
        (
            "「スタンダード」殿堂入り解除のお知らせ",
            "「クロスオーバー」殿堂入り解除のお知らせ",
        ),
        ("2026年10月2日(金)から殿堂入り解除", "2026年2月30日(月)から殿堂入り解除"),
        ("2026年10月2日(金)から殿堂入り解除", "２０２６年10月2日(金)から殿堂入り解除"),
        ("cardno=TEST-a", "other=TEST-a"),
        ("『合成解除』", "unquoted"),
        ("『合成解除』", "『』"),
        ("『合成解除』", "『合成<span>解除</span>』"),
        ("</html>", ""),
        ("contents", "unknown"),
    ],
)
def test_unsupported_lifts_fail_instead_of_guessing(old: str, new: str) -> None:
    with pytest.raises((ValueError, MissingElementError)):
        parse_jp_restriction_lifts(HTML.replace(old, new).encode())


def test_repeated_lifted_card_is_rejected() -> None:
    entry = '<p><a href="https://shadowverse-evolve.com/cardlist/?cardno=TEST-a">『合成解除』</a></p>'
    with pytest.raises(ValueError, match="unique lift section"):
        parse_jp_restriction_lifts(HTML.replace(entry, entry + entry).encode())

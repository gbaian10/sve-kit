from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.extract.jsonl import extract_cards
from sve_carddb.extract.official_jp import QA, extract_card
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import Writer

if TYPE_CHECKING:
    from sve_carddb.manifest import Manifest

FIXTURES = Path(__file__).parent / "fixtures" / "official_jp"


def fixture(name: str) -> bytes:
    return (FIXTURES / name).read_bytes()


def test_basic_card() -> None:
    card = extract_card(fixture("card_BP02-071.html"), number="BP02-071")
    [face] = card.faces
    assert face.name == "ソウルディーラー"
    assert (face.card_class, face.card_type, face.traits) == (
        "ナイトメア",
        "フォロワー",
        ["魔界"],
    )
    assert (face.cost, face.power, face.hp) == ("4", "6", "5")
    assert face.rarity == "LG"
    assert face.illustrator == "InHyuk Lee"
    assert (
        face.text
        == "{進化}{コスト2}：これは進化する。\n【守護】\n{ファンファーレ}自分のリーダーに3ダメージ。"
    )
    assert face.token_text is None


def test_double_faced_card_has_two_faces_and_token_details() -> None:
    card = extract_card(fixture("card_BP08-003.html"), number="BP08-003")
    front, back = card.faces
    assert front.name == "決意の人形・オーキス"
    assert back.name == "復讐の人形・オーキス"
    assert front.card_type == "フォロワー・エボルヴ"
    assert front.cost == "-"
    assert front.traits == ["人形", "光輝"]
    assert back.product is None
    assert front.text is not None
    assert "――" not in front.text
    assert front.token_text is not None
    assert front.token_text.startswith(
        "『操り人形』{ニュートラル}人形・フォロワー{コスト1}"
    )


def test_errata_release_date_and_rulings() -> None:
    card = extract_card(fixture("card_BP01-004.html"), number="BP01-004")
    assert card.errata_url == "https://shadowverse-evolve.com/errata/bp01-004/"
    assert card.release_date == "2022-04-28"
    # The errata notice must not be mistaken for the illustrator.
    assert card.faces[0].illustrator == "ねじ太"
    assert card.notes == ["能力テキストにエラッタが含まれます。くわしくはこちら"]
    assert card.qa
    first = card.qa[0]
    assert isinstance(first, QA)
    assert first.id.startswith("Q")
    assert not first.question.startswith("Q")
    assert not first.answer.startswith("A")


def test_flavor_keeps_line_breaks() -> None:
    card = extract_card(fixture("card_BP01-003.html"), number="BP01-003")
    assert card.faces[0].flavor == (
        "生命とは輪廻、繰り返される循環。\n森が育んだ者たちは、やがて森を育む者へと変わる。"
    )


def test_page_without_card_detail_is_rejected() -> None:
    body = (
        b"<!DOCTYPE html><html><body>" + b"<p>maintenance</p>" * 100 + b"</body></html>"
    )
    with pytest.raises(Exception, match="cardlist-Detail"):
        extract_card(body, number="BP01-001")


def test_missing_info_row_is_rejected() -> None:
    body = fixture("card_BP02-071.html").replace(
        "<dt>クラス</dt>".encode(), b"<dt>x</dt>"
    )
    with pytest.raises(ValidationError, match="クラス"):
        extract_card(body, number="BP02-071")


def test_extract_cards_writes_only_trusted_listed_cards(
    manifest: Manifest, tmp_path: Path
) -> None:
    # No validated lists yet: nothing to extract, but the file is still replaced cleanly.
    dest = tmp_path / "derived" / "cards.jsonl"
    report = extract_cards(manifest, Writer(tmp_path / "data", manifest), dest)
    assert report.written == 0
    assert dest.read_bytes() == b""
    assert list(dest.parent.glob(".tmp-*")) == []

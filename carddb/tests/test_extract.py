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
    assert face.sections == []


def test_card_without_illustrator_credit() -> None:
    # The site puts the card number where the illustrator name would be.
    card = extract_card(fixture("card_BP02-070.html"), number="BP02-070")
    assert [face.illustrator for face in card.faces] == [None]


def test_double_faced_card_has_two_faces_and_sections() -> None:
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
    [tokens] = front.sections
    assert tokens.startswith("『操り人形』{ニュートラル}人形・フォロワー{コスト1}")


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


def test_compound_trait_is_not_split() -> None:
    body = fixture("card_BP02-071.html").replace(
        "<dd>魔界</dd>".encode(), "<dd>魔界・ジオ・テオゴニア・光輝</dd>".encode()
    )
    [face] = extract_card(body, number="BP02-071").faces
    assert face.traits == ["魔界", "ジオ・テオゴニア", "光輝"]
    assert face.trait_raw == "魔界・ジオ・テオゴニア・光輝"


@pytest.mark.parametrize("number", ["BP03-LDⓈ01", "CP01-001a"])
def test_special_card_numbers_keep_raw_fields_and_hints(number: str) -> None:
    body = f"""<!DOCTYPE html><html><body>
    <div class="cardlist-Detail"><div class="cardlist-Detail_Box_Inner">
    <div class="img"><img src="/images/{number}.png"></div>
    <h1 class="ttl">名前</h1><div class="info">
    <dl><dt>クラス</dt><dd>エルフ</dd></dl>
    <dl><dt>カード種類</dt><dd>トークン</dd></dl>
    <dl><dt>タイプ</dt><dd>-</dd></dl>
    <dl><dt>レアリティ</dt><dd>PR</dd></dl>
    </div><div class="status">
    <span class="status-Item status-Item-Cost"><span class="heading">コスト</span>-</span>
    <span class="status-Item status-Item-Power"><span class="heading">攻撃力</span>0</span>
    <span class="status-Item status-Item-Hp"><span class="heading">体力</span>1</span>
    </div><div class="detail"></div><div class="speech"></div>
    </div></div><div class="cardlist-Under">
    <div class="cardlist-Detail_Products_Inner"><p class="date">2026-01-01</p>
    <p class="ttl">商品</p><a href="/products/test/">商品ページ</a></div>
    <a href="/cardlist/?cardno=BP01-003">関連カード</a></div>
    <!-- {"x" * 1000} --></body></html>""".encode()
    record = extract_card(body, number=number)
    [face] = record.faces
    assert record.number == number
    assert (face.cost, face.power, face.hp) == ("-", "0", "1")
    assert face.image == f"/images/{number}.png"
    assert face.traits == []
    assert face.text == ""  # ruff: ignore[compare-to-empty-string] -- distinguish present empty from missing
    assert face.flavor == ""  # ruff: ignore[compare-to-empty-string] -- distinguish present empty from missing
    assert record.products[0].name == "商品"
    assert record.products[0].links == ["/products/test/"]
    assert record.related_cards[0].href == "/cardlist/?cardno=BP01-003"


def test_missing_ability_differs_from_present_empty_ability() -> None:
    body = fixture("card_BP02-071.html")
    start = body.index(b'<div class="detail">')
    end = body.index(b"</div>", start) + len(b"</div>")
    missing = extract_card(body[:start] + body[end:], number="BP02-071")
    empty = extract_card(
        body[:start] + b'<div class="detail"></div>' + body[end:],
        number="BP02-071",
    )
    assert missing.faces[0].text is None
    assert empty.faces[0].text == ""  # ruff: ignore[compare-to-empty-string] -- distinguish present empty from missing


def test_empty_section_keeps_its_position() -> None:
    body = fixture("card_BP02-071.html")
    start = body.index(b'<div class="detail">')
    end = body.index(b"</div>", start) + len(b"</div>")
    section_only = (
        body[:start] + "<div class='detail'>―――――</div>".encode() + body[end:]
    )
    [face] = extract_card(section_only, number="BP02-071").faces
    assert face.text == ""  # ruff: ignore[compare-to-empty-string] -- empty leading section is present
    assert face.sections == [""]


def test_missing_image_src_is_rejected() -> None:
    body = fixture("card_BP02-071.html").replace(
        b'<img src="/wordpress/wp-content/images/cardlist/BP02/bp02_071.png"',
        b"<img",
    )
    with pytest.raises(ValidationError, match="without src"):
        extract_card(body, number="BP02-071")


def test_empty_card_name_is_rejected() -> None:
    body = fixture("card_BP02-071.html").replace(
        '<h1 class="ttl Sans">ソウルディーラー</h1>'.encode(),
        b'<h1 class="ttl Sans"></h1>',
    )
    with pytest.raises(ValidationError, match="empty card name"):
        extract_card(body, number="BP02-071")


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

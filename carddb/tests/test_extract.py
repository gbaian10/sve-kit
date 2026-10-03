import re
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.extract.jsonl import extract_cards
from sve_carddb.extract.official_jp import QA, extract_card
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import Writer

if TYPE_CHECKING:
    from sve_carddb.manifest import Manifest

FIXTURES = Path(__file__).parent / "fixtures" / "synthetic_jp"


def fixture(name: str) -> bytes:
    return (FIXTURES / name).read_bytes()


def test_basic_card() -> None:
    card = extract_card(fixture("F10-card.html"), number="SYN01-010")
    [face] = card.faces
    assert face.name == "SVE-KIT 合成測試卡 F10-01"
    assert (face.card_class, face.card_type, face.traits) == (
        "ナイトメア",
        "フォロワー",
        ["合成特性甲"],
    )
    assert face.trait_raw == "合成特性甲"
    assert (face.cost, face.power, face.hp) == ("3", "3", "3")
    assert face.rarity == "LG"
    assert face.product == "SVE-KIT 合成商品 F10"
    assert face.illustrator == "SVE-KIT 合成插畫者 F10-01"
    assert face.text == (
        "{進化}{コスト2}SVE-KIT 合成F10段落018。\n"
        "SVE-KIT 合成F10段落019。\n"
        "{ファンファーレ}SVE-KIT 合成F10段落020。"
    )
    assert face.sections == []


def test_card_without_illustrator_credit() -> None:
    # The site puts the card number where the illustrator name would be.
    card = extract_card(fixture("F09-card.html"), number="SYN01-009")
    assert [face.illustrator for face in card.faces] == [None]


def test_double_faced_card_has_two_faces_and_sections() -> None:
    card = extract_card(fixture("F12-card.html"), number="SYN01-012")
    front, back = card.faces
    assert front.name == "SVE-KIT 合成測試卡 F12-01"
    assert back.name == "SVE-KIT 合成測試卡 F12-02"
    assert front.card_type == "フォロワー・エボルヴ"
    assert front.cost == "-"
    assert front.traits == ["合成特性甲", "合成特性乙"]
    assert front.product == "SVE-KIT 合成商品 F12"
    assert back.product is None
    assert front.text == "SVE-KIT 合成F12段落018。\nSVE-KIT 合成F12段落019。"
    assert back.text == (
        "SVE-KIT 合成F12段落048。\nSVE-KIT 合成F12段落049。\nSVE-KIT 合成F12段落050。"
    )
    [tokens] = front.sections
    assert tokens == (
        "SVE-KIT 合成F12段落021。{ニュートラル}SVE-KIT 合成F12段落022。"
        "{コスト1}{攻撃力}SVE-KIT 合成F12段落023。{体力}SVE-KIT 合成F12段落024。\n"
        "SVE-KIT 合成F12段落025。{エルフ}SVE-KIT 合成F12段落026。"
        "{コスト0}{攻撃力}SVE-KIT 合成F12段落027。{体力}SVE-KIT 合成F12段落028。"
        "{ファンファーレ}SVE-KIT 合成F12段落029。{体力}SVE-KIT 合成F12段落030。"
    )


def test_errata_release_date_and_rulings() -> None:
    card = extract_card(fixture("F08-card.html"), number="SYN01-008")
    assert card.errata_url == "https://shadowverse-evolve.com/errata/synthetic-f08/"
    assert card.release_date == "2099-01-02"
    assert card.products[0].date == "2099-01-02"
    assert card.products[0].name == "SVE-KIT 合成商品 F08"
    assert card.products[0].links == [
        "/products/synthetic-f08/",
        "/products/synthetic-f08-alternate/",
    ]
    # A notice sharing the credit class must not become the illustrator.
    assert card.faces[0].illustrator == "SVE-KIT 合成插畫者 F08-01"
    assert card.faces[0].traits == ["合成特性甲"]
    assert card.notes == [
        "SVE-KIT 合成F08段落024。SVE-KIT 合成F08段落025。SVE-KIT 合成F08段落026。"
    ]
    assert len(card.qa) == 3
    first = card.qa[0]
    assert isinstance(first, QA)
    assert first == QA(
        id="Q90001",
        date="2099-01-02",
        question="SVE-KIT 合成F08段落035。",
        answer="SVE-KIT 合成F08段落037。",
    )
    assert not first.question.startswith("Q")
    assert not first.answer.startswith("A")


def test_flavor_keeps_line_breaks() -> None:
    card = extract_card(fixture("F07-card.html"), number="SYN01-007")
    assert card.faces[0].traits == ["合成特性甲"]
    assert card.faces[0].flavor == (
        "SVE-KIT 合成F07段落022。\nSVE-KIT 合成F07段落023。"
    )


def test_compound_trait_is_not_split() -> None:
    body = fixture("F10-card.html").replace(
        "<dd>合成特性甲</dd>".encode(),
        "<dd>合成特性甲・ジオ・テオゴニア・合成特性乙</dd>".encode(),
    )
    [face] = extract_card(body, number="SYN01-010").faces
    assert face.traits == ["合成特性甲", "ジオ・テオゴニア", "合成特性乙"]
    assert face.trait_raw == "合成特性甲・ジオ・テオゴニア・合成特性乙"


def test_angle_bracket_trait_keeps_its_inner_separator() -> None:
    original = fixture("F10-card.html")
    row = "<dd>合成特性甲</dd>".encode()
    assert original.count(row) == 1
    body = original.replace(row, "<dd>〈合成・括號〉・合成特性乙</dd>".encode())
    [face] = extract_card(body, number="SYN01-010").faces
    assert face.trait_raw == "〈合成・括號〉・合成特性乙"
    assert face.traits == ["〈合成・括號〉", "合成特性乙"]


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
    <div class="cardlist-Detail_Relation">
    <a href="/cardlist/?cardno=SYN01-007">関連カード</a></div></div>
    <header><a href="/cardlist/?cardno={number}">自己導覽</a></header>
    <footer><a href="/cardlist/?cardno=PR-001">共有</a></footer>
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
    assert [hint.href for hint in record.related_cards] == [
        "/cardlist/?cardno=SYN01-007"
    ]


def test_missing_ability_differs_from_present_empty_ability() -> None:
    body = fixture("F10-card.html")
    start = body.index(b'<div class="detail">')
    end = body.index(b"</div>", start) + len(b"</div>")
    missing = extract_card(body[:start] + body[end:], number="SYN01-010")
    empty = extract_card(
        body[:start] + b'<div class="detail"></div>' + body[end:],
        number="SYN01-010",
    )
    assert missing.faces[0].text is None
    assert empty.faces[0].text == ""  # ruff: ignore[compare-to-empty-string] -- distinguish present empty from missing


def test_empty_section_keeps_its_position() -> None:
    body = fixture("F10-card.html")
    start = body.index(b'<div class="detail">')
    end = body.index(b"</div>", start) + len(b"</div>")
    section_only = (
        body[:start] + "<div class='detail'>―――――</div>".encode() + body[end:]
    )
    [face] = extract_card(section_only, number="SYN01-010").faces
    assert face.text == ""  # ruff: ignore[compare-to-empty-string] -- empty leading section is present
    assert face.sections == [""]


def test_missing_image_src_is_rejected() -> None:
    body = fixture("F10-card.html").replace(
        b'<img src="/wordpress/wp-content/images/cardlist/synthetic/SYN01-010-1.png"',
        b"<img",
    )
    with pytest.raises(
        ValidationError, match=r"^SYN01-010 has a card image without src$"
    ):
        extract_card(body, number="SYN01-010")


def test_empty_card_name_is_rejected() -> None:
    body = fixture("F10-card.html").replace(
        '<h1 class="ttl Sans">SVE-KIT 合成測試卡 F10-01</h1>'.encode(),
        b'<h1 class="ttl Sans"></h1>',
    )
    with pytest.raises(ValidationError, match=r"^SYN01-010 has an empty card name$"):
        extract_card(body, number="SYN01-010")


def test_page_without_card_detail_is_rejected() -> None:
    body = (
        b"<!DOCTYPE html><html><body>" + b"<p>maintenance</p>" * 100 + b"</body></html>"
    )
    with pytest.raises(Exception, match="cardlist-Detail"):
        extract_card(body, number="BP01-001")


def test_missing_info_row_is_rejected() -> None:
    row = "<dl><dt>クラス</dt><dd>ナイトメア</dd></dl>".encode()
    original = fixture("F10-card.html")
    assert row in original
    for replacement in (b"", "<dl><dt>合成未知欄位</dt><dd>合成值</dd></dl>".encode()):
        body = original.replace(row, replacement)
        with pytest.raises(ValidationError, match=r"^card info has no 'クラス'$"):
            extract_card(body, number="SYN01-010")


def test_empty_required_info_is_rejected() -> None:
    body = fixture("F10-card.html").replace(
        "<dt>クラス</dt><dd>ナイトメア</dd>".encode(),
        "<dt>クラス</dt><dd></dd>".encode(),
    )
    with pytest.raises(ValidationError, match=r"^card info has empty 'クラス'$"):
        extract_card(body, number="SYN01-010")


def test_empty_status_is_rejected() -> None:
    body = fixture("F10-card.html").replace(
        'heading-Cost">コスト</span>3'.encode(),
        'heading-Cost">コスト</span>'.encode(),
    )
    with pytest.raises(ValidationError, match=r"^card status has empty cost$"):
        extract_card(body, number="SYN01-010")


def test_duplicate_info_key_is_rejected() -> None:
    row = "<dl><dt>クラス</dt><dd>ナイトメア</dd></dl>".encode()
    body = fixture("F10-card.html").replace(row, row + row)
    with pytest.raises(
        ValidationError, match=r"^SYN01-010 has duplicate card info 'クラス'$"
    ):
        extract_card(body, number="SYN01-010")


@pytest.mark.parametrize("traits", ["合成甲・・合成乙", "・合成甲", "合成甲・"])
def test_malformed_trait_separators_are_rejected(traits: str) -> None:
    body = fixture("F10-card.html").replace(
        "<dd>合成特性甲</dd>".encode(),
        f"<dd>{traits}</dd>".encode(),
    )
    with pytest.raises(
        ValidationError, match=rf"^malformed trait list {re.escape(repr(traits))}$"
    ):
        extract_card(body, number="SYN01-010")


def test_present_empty_illustrator_heading_is_empty_text() -> None:
    body = fixture("F10-card.html").replace(
        '<span class="heading">SVE-KIT 合成插畫者 F10-01</span>'.encode(),
        b'<span class="heading"></span>',
    )
    [face] = extract_card(body, number="SYN01-010").faces
    assert face.illustrator == ""  # ruff: ignore[compare-to-empty-string] -- present empty differs from missing


def test_extract_cards_writes_only_trusted_listed_cards(
    manifest: Manifest, tmp_path: Path
) -> None:
    # No validated lists yet: nothing to extract, but the file is still replaced cleanly.
    dest = tmp_path / "derived" / "cards.jsonl"
    report = extract_cards(manifest, Writer(tmp_path / "data", manifest), dest)
    assert report.written == 0
    assert dest.read_bytes() == b""
    assert list(dest.parent.glob(".tmp-*")) == []


def test_status_without_heading_is_rejected() -> None:
    body = fixture("F10-card.html").replace(
        '<span class="heading heading-Cost">コスト</span>'.encode(),
        b"",
    )
    with pytest.raises(ValidationError, match=r"^card status has no \['cost'\]$"):
        extract_card(body, number="SYN01-010")

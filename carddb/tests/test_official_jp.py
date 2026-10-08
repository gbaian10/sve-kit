from pathlib import Path, PurePosixPath

import pytest

from sve_carddb.ingest.http.validate import ValidationError
from sve_carddb.parse.html import MissingElementError
from sve_carddb.parse.pages import official_jp as jp

FIXTURES = Path(__file__).parent / "fixtures" / "synthetic_jp"
MAINTENANCE = (
    "<!DOCTYPE html><html><body><header>Shadowverse EVOLVE</header>"
    + "<p>ただいまメンテナンス中です</p>" * 50
    + "</body></html>"
).encode()


def fixture(name: str) -> bytes:
    return (FIXTURES / name).read_bytes()


def test_urls() -> None:
    assert jp.sets_url() == "https://shadowverse-evolve.com/cardlist/"
    assert jp.list_url("BP01", 1) == (
        "https://shadowverse-evolve.com/cardlist/cardsearch/?expansion_name=BP01&view=text"
    )
    assert jp.list_url("BP01", 2) == (
        "https://shadowverse-evolve.com/cardlist/cardsearch_ex"
        "?expansion_name=BP01&page=2&view=text"
    )
    assert jp.card_url("BP03-LDⓈ01") == (
        "https://shadowverse-evolve.com/cardlist/?cardno=BP03-LD%E2%93%8801"
    )


def test_image_url_and_path_keep_the_official_file_name() -> None:
    url = jp.image_url("/wordpress/wp-content/images/cardlist/ETD01/etd01-002 .png")
    assert url.endswith("/cardlist/ETD01/etd01-002%20.png")
    assert jp.image_path(url) == PurePosixPath("media/images/jp/ETD01/etd01-002 .png")


def test_paths() -> None:
    assert jp.sets_path() == PurePosixPath("raw/jp/sets.html.zst")
    assert jp.list_path("BP01", 3) == PurePosixPath("raw/jp/list/BP01/3.html.zst")
    assert jp.card_path("BP03-LDⓈ01") == PurePosixPath(
        "raw/jp/card/BP03-LDⓈ01.html.zst"
    )


@pytest.mark.parametrize(
    ("url", "expected"),
    [
        ("https://shadowverse-evolve.com/cardlist/", True),
        ("http://shadowverse-evolve.com/cardlist/", False),
        ("https://en.shadowverse-evolve.com/cards/", False),
        ("https://shadowverse-evolve.com.evil.example/", False),
    ],
)
def test_allowed(url: str, *, expected: bool) -> None:
    assert jp.allowed(url) is expected


def test_parse_sets() -> None:
    sets = jp.parse_sets(fixture("F16-sets.html"))
    assert len(sets) == 55
    assert sets[0] == jp.CardSet(code="SYN01", name="SVE-KIT 合成F16段落002。")
    assert sets[1] == jp.CardSet(code="SN02", name="SVE-KIT 合成F16段落003。")
    assert sets[-6] == jp.CardSet(code="SYN50A", name="SVE-KIT 合成F16段落051。")
    assert sets[-1] == jp.CardSet(code="SN", name="SVE-KIT 合成F16段落056。")
    assert [s.code for s in sets] == [
        "SYN01",
        *[f"SN{i:02}" for i in range(2, 36)],
        *[f"SYN{i:02}" for i in range(36, 50)],
        *[f"SYN{i:02}A" for i in range(50, 55)],
        "SN",
    ]
    assert all(s.code for s in sets)


def test_parse_sets_rejects_a_page_without_the_form() -> None:
    with pytest.raises(MissingElementError, match="expansion_name"):
        jp.parse_sets(MAINTENANCE)


def test_parse_list_first() -> None:
    page = jp.parse_list_first(fixture("F13-list.html"))
    assert page.total == 273
    assert page.max_page == 19
    assert page.card_numbers == [f"SYN01-{i:03}" for i in range(1, 16)]


def test_parse_list_first_rejects_inconsistent_counts() -> None:
    original = fixture("F13-list.html")
    for before, after, message in (
        (
            b"max_page = 19;",
            b"max_page = 20;",
            r"^273 cards do not fit 20 pages of 15$",
        ),
        (b">273<", b">270<", r"^270 cards do not fit 19 pages of 15$"),
    ):
        assert before in original
        body = original.replace(before, after)
        with pytest.raises(ValidationError, match=message):
            jp.parse_list_first(body)


def test_parse_list_first_rejects_a_page_without_max_page() -> None:
    body = fixture("F13-list.html").replace(b"var max_page = 19;", b"")
    with pytest.raises(ValidationError, match="max_page"):
        jp.parse_list_first(body)


def test_parse_list_first_rejects_a_maintenance_page() -> None:
    with pytest.raises(MissingElementError):
        jp.parse_list_first(MAINTENANCE)


def test_parse_list_more_middle_and_last_page() -> None:
    middle = jp.parse_list_more(
        fixture("F14-list.html"), page=18, max_page=19, total=273
    )
    last = jp.parse_list_more(fixture("F15-list.html"), page=19, max_page=19, total=273)
    assert middle.card_numbers == [f"SN01-U{i:02}" for i in range(16, 31)]
    assert last.card_numbers == ["SN01-U31", "SN01-U32", "SN01-U33"]


def test_parse_list_more_rejects_a_short_middle_page() -> None:
    with pytest.raises(ValidationError, match="expected 15"):
        jp.parse_list_more(fixture("F15-list.html"), page=18, max_page=19, total=273)


def test_parse_list_more_rejects_an_empty_response() -> None:
    with pytest.raises(ValidationError, match="at least"):
        jp.parse_list_more(b"", page=19, max_page=19, total=273)


def test_parse_list_more_rejects_a_page_without_cards() -> None:
    with pytest.raises(ValidationError, match="has 0 cards"):
        jp.parse_list_more(b"<ul></ul>", page=19, max_page=19, total=273)


def test_card_without_illustrator_has_the_number_in_heading() -> None:
    card = jp.parse_card(fixture("F09-card.html"), expected_number="SYN01-009")
    assert card.card_number == "SYN01-009"
    assert card.name == "SVE-KIT 合成測試卡 F09-01"
    assert card.image_originals == [
        "/wordpress/wp-content/images/cardlist/synthetic/SYN01-009-1.png"
    ]
    assert card.image_urls == [
        "https://shadowverse-evolve.com/wordpress/wp-content/images/cardlist/synthetic/SYN01-009-1.png"
    ]


def test_card_with_illustrator_has_the_number_in_name() -> None:
    card = jp.parse_card(fixture("F10-card.html"), expected_number="SYN01-010")
    assert card.card_number == "SYN01-010"


def test_alternate_art_keeps_its_own_image() -> None:
    card = jp.parse_card(fixture("F11-card.html"), expected_number="SYN01-SP01")
    assert card.image_originals == [
        "/wordpress/wp-content/images/cardlist/synthetic/SYN01-SP01-1.png"
    ]


def test_card_page_must_be_the_requested_card() -> None:
    with pytest.raises(
        ValidationError, match=r"^asked for SYN01-099, page shows SYN01-009$"
    ):
        jp.parse_card(fixture("F09-card.html"), expected_number="SYN01-099")


def test_card_page_rejects_a_maintenance_page() -> None:
    with pytest.raises(MissingElementError, match="cardlist-Detail"):
        jp.parse_card(MAINTENANCE, expected_number="BP02-070")


def test_list_numbers_outside_list_items_are_ignored() -> None:
    body = fixture("F13-list.html") + b'<span class="number">SYN01-OUTSIDE</span>'
    assert jp.parse_list_first(body).card_numbers == [
        f"SYN01-{i:03}" for i in range(1, 16)
    ]


def test_duplicate_list_number_is_rejected() -> None:
    body = fixture("F13-list.html").replace(b">SYN01-002<", b">SYN01-001<")
    with pytest.raises(ValidationError, match=r"^duplicate card numbers on one page$"):
        jp.parse_list_first(body)


def test_product_code_trailing_character_is_rejected() -> None:
    body = fixture("F16-sets.html").replace(b'value="SYN01"', b'value="SYN01!"')
    with pytest.raises(ValidationError, match=r"^unexpected product code 'SYN01!'$"):
        jp.parse_sets(body)

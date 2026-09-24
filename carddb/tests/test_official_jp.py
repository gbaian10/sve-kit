from pathlib import Path, PurePosixPath

import pytest

from sve_carddb.fetch.validate import ValidationError
from sve_carddb.html import MissingElementError
from sve_carddb.sources import official_jp as jp

FIXTURES = Path(__file__).parent / "fixtures" / "official_jp"
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
    sets = jp.parse_sets(fixture("sets.html"))
    assert len(sets) == 55
    assert sets[0].code == "PCS02"
    assert sets[-1] == jp.CardSet(code="PR", name="PRカード")
    assert all(s.code for s in sets)


def test_parse_sets_rejects_a_page_without_the_form() -> None:
    with pytest.raises(MissingElementError, match="expansion_name"):
        jp.parse_sets(MAINTENANCE)


def test_parse_list_first() -> None:
    page = jp.parse_list_first(fixture("list_BP01_1.html"))
    assert page.total == 273
    assert page.max_page == 19
    assert page.card_numbers[:3] == ["BP01-001", "BP01-002", "BP01-003"]
    assert len(page.card_numbers) == 15


def test_parse_list_first_rejects_inconsistent_counts() -> None:
    body = fixture("list_BP01_1.html").replace(b"max_page = 19;", b"max_page = 20;")
    with pytest.raises(ValidationError, match="do not fit"):
        jp.parse_list_first(body)


def test_parse_list_first_rejects_a_page_without_max_page() -> None:
    body = fixture("list_BP01_1.html").replace(b"var max_page = 19;", b"")
    with pytest.raises(ValidationError, match="max_page"):
        jp.parse_list_first(body)


def test_parse_list_first_rejects_a_maintenance_page() -> None:
    with pytest.raises(MissingElementError):
        jp.parse_list_first(MAINTENANCE)


def test_parse_list_more_middle_and_last_page() -> None:
    middle = jp.parse_list_more(
        fixture("list_BP01_18.html"), page=18, max_page=19, total=273
    )
    last = jp.parse_list_more(
        fixture("list_BP01_19.html"), page=19, max_page=19, total=273
    )
    assert len(middle.card_numbers) == 15
    assert last.card_numbers == ["BP01-U05", "BP01-U06", "BP01-U07"]


def test_parse_list_more_rejects_a_short_middle_page() -> None:
    with pytest.raises(ValidationError, match="expected 15"):
        jp.parse_list_more(
            fixture("list_BP01_19.html"), page=18, max_page=19, total=273
        )


def test_parse_list_more_rejects_an_empty_response() -> None:
    with pytest.raises(ValidationError, match="at least"):
        jp.parse_list_more(b"", page=19, max_page=19, total=273)


def test_parse_list_more_rejects_a_page_without_cards() -> None:
    with pytest.raises(ValidationError, match="has 0 cards"):
        jp.parse_list_more(b"<ul></ul>", page=19, max_page=19, total=273)


def test_card_without_illustrator_has_the_number_in_heading() -> None:
    card = jp.parse_card(fixture("card_BP02-070.html"), expected_number="BP02-070")
    assert card.card_number == "BP02-070"
    assert card.name
    assert card.image_originals == [
        "/wordpress/wp-content/images/cardlist/BP02/bp02_070.png"
    ]
    assert card.image_urls == [
        "https://shadowverse-evolve.com/wordpress/wp-content/images/cardlist/BP02/bp02_070.png"
    ]


def test_card_with_illustrator_has_the_number_in_name() -> None:
    card = jp.parse_card(fixture("card_BP02-071.html"), expected_number="BP02-071")
    assert card.card_number == "BP02-071"


def test_alternate_art_keeps_its_own_image() -> None:
    card = jp.parse_card(fixture("card_BP02-SP01.html"), expected_number="BP02-SP01")
    assert card.image_originals[0].endswith("/BP02/bp02_sp01_3.png")


def test_card_page_must_be_the_requested_card() -> None:
    with pytest.raises(ValidationError, match="asked for BP02-069"):
        jp.parse_card(fixture("card_BP02-070.html"), expected_number="BP02-069")


def test_card_page_rejects_a_maintenance_page() -> None:
    with pytest.raises(MissingElementError, match="cardlist-Detail"):
        jp.parse_card(MAINTENANCE, expected_number="BP02-070")

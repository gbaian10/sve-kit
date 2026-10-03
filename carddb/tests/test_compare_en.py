"""Offline EN measurement checks using existing fixtures and synthetic cards."""

import json
from pathlib import Path
from typing import TYPE_CHECKING, cast
from urllib.parse import parse_qs, urlsplit

if TYPE_CHECKING:
    import pytest
    from pydantic import JsonValue

from sve_carddb.extract import compare_en
from sve_carddb.fetch.validate import ValidationError
from sve_carddb.fetch.writer import LocalState
from sve_carddb.registry.inputs import Card
from sve_carddb.registry.review import observation


def _card(number: str, *, back_text: str = "unchanged") -> Card:
    return Card.model_validate(
        {
            "number": number,
            "faces": [
                {"name": "front", "text": "front text", "image": "/front.png"},
                {"name": "back", "text": back_text, "image": "/back.png"},
            ],
        }
    )


def test_candidate_parser_uses_english_page_fields() -> None:
    body = (
        Path(__file__).parent / "fixtures" / "synthetic_en" / "F01-card.html"
    ).read_bytes()
    card = compare_en.parse_card(body, "SYN01-001EN")
    assert card.number == "SYN01-001EN"
    [face] = card.faces
    assert face.name == "SVE-KIT 合成測試卡 F01-01"
    assert face.info == {
        "Format": "Any",
        "Class": "Forestcraft",
        "Card Type": "Follower",
        "Trait": "SyntheticAlpha/SyntheticBeta",
        "Rarity": "Legendary",
        "Card Set": "SVE-KIT 合成商品 F01",
    }
    assert face.stats == {"cost": "3", "power": "3", "hp": "3"}
    assert (
        face.text
        == "{[fanfare]} SVE-KIT 合成F01段落020。\n{[act]}{[engage]}SVE-KIT 合成F01段落021。"
    )
    assert face.speech == "{[forestcraft]}{[cost02]} SVE-KIT 合成F01段落022。"
    assert (
        face.image
        == "/wordpress/wp-content/images/cardlist/synthetic/SYN01-001EN-1.png"
    )


def test_info_value_keeps_break_between_text_nodes() -> None:
    body = (
        "<html><div class='cardlist-Detail'><div class='cardlist-Detail_Box_Inner'>"
        "<div class='ttl'>Synthetic</div><div class='img'><img src='/synthetic.png'></div>"
        "<div class='info'><dl><dt>Card Set</dt>"
        "<dd>SVE-KIT synthetic set<br />\nSynthetic edition</dd></dl></div>"
        "<div class='status-Item status-Item-Cost'><span class='heading'>Cost</span>1</div>"
        "<div class='status-Item status-Item-Power'><span class='heading'>Power</span>2</div>"
        "<div class='status-Item status-Item-Hp'><span class='heading'>HP</span>3</div>"
        "<div class='illustrator'><span class='name'>SYN-001EN</span></div>"
        "</div></div><!-- " + "x" * 1100 + " --></html>"
    ).encode()
    card = compare_en.parse_card(body, "SYN-001EN")
    assert card.faces[0].info["Card Set"] == "SVE-KIT synthetic set\nSynthetic edition"


def test_length_distribution_separates_direction_and_common_deltas() -> None:
    items = cast(
        "list[dict[str, JsonValue]]",
        [
            {"old": compare_en._summary("abc"), "new": compare_en._summary("a")},
            {"old": compare_en._summary("def"), "new": compare_en._summary("d")},
            {"old": compare_en._summary("ab"), "new": compare_en._summary("cd")},
            {"old": compare_en._summary("a"), "new": compare_en._summary("abc")},
            {"old": None, "new": compare_en._summary("added")},
        ],
    )
    assert compare_en._length_distribution(items) == {
        "candidate_shorter": 2,
        "equal_length": 1,
        "candidate_longer": 1,
        "unpaired": 1,
        "common_deltas": [
            {"candidate_minus_legacy_bytes": -2, "count": 2},
            {"candidate_minus_legacy_bytes": 0, "count": 1},
            {"candidate_minus_legacy_bytes": 2, "count": 1},
        ],
    }


def test_measurement_partitions_statuses_and_diffs_every_face(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    numbers = ["A", "B", "C", "D", "E", "F"]
    legacy = {number: _card(number) for number in numbers if number != "E"}
    expected: list[tuple[str, str, str, str, str | None]] = []
    for number in numbers:
        observed = observation(_card(number), "en")
        expected.append(
            (
                number,
                "printing-" + number,
                "sha256:" + "0" * 64
                if number == "F"
                else str(observed["observation_hash"]),
                str(observed["rules_hash"]),
                "decision-" + number,
            )
        )

    class Reader:
        def local_state(self, url: str) -> LocalState:
            number = parse_qs(urlsplit(url).query)["cardno"][0]
            return LocalState.MISSING if number == "C" else LocalState.TRUSTED

        def read(self, _url: str) -> bytes:
            return b"synthetic raw"

    def candidate(_body: bytes, number: str) -> Card:
        if number == "D":
            raise ValidationError("synthetic parser failure")
        return _card(number, back_text="changed" if number == "B" else "unchanged")

    monkeypatch.setattr(compare_en, "parse_card", candidate)
    report = compare_en.measure(expected, legacy, Reader())
    assert report["denominator"] == 6
    assert report["counts"] == {
        "exact": 1,
        "mismatch": 1,
        "missing_raw": 1,
        "parse_failed": 1,
        "no_corresponding_input": 2,
    }
    assert report["rates"] == {
        "all_registered": 1 / 6,
        "comparable": 1 / 2,
        "comparable_denominator": 2,
    }
    groups = cast("dict[str, dict[str, object]]", report["field_groups"])
    assert set(groups) == {"card.faces[].text"}
    assert groups["card.faces[].text"]["face_indexes"] == [1]
    assert groups["card.faces[].text"]["card_numbers"] == ["B"]
    assert groups["card.faces[].text"]["length_delta_bytes"] == {
        "candidate_shorter": 1,
        "equal_length": 0,
        "candidate_longer": 0,
        "unpaired": 0,
        "common_deltas": [{"candidate_minus_legacy_bytes": -2, "count": 1}],
    }
    rows = cast("list[dict[str, object]]", report["cards"])
    assert [row["status"] for row in rows] == [
        "exact",
        "mismatch",
        "missing_raw",
        "parse_failed",
        "no_corresponding_input",
        "no_corresponding_input",
    ]
    assert rows[4]["input_reason"] == "missing_number"
    assert rows[5]["input_reason"] == "registry_hash_mismatch"
    assert set(cast("dict[str, object]", report["affected_decisions"])) == {
        "decision-B",
        "decision-C",
        "decision-D",
        "decision-E",
        "decision-F",
    }
    assert "front text" not in json.dumps(report)
    assert "changed" not in json.dumps(report)

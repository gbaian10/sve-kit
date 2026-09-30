"""Offline EN measurement checks using existing fixtures and synthetic cards."""

import json
from pathlib import Path
from typing import TYPE_CHECKING, cast
from urllib.parse import parse_qs, urlsplit

if TYPE_CHECKING:
    import pytest

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
        Path(__file__).parent / "fixtures" / "official_en" / "card_BP01-001EN.html"
    ).read_bytes()
    card = compare_en.parse_card(body, "BP01-001EN")
    assert len(card.faces) == 1
    assert set(card.faces[0].stats) == {"cost", "power", "hp"}
    assert {"Class", "Card Type", "Trait"} <= card.faces[0].info.keys()
    assert card.faces[0].image.endswith("/BP01-001EN.png")


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

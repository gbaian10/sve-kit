"""Synthetic evidence checks; no local card cache or manifest is opened."""

from __future__ import annotations

import json
from dataclasses import asdict, replace
from typing import TYPE_CHECKING, cast

if TYPE_CHECKING:
    from pathlib import Path

    import pytest

from sve_carddb.extract import compare_jp
from sve_carddb.extract.official_jp import CardRecord, Face
from sve_carddb.registry.review import observation
from sve_carddb.registry.storage import Entry, Index


def synthetic() -> CardRecord:
    return CardRecord(
        number="BP01-001a",
        faces=[
            Face(
                name="名前",
                card_class="エルフ",
                card_type="フォロワー",
                traits=["ジオ・テオゴニア"],
                rarity="LG",
                product=None,
                title=None,
                cost="-",
                power="2",
                hp="3",
                text="",
                sections=["token definition"],
                flavor="flavor",
                illustrator=None,
                image="/image.png",
                trait_raw="ジオ・テオゴニア",
            )
        ],
        release_date=None,
        errata_url=None,
        notes=[],
        qa=[],
        products=[],
        related_cards=[],
    )


def files(tmp_path: Path, record: CardRecord) -> tuple[Path, Path]:
    legacy = tmp_path / "legacy.jsonl"
    candidate = tmp_path / "candidate.jsonl"
    legacy.write_text(
        compare_jp.legacy_projection(record).model_dump_json() + "\n",
        encoding="utf-8",
    )
    candidate.write_text(
        json.dumps(asdict(record), ensure_ascii=False) + "\n", encoding="utf-8"
    )
    return legacy, candidate


def test_legacy_adapter_keeps_old_projection_separate_from_typed_flavor() -> None:
    card = compare_jp.legacy_projection(synthetic())
    [face] = card.faces
    assert face.speech is None
    assert synthetic().faces[0].flavor == "flavor"
    assert face.text == ""  # ruff: ignore[compare-to-empty-string] -- distinguish present empty from missing
    assert face.sections == ["token definition"]
    assert face.image == "/image.png"
    assert face.traits == ["ジオ・テオゴニア"]


def test_comparison_matches_envelope_then_reports_field_differences(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    record = synthetic()
    legacy, candidate = files(tmp_path, record)
    envelope = observation(compare_jp.legacy_projection(record), "jp")
    envelope["role"] = "to"
    entry = Entry(
        record_key="printing:one",
        kind="printing",
        owner="BP01",
        data={"observation": envelope},
    )
    monkeypatch.setattr(compare_jp, "load", lambda _: (Index(), {"one": entry}))
    exact = compare_jp.compare(legacy, candidate, tmp_path, {record.number})
    assert exact["complete"] is True
    assert exact["counts"] == {"exact": 1}
    [row] = cast("list[dict[str, object]]", exact["cards"])
    assert row["candidate_projection_sha256"] != envelope["observation_hash"]

    changed = replace(record, faces=[replace(record.faces[0], text="new text")])
    candidate.write_text(
        json.dumps(asdict(changed), ensure_ascii=False) + "\n", encoding="utf-8"
    )
    report = compare_jp.compare(legacy, candidate, tmp_path, {record.number})
    assert report["complete"] is False
    assert report["field_counts"] == {"card.faces[0].text": 1}
    [row] = cast("list[dict[str, object]]", report["cards"])
    assert row["status"] == "envelope_difference"
    assert [
        diff["field"] for diff in cast("list[dict[str, object]]", row["envelope_diffs"])
    ] == [
        "observation_hash",
        "rules_hash",
    ]
    assert "new text" not in json.dumps(report)


def test_missing_candidate_cannot_claim_complete(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    record = synthetic()
    legacy, candidate = files(tmp_path, record)
    candidate.write_bytes(b"")
    entry = Entry(
        record_key="printing:one",
        kind="printing",
        owner="BP01",
        data={"observation": observation(compare_jp.legacy_projection(record), "jp")},
    )
    monkeypatch.setattr(compare_jp, "load", lambda _: (Index(), {"one": entry}))
    report = compare_jp.compare(legacy, candidate, tmp_path, {record.number})
    assert report["complete"] is False
    assert report["counts"] == {"missing_candidate": 1}


def test_partial_catalog_cannot_claim_complete(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    record = synthetic()
    legacy, candidate = files(tmp_path, record)
    entry = Entry(
        record_key="printing:one",
        kind="printing",
        owner="BP01",
        data={"observation": observation(compare_jp.legacy_projection(record), "jp")},
    )
    monkeypatch.setattr(compare_jp, "load", lambda _: (Index(), {"one": entry}))
    report = compare_jp.compare(
        legacy, candidate, tmp_path, {record.number, "BP01-002"}
    )
    assert report["complete"] is False
    assert cast("dict[str, int]", report["denominators"])["expected"] == 2
    assert report["counts"] == {"exact": 1, "missing_legacy": 1}


def test_unexpected_number_and_recipe_change_are_reported(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    record = synthetic()
    legacy, candidate = files(tmp_path, record)
    envelope = observation(compare_jp.legacy_projection(record), "jp")
    envelope["recipe"] = "unexpected-recipe"
    entry = Entry(
        record_key="printing:one",
        kind="printing",
        owner="BP01",
        data={"observation": envelope},
    )
    monkeypatch.setattr(compare_jp, "load", lambda _: (Index(), {"one": entry}))
    report = compare_jp.compare(legacy, candidate, tmp_path, set())
    assert report["complete"] is False
    assert report["counts"] == {"unexpected": 1}
    assert cast("dict[str, list[str]]", report["coverage"])["unexpected_candidate"] == [
        record.number
    ]
    [row] = cast("list[dict[str, object]]", report["cards"])
    assert {
        diff["field"] for diff in cast("list[dict[str, object]]", row["envelope_diffs"])
    } == {"recipe"}

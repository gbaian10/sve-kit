"""Inventory conclusions are input-bound and never infer or write permanent identity."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.domains.registry.english_inventory import (
    Batch,
    Conclusion,
    JPBaseline,
    inventory,
    read_jp_baseline,
)
from sve_carddb.domains.registry.records import EnglishPrintingData
from sve_carddb.domains.registry.review import observation
from sve_carddb.domains.registry.snapshot import load_registry

from ...support.registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose synthetic fixture
from .test_registry import card
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose synthetic fixture dependency

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.core.regions import Region
    from sve_carddb.domains.registry.inputs import Card
    from sve_carddb.domains.registry.review import Inputs


def batch(region: Region, cards: dict[str, Card]) -> Batch:
    return Batch(
        region,
        {"batch_id": "sha256:" + "1" * 64, "expected_sources": len(cards)},
        cards,
        {
            number: {"observation": observation(item, region)}
            for number, item in cards.items()
        },
    )


def conclusion(jp: Batch, en: Batch, **changes: JsonValue) -> Conclusion:
    value: dict[str, JsonValue] = {
        "en_card_no": "GF01-001EN",
        "source_index": 0,
        "en_observation_hash": observation(en.cards["GF01-001EN"], "en")[
            "observation_hash"
        ],
        "jp_coverage_hash": jp.coverage_hash,
        "classification": "confirmed_no_jp",
        "reason": "Synthetic identity review of the complete input",
        "compared_fields": ["name", "class", "type", "stats", "text", "image"],
    }
    value.update(changes)
    return Conclusion.model_validate_json(canonical(value))


def rows(report: dict[str, JsonValue]) -> list[dict[str, JsonValue]]:
    return [object_value(row) for row in array(report["candidates"])]


def test_recomputes_candidates_without_name_or_suffix_pairing(
    registry_root: Path, inputs: Inputs
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    en.cards["BP02-071EN"] = card("BP02-071EN", "名前", english=True)
    en.sources["BP02-071EN"] = {
        "observation": observation(en.cards["BP02-071EN"], "en")
    }
    report = inventory(load_registry(registry_root), jp, en)
    found = rows(report)
    assert {row["en_card_no"] for row in found} == {"GF01-001EN", "BP02-071EN"}
    assert all(row["classification"] == "unresolved" for row in found)
    assert report["counts"] == {"has_jp": 0, "confirmed_no_jp": 0, "unresolved": 2}
    assert {row["reason"] for row in found} == {
        "historical_absence_requires_current_conclusion",
        "no_manual_identity_conclusion",
    }
    encoded = canonical(report)
    assert b"Rule." not in encoded
    assert "名前".encode() not in encoded


def test_explicit_absence_reports_frozen_evidence_and_preserves_files(
    registry_root: Path, inputs: Inputs
) -> None:
    before = {path: path.read_bytes() for path in registry_root.rglob("*.yaml")}
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    registry = load_registry(registry_root)
    report = inventory(registry, jp, en, (conclusion(jp, en),))
    row = rows(report)[0]
    assert row["classification"] == "confirmed_no_jp"
    assert row["permanent_identity_change"] is False
    assert row["printing_id"] is not None
    assert row["face_id"] is not None
    assert (
        object_value(row["face"])["image_url"] == inputs.en["GF01-001EN"].faces[0].image
    )
    assert before == {path: path.read_bytes() for path in before}
    assert report == inventory(registry, jp, en, (conclusion(jp, en),))


@pytest.mark.parametrize("field", ["en_observation_hash", "jp_coverage_hash"])
def test_changed_inputs_never_inherit_conclusion(
    registry_root: Path, inputs: Inputs, field: str
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    decision = conclusion(jp, en, **{field: "sha256:" + "f" * 64})
    row = rows(inventory(load_registry(registry_root), jp, en, (decision,)))[0]
    assert row["classification"] == "unresolved"
    assert row["reason"] == "conclusion_input_changed"


def test_partial_jp_input_cannot_confirm_absence(
    registry_root: Path, inputs: Inputs
) -> None:
    jp = replace(
        batch("jp", inputs.jp), failures=({"reason": "source_projection_failed"},)
    )
    en = batch("en", inputs.en)
    row = rows(inventory(load_registry(registry_root), jp, en, (conclusion(jp, en),)))[
        0
    ]
    assert row["classification"] == "unresolved"
    assert row["reason"] == "jp_coverage_incomplete"


def test_manual_target_is_reported_as_separate_permanent_change(
    registry_root: Path, inputs: Inputs
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    decision = conclusion(
        jp,
        en,
        classification="has_jp",
        jp_card_no="BP02-071",
        jp_source_index=0,
        jp_observation_hash=observation(inputs.jp["BP02-071"], "jp")[
            "observation_hash"
        ],
    )
    registry = load_registry(registry_root)
    before = dict(registry.records)
    row = rows(inventory(registry, jp, en, (decision,)))[0]
    assert row["classification"] == "has_jp"
    assert object_value(row["jp_target"])["card_no"] == "BP02-071"
    assert row["permanent_identity_change"] is True
    assert registry.records == before


@pytest.mark.parametrize(
    "change",
    [
        {"jp_card_no": "absent"},
        {"jp_source_index": 1},
        {"jp_observation_hash": "sha256:" + "f" * 64},
    ],
)
def test_manual_target_must_pin_a_real_current_face(
    registry_root: Path, inputs: Inputs, change: dict[str, JsonValue]
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    values: dict[str, JsonValue] = {
        "classification": "has_jp",
        "jp_card_no": "BP02-071",
        "jp_source_index": 0,
        "jp_observation_hash": observation(inputs.jp["BP02-071"], "jp")[
            "observation_hash"
        ],
    }
    values.update(change)
    row = rows(
        inventory(load_registry(registry_root), jp, en, (conclusion(jp, en, **values),))
    )[0]
    assert row["classification"] == "unresolved"
    assert row["reason"] == "conclusion_jp_target_unavailable_or_changed"


def test_all_faces_and_unavailable_sources_stay_in_inventory(
    registry_root: Path, inputs: Inputs
) -> None:
    en = batch("en", {})
    en.cards["SYN-002EN"] = card("SYN-002EN", "Synthetic")
    en.cards["SYN-002EN"].faces.append(en.cards["SYN-002EN"].faces[0].model_copy())
    en.sources["SYN-002EN"] = {"source_version_id": "synthetic"}
    found = rows(inventory(load_registry(registry_root), batch("jp", inputs.jp), en))
    assert {(row["en_card_no"], row["source_index"]) for row in found} == {
        ("GF01-001EN", 0),
        ("SYN-002EN", 0),
        ("SYN-002EN", 1),
    }
    missing = next(row for row in found if row["en_card_no"] == "GF01-001EN")
    assert missing["reason"] == "en_source_or_face_unavailable"


def test_duplicate_or_non_candidate_conclusion_rejected(
    registry_root: Path, inputs: Inputs
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    registry = load_registry(registry_root)
    decision = conclusion(jp, en)
    with pytest.raises(ValueError, match="Duplicate"):
        inventory(registry, jp, en, (decision, decision))
    with pytest.raises(ValueError, match="outside"):
        inventory(registry, jp, en, (conclusion(jp, en, en_card_no="BP02-070EN"),))
    with pytest.raises(ValueError, match="that order"):
        inventory(registry, en, jp)


def test_shared_back_face_is_not_inferred_from_source_order(
    registry_root: Path, inputs: Inputs
) -> None:
    registry = load_registry(registry_root)
    en = batch("en", inputs.en)
    en.cards["BP02-070EN"].faces.append(en.cards["BP02-070EN"].faces[0].model_copy())
    found = rows(inventory(registry, batch("jp", inputs.jp), en))
    assert ("BP02-070EN", 1) in {
        (row["en_card_no"], row["source_index"]) for row in found
    }
    assert any(
        isinstance(record.data, EnglishPrintingData)
        for record in registry.records.values()
    )


def test_historical_baseline_is_diagnostic_and_never_grants_absence(
    registry_root: Path, inputs: Inputs
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    baseline = JPBaseline(
        inputs.jp_hash,
        {number: value.model_copy(deep=True) for number, value in inputs.jp.items()},
    )
    registry = load_registry(registry_root)
    report = inventory(registry, jp, en, jp_baseline=baseline)
    assert rows(report)[0]["classification"] == "unresolved"
    assert rows(report)[0]["reason"] == "historical_absence_requires_current_conclusion"
    coverage = object_value(object_value(report["inputs"])["historical_jp_coverage"])
    assert coverage["matches_current"] is True
    jp.cards["BP02-071"].faces[0].text = "Different synthetic rule."
    report = inventory(registry, jp, en, (conclusion(jp, en),), jp_baseline=baseline)
    assert rows(report)[0]["classification"] == "confirmed_no_jp"
    coverage = object_value(object_value(report["inputs"])["historical_jp_coverage"])
    assert coverage["matches_current"] is False
    assert coverage["changed_card_numbers"] == ["BP02-071"]


def test_private_jp_baseline_file_is_pinned_by_exact_bytes(
    tmp_path: Path, inputs: Inputs
) -> None:
    path = tmp_path / "jp.jsonl"
    path.write_text(
        "\n".join(card.model_dump_json() for card in inputs.jp.values()) + "\n"
    )
    result = read_jp_baseline(path)
    assert result.exact_sha256 == digest(path.read_bytes())
    assert result.cards == inputs.jp


def test_explicit_unresolved_and_absence_with_target(
    registry_root: Path, inputs: Inputs
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    registry = load_registry(registry_root)
    decision = conclusion(
        jp,
        en,
        classification="unresolved",
        reason="Synthetic identity remains ambiguous",
    )
    row = rows(inventory(registry, jp, en, (decision,)))[0]
    assert row["classification"] == "unresolved"
    assert row["reason"] == decision.reason
    invalid = conclusion(jp, en, jp_card_no="BP02-071")
    row = rows(inventory(registry, jp, en, (invalid,)))[0]
    assert row["reason"] == "absence_conclusion_has_target"


def test_unregistered_unparsed_en_card_remains_unresolved(
    registry_root: Path, inputs: Inputs
) -> None:
    en = replace(
        batch("en", inputs.en),
        failures=({"card_no": "SYN-UNKNOWN", "reason": "source_projection_failed"},),
    )
    found = rows(inventory(load_registry(registry_root), batch("jp", inputs.jp), en))
    unparsed = next(row for row in found if row["en_card_no"] == "SYN-UNKNOWN")
    assert unparsed["source_index"] is None
    assert unparsed["classification"] == "unresolved"
    assert unparsed["reason"] == "unparsed_en_face_inventory"


def test_new_source_face_needs_permanent_identity_review(
    registry_root: Path, inputs: Inputs
) -> None:
    jp, en = batch("jp", inputs.jp), batch("en", inputs.en)
    en.cards["GF01-001EN"].faces.append(en.cards["GF01-001EN"].faces[0].model_copy())
    decision = conclusion(jp, en, source_index=1)
    found = rows(inventory(load_registry(registry_root), jp, en, (decision,)))
    row = next(row for row in found if row["source_index"] == 1)
    assert row["classification"] == "confirmed_no_jp"
    assert row["face_id"] is None
    assert row["permanent_identity_change"] is True

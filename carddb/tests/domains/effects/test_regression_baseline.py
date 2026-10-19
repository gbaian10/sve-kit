"""Freeze the reviewed inputs without treating annotation coverage as execution."""

import hashlib
import json
from collections import defaultdict
from pathlib import Path

import pytest
from pydantic import JsonValue, TypeAdapter

from sve_carddb.core.yaml import JSON_VALUE, parse_yaml
from sve_carddb.domains.effects.reader import Catalog, load

ROOT = Path(__file__).resolve().parents[4]
BASELINE = ROOT / "carddb/tests/fixtures/effect_baseline"
OBJECT = TypeAdapter(dict[str, JsonValue])


def object_value(value: JsonValue) -> dict[str, JsonValue]:
    assert isinstance(value, dict)
    return value


def array(value: JsonValue) -> list[JsonValue]:
    assert isinstance(value, list)
    return value


def read_yaml(path: Path) -> dict[str, JsonValue]:
    return OBJECT.validate_python(parse_yaml(path.read_bytes()))


def fingerprint(value: JsonValue) -> str:
    encoded = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    )
    return hashlib.sha256(encoded.encode()).hexdigest()


def location(ability: dict[str, JsonValue]) -> str:
    return json.dumps(
        [
            ability.get("face", 0),
            "section" if "section" in ability else "effect",
            ability.get("section"),
            ability["line"],
        ]
    )


def test_baseline_covers_every_authored_pack() -> None:
    assert {p.stem for p in BASELINE.glob("*.json")} == {
        p.stem for p in (ROOT / "authored/rules/effects").glob("*.yaml")
    }


@pytest.mark.parametrize("path", sorted(BASELINE.glob("*.json")), ids=lambda p: p.stem)
def test_handwritten_programs_keep_the_reviewed_positions_and_inputs(
    path: Path,
) -> None:
    expected = OBJECT.validate_json(path.read_bytes())
    actual = object_value(
        read_yaml(ROOT / "authored/rules/effects" / f"{path.stem}.yaml")["cards"]
    )
    assert actual.keys() == expected.keys()
    for number, value in actual.items():
        baseline = object_value(expected[number])
        # Hash diagnostics cannot disclose official wording on a failing test.
        assert fingerprint(value) == baseline["ast_sha256"], number
        positions: dict[str, list[int]] = defaultdict(list)
        for index, ability in enumerate(array(object_value(value)["abilities"])):
            positions[location(object_value(ability))].append(index)
        frozen: dict[str, list[JsonValue]] = {}
        for value_row in array(baseline["positions"]):
            row = object_value(value_row)
            key = json.dumps([row[k] for k in ("face", "field", "section", "line")])
            assert key not in frozen, number
            frozen[key] = array(row["ability_indices"])
            fragments = [object_value(f) for f in array(row["fragments"])]
            assert fragments, number
            assert len({str(f["id"]) for f in fragments}) == len(fragments), number
        assert dict(positions) == frozen, number


@pytest.fixture(scope="module")
def classifications() -> Catalog:
    return load(ROOT / "authored")


def test_review_drafts_are_bound_to_the_classifications_they_explain(
    classifications: Catalog,
) -> None:
    records = {
        record.id: fingerprint(record.model_dump(mode="json"))
        for record in classifications.classifications
    }
    scenarios = (ROOT / "sim/engine/tests/boundaries.rs").read_text()
    seen: set[str] = set()
    conclusions = {"參數值差異", "等價寫法", "舊標註修正", "需來源變體", "尚未知"}
    for path in sorted((ROOT / "authored/rules/effect-reviews").glob("*.yaml")):
        draft = read_yaml(path)
        assert draft["status"] == "candidate"
        for value in array(draft["groups"]):
            group = object_value(value)
            identifier = str(group["id"])
            assert identifier not in seen, identifier
            seen.add(identifier)
            assert group["conclusion"] in conclusions, identifier
            assert group["rationale"], identifier
            for key, expected in object_value(group["classifications"]).items():
                assert records[key] == expected, key
            review = object_value(group["translation_review"])
            for field in ("required_uses", "conditional_uses"):
                assert set(map(str, array(review[field]))) <= set(
                    map(str, array(group["uses"]))
                ), identifier

        for value in array(draft.get("handwritten_differences", [])):
            difference = object_value(value)
            for key, expected in object_value(difference["classifications"]).items():
                assert records[key] == expected, key
            assert f"fn {difference['scenario']}()" in scenarios
            assert difference["conclusion"] in conclusions
            assert difference["rationale"]


def test_known_invalid_alignment_keeps_both_source_locations_visible() -> None:
    for path in (ROOT / "authored/rules/effect-reviews").glob("*.yaml"):
        if path.stem != "BP07":
            assert "positive_alignment_exceptions" not in read_yaml(path), path.stem
    draft = read_yaml(ROOT / "authored/rules/effect-reviews/BP07.yaml")
    baseline = OBJECT.validate_json((BASELINE / "BP07.json").read_bytes())
    exceptions = array(draft["positive_alignment_exceptions"])
    assert exceptions
    for value in exceptions:
        row = object_value(value)
        number = str(row["card_no"])
        assert row["status"] == "known_invalid_alignment"
        assert row["source_issue"] == "uncorrected_duplicate"
        card = object_value(baseline[number])
        source = object_value(row["position"])
        target = object_value(row["proposed_position"])
        positive = [
            object_value(p)
            for p in array(card["positions"])
            if all(object_value(p)[key] == value for key, value in source.items())
        ]
        assert len(positive) == 1, number
        assert row["ability"] in array(positive[0]["ability_indices"])
        reverse = [
            object_value(p)
            for p in array(draft["reverse_positions"])
            if object_value(p)["card_no"] == number
            and all(object_value(p)[key] == value for key, value in target.items())
        ]
        assert len(reverse) == 1, number
        assert object_value(reverse[0]["covered_by"])["ability"] == row["ability"]
        corrections = array(
            read_yaml(
                ROOT / "authored/registry/source-correction/active/BP07/001.yaml"
            )["records"]
        )
        assert row["source_correction"] in {
            str(object_value(object_value(c)["data"])["id"]) for c in corrections
        }


def test_reverse_positions_preserve_concrete_cross_line_coverage() -> None:
    categories = {"通用規則", "說明", "token", "跨行涵蓋", "真正缺漏"}
    seen: set[str] = set()
    expected: dict[str, JsonValue] = {}
    for path in sorted(BASELINE.glob("*.json")):
        for number, card in OBJECT.validate_json(path.read_bytes()).items():
            for value in array(object_value(card)["unclassified_positions"]):
                row = object_value(value)
                key = json.dumps(
                    [number, *[row[k] for k in ("face", "field", "section", "line")]]
                )
                expected[key] = row["frame_ids"]
    for path in sorted((ROOT / "authored/rules/effect-reviews").glob("*.yaml")):
        cards_path = ROOT / "authored/rules/effects" / path.name
        cards = object_value(read_yaml(cards_path)["cards"])
        for value in array(read_yaml(path)["reverse_positions"]):
            row = object_value(value)
            number = str(row["card_no"])
            key = json.dumps(
                [row[k] for k in ("card_no", "face", "field", "section", "line")]
            )
            assert key not in seen, key
            seen.add(key)
            assert row["frame_ids"] == expected[key], key
            assert row["category"] in categories, key
            abilities = array(object_value(cards[number])["abilities"])
            reverse_location = json.dumps(
                [row[k] for k in ("face", "field", "section", "line")]
            )
            assert reverse_location not in {
                location(object_value(a)) for a in abilities
            }, key
            if row["category"] == "跨行涵蓋":
                coverage = object_value(row["covered_by"])
                index = coverage["ability"]
                assert isinstance(index, int)
                node = abilities[index]
                for token in str(coverage["pointer"]).split("/")[1:]:
                    node = (
                        node[int(token)]
                        if isinstance(node, list)
                        else object_value(node)[token]
                    )
                assert JSON_VALUE.validate_python(node) is not None, key
    assert seen == expected.keys()

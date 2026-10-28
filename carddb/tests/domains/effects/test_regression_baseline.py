"""Freeze the reviewed inputs without treating annotation coverage as execution."""

import hashlib
import json
from collections import defaultdict
from pathlib import Path

import pytest
from pydantic import JsonValue, TypeAdapter

from sve_carddb.core.yaml import JSON_VALUE, parse_yaml
from sve_carddb.domains.effects.reader import Catalog, load
from sve_carddb.domains.translations.four_layer_normalizer import VERSION

ROOT = Path(__file__).resolve().parents[4]
BASELINE = ROOT / "carddb/tests/fixtures/effect_baseline"
AUTHOR_INDEX = ROOT / "authored/rules/effects-index"
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
    } | {p.stem.split("-", 1)[0] for p in (BASELINE / "migrated").glob("*.json")}


@pytest.fixture(scope="session")
def migrated_printings() -> frozenset[str]:
    """Include reprints whose permanent identity already has an author program."""
    card_ids = {
        str(row["card_id"])
        for index in sorted(AUTHOR_INDEX.glob("*.yaml"))
        for row in map(object_value, array(read_yaml(index)["records"]))
        if row["kind"] == "author_effect_set"
    }
    return frozenset(
        str(data["card_no"])
        for path in sorted((ROOT / "authored/registry/printing").glob("*/*.yaml"))
        for row in map(object_value, array(read_yaml(path)["records"]))
        if row["kind"] == "printing"
        and (data := object_value(row["data"]))["region"] == "jp"
        and data["card_id"] in card_ids
    )


@pytest.mark.parametrize("path", sorted(BASELINE.glob("*.json")), ids=lambda p: p.stem)
def test_handwritten_programs_keep_the_reviewed_positions_and_inputs(
    path: Path,
    migrated_printings: frozenset[str],
) -> None:
    expected = OBJECT.validate_json(path.read_bytes())
    prototype = ROOT / "authored/rules/effects" / f"{path.stem}.yaml"
    actual = object_value(read_yaml(prototype)["cards"]) if prototype.exists() else {}
    migrated = set(migrated_printings)
    migrated.update(
        file.stem for file in (BASELINE / "migrated").glob(f"{path.stem}-*.json")
    )
    assert set(actual) == set(expected) - migrated
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


def test_review_drafts_reference_the_current_frame_catalog() -> None:
    frames = {
        str(object_value(object_value(record)["data"])["id"])
        for path in (ROOT / "authored/translations/templates/definitions").glob(
            "*.yaml"
        )
        for record in array(read_yaml(path)["records"])
        if object_value(record)["kind"] == "sentence_template"
    }
    assert frames
    for path in (ROOT / "authored/rules/effect-reviews").glob("*.yaml"):
        draft = read_yaml(path)
        assert draft["kind"] == "effect_review_draft"
        assert object_value(draft["basis"])["normalizer"] == VERSION, path.stem
        references = {
            str(frame)
            for group in array(draft["groups"])
            for frame in array(object_value(group)["frames"])
        }
        references.update(
            str(object_value(row)["frame"])
            for row in array(draft.get("handwritten_differences", []))
        )
        references.update(
            str(frame)
            for row in array(draft["reverse_positions"])
            for frame in array(object_value(row)["frame_ids"])
        )
        assert references <= frames, path.stem


def test_bp07_p06_uses_the_corrected_main_text_without_duplicate_coverage() -> None:
    card = object_value(
        OBJECT.validate_json((BASELINE / "BP07.json").read_bytes())["BP07-P06"]
    )
    positions = [object_value(row) for row in array(card["positions"])]
    damage = next(row for row in positions if row["ability_indices"] == [2])
    assert [damage[k] for k in ("face", "field", "section", "line")] == [
        0,
        "effect",
        None,
        3,
    ]
    assert all(row["field"] == "effect" for row in positions)
    records = array(
        read_yaml(ROOT / "authored/registry/source-correction/active/BP07/001.yaml")[
            "records"
        ]
    )
    correction = next(
        object_value(object_value(row)["data"])
        for row in records
        if object_value(object_value(row)["data"])["id"]
        == "x:d529a2a90bd757f2be41c721eb0eaf4b"
    )
    assert object_value(correction["sections"])["corrected_value"] == []
    text = correction["corrected_value"]
    assert isinstance(text, str)
    lines = text.splitlines(keepends=True)
    assert len(lines) == 3
    start = sum(map(len, lines[:2]))
    for value in array(damage["fragments"]):
        fragment = object_value(value)
        assert fragment["source_sha256"] == hashlib.sha256(text.encode()).hexdigest()
        span = object_value(fragment["span"])
        assert span["role"] == "body"
        assert span["segments"] == [{"start": start, "end": len(text)}]


def test_reverse_positions_preserve_concrete_cross_line_coverage(
    migrated_printings: frozenset[str],
) -> None:
    admitted = migrated_printings
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
        cards = (
            object_value(read_yaml(cards_path)["cards"]) if cards_path.exists() else {}
        )
        for value in array(read_yaml(path)["reverse_positions"]):
            row = object_value(value)
            number = str(row["card_no"])
            key = json.dumps(
                [row[k] for k in ("card_no", "face", "field", "section", "line")]
            )
            assert key not in seen, key
            seen.add(key)
            assert row["frame_ids"] == expected[key], key
            assert row["category"] in {
                "通用規則",
                "說明",
                "token",
                "跨行涵蓋",
                "真正缺漏",
            }, key
            # Direct author admissions replace prototype coverage without requiring frozen ASTs.
            frozen = BASELINE / "migrated" / f"{number}.json"
            if not frozen.exists() and number not in cards:
                assert number in admitted, number
                continue
            card = (
                OBJECT.validate_json(frozen.read_bytes())
                if frozen.exists()
                else object_value(cards[number])
            )
            abilities = array(card["abilities"])
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

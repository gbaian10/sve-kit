"""Freeze the reviewed inputs without treating annotation coverage as execution."""

import hashlib
import json
from collections import defaultdict
from pathlib import Path

import pytest
from pydantic import JsonValue, TypeAdapter

from sve_carddb.core.json import canonical
from sve_carddb.core.yaml import JSON_VALUE, parse_yaml
from sve_carddb.domains.effects.author_expansion import AuthorExpander
from sve_carddb.domains.effects.author_set import load_author_set
from sve_carddb.domains.effects.reader import Catalog, load
from sve_carddb.domains.translations.four_layer_normalizer import VERSION

ROOT = Path(__file__).resolve().parents[4]
BASELINE = ROOT / "carddb/tests/fixtures/effect_baseline"
AUTHOR_INDEX = ROOT / "authored/rules/effects-index.yaml"
MIGRATED_ALIASES = {"BP18-SL13": "BP18-058"}
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


def author_records(pack: str) -> dict[str, dict[str, JsonValue]]:
    """Index rows from every author shard in ``pack``, keyed by CardNumber."""
    return {
        str(row["record_key"]): row
        for row in map(object_value, array(read_yaml(AUTHOR_INDEX)["records"]))
        if str(row["file"]).startswith(f"rules/author-effects/{pack}/")
    }


def assert_migrated_evolve(
    number: str, record: dict[str, JsonValue], baseline: dict[str, JsonValue]
) -> None:
    """The author entry keeps the prototype's single evolve ability at its position."""
    document = read_yaml(ROOT / "authored" / str(record["file"]))
    card = object_value(object_value(document["cards"])[number])
    faces = object_value(card["faces"])
    assert len(faces) == 1, number
    abilities = array(object_value(next(iter(faces.values())))["abilities"])
    assert len(abilities) == 1, number
    ability = object_value(abilities[0])
    assert ability["line"] == 1, number
    assert set(ability) == {"line", "sources", "ports", "evolve"}, number
    cost = object_value(object_value(ability["evolve"])["cost"])
    assert set(object_value(ability["evolve"])) == {"cost"}, number
    assert set(cost) == {"pp"}, number
    assert baseline["unclassified_positions"] == [], number
    rows = array(baseline["positions"])
    assert len(rows) == 1, number
    row = object_value(rows[0])
    assert (row["face"], row["field"], row["section"], row["line"]) == (
        0,
        "effect",
        None,
        1,
    ), number
    assert array(row["ability_indices"]) == [0], number


@pytest.mark.parametrize("path", sorted(BASELINE.glob("*.json")), ids=lambda p: p.stem)
def test_handwritten_programs_keep_the_reviewed_positions_and_inputs(
    path: Path,
) -> None:
    expected = OBJECT.validate_json(path.read_bytes())
    actual = object_value(
        read_yaml(ROOT / "authored/rules/effects" / f"{path.stem}.yaml")["cards"]
    )
    records = author_records(path.stem)
    migrated = set(records)
    migrated.update(
        file.stem for file in (BASELINE / "migrated").glob(f"{path.stem}-*.json")
    )
    migrated.update(
        alias
        for alias, target in MIGRATED_ALIASES.items()
        if target in migrated and alias in expected
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

    for number, record in records.items():
        if str(record["file"]).endswith("/001.yaml"):
            assert_migrated_evolve(number, record, object_value(expected[number]))


@pytest.mark.parametrize(
    "path", sorted((BASELINE / "migrated").glob("*.json")), ids=lambda p: p.stem
)
def test_migrated_prototype_asts_equal_the_frozen_baseline(path: Path) -> None:
    number = path.stem
    frozen = object_value(
        OBJECT.validate_json((BASELINE / f"{number.split('-')[0]}.json").read_bytes())[
            number
        ]
    )
    original = OBJECT.validate_json(path.read_bytes())
    assert fingerprint(original) == frozen["ast_sha256"]
    assert author_records(number.split("-")[0]).get(number) or number == "BP01-P25"


def test_migrated_lastword_preserves_the_frozen_ast_and_position() -> None:
    number = "BP01-P25"
    original = OBJECT.validate_json(
        (BASELINE / "migrated" / f"{number}.json").read_bytes()
    )
    frozen = object_value(
        OBJECT.validate_json((BASELINE / "BP01.json").read_bytes())[number]
    )
    assert fingerprint(original) == frozen["ast_sha256"]
    author = object_value(
        object_value(
            read_yaml(ROOT / "authored/rules/author-effects/BP01/000.yaml")["cards"]
        )[number]
    )
    ability = object_value(array(author["abilities"])[0])
    assert len(array(author["abilities"])) == 1
    expander = AuthorExpander(ROOT / "dsl")
    inputs = load_author_set(ROOT / "authored", expander.schemas)
    expanded = expander.expand(
        canonical(read_yaml(ROOT / "authored/rules/author-effects/BP01/000.yaml")),
        [file.raw for file in inputs.body_files],
    )
    body = object_value(
        array(
            object_value(object_value(object_value(expanded.tree)["cards"])[number])[
                "abilities"
            ]
        )[0]
    )
    assert body["lastword"] == {"do": [{"damage": {"to": {"leaders": "all"}, "n": 2}}]}
    use = object_value(array(ability["sources"])[0])
    occurrence = object_value(use["occurrence"])
    position = object_value(array(frozen["positions"])[0])
    fragment = object_value(array(position["fragments"])[0])
    assert ability["line"] == position["line"] == 1
    assert use["ability_scope"] == "ability_0"
    assert use["binding_id"] == fragment["id"]
    assert occurrence["field"] == position["field"] == "effect"
    assert occurrence["ordinal"] == position["section"] is None
    assert occurrence["line_ordinal"] == 0
    assert occurrence["source_hash"] == fragment["source_sha256"]
    assert occurrence["segments"] == object_value(fragment["span"])["segments"]
    assert position["ability_indices"] == [0]
    assert frozen["unclassified_positions"] == []


def assert_migrated_named_search_position(
    frozen: dict[str, JsonValue],
) -> None:
    rows = array(frozen["positions"])
    assert len(rows) == 1
    position = object_value(rows[0])
    assert (
        position["face"],
        position["field"],
        position["section"],
        position["line"],
    ) == (
        0,
        "effect",
        None,
        1,
    )
    assert position["ability_indices"] == [0]
    fragments = array(position["fragments"])
    assert len(fragments) == 1
    fragment = object_value(fragments[0])
    author = object_value(
        object_value(
            read_yaml(ROOT / "authored/rules/author-effects/BP06/000.yaml")["cards"]
        )["BP06-048"]
    )
    face = object_value(
        object_value(author["faces"])["f:e9d8deca215953a99d12bd07acd566fa"]
    )
    ability = object_value(array(face["abilities"])[0])
    assert ability["line"] == position["line"] == 1
    assert len(array(face["abilities"])) == 1
    use = object_value(array(ability["sources"])[0])
    occurrence = object_value(use["occurrence"])
    assert use["ability_scope"] == "ability_0"
    assert use["binding_id"] == fragment["id"]
    assert occurrence["field"] == position["field"] == "effect"
    assert occurrence["ordinal"] == position["section"] is None
    assert occurrence["line_ordinal"] == 0
    assert occurrence["source_hash"] == fragment["source_sha256"]
    assert occurrence["segments"] == object_value(fragment["span"])["segments"]
    assert position["ability_indices"] == [0]
    assert frozen["unclassified_positions"] == []


def assert_migrated_named_search_body() -> None:
    expander = AuthorExpander(ROOT / "dsl")
    inputs = load_author_set(ROOT / "authored", expander.schemas)
    author_file = next(
        file
        for file in inputs.author_files
        if file.path == "rules/author-effects/BP06/000.yaml"
    )
    expanded = expander.expand(
        author_file.raw, [file.raw for file in inputs.body_files]
    )
    expanded_card = object_value(
        object_value(object_value(expanded.tree)["cards"])["BP06-048"]
    )
    expanded_face = object_value(
        object_value(expanded_card["faces"])["f:e9d8deca215953a99d12bd07acd566fa"]
    )
    expanded_ability = object_value(array(expanded_face["abilities"])[0])
    assert expanded_ability["lastword"] == {
        "do": [
            {
                "search": {
                    "sel": {"who": "self", "zone": "deck", "name": "式神の使役"},
                    "n": 1,
                    "to": "hand",
                }
            }
        ]
    }


def test_migrated_named_search_preserves_the_frozen_ast_and_position() -> None:
    number = "BP06-048"
    original = OBJECT.validate_json(
        (BASELINE / "migrated" / f"{number}.json").read_bytes()
    )
    frozen = object_value(
        OBJECT.validate_json((BASELINE / "BP06.json").read_bytes())[number]
    )
    assert fingerprint(original) == frozen["ast_sha256"]
    assert_migrated_named_search_position(frozen)
    assert_migrated_named_search_body()


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

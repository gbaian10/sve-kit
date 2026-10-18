"""The complete reader preserves all eighteen original/translation language choices."""

from copy import deepcopy

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.snapshot import columns
from sve_carddb.core.json import array, canonical, object_value, string
from sve_carddb.export.project import display_text
from sve_carddb.export.reader import View, read_snapshot
from sve_carddb.export.text_owners import TextOwners

from ..support.snapshot_contract_fixtures import _reseal, fixture

MATRIX = object_value(fixture("language-matrix.json"))
CASES = tuple(object_value(c) for c in array(MATRIX["cases"]))
INVALID = tuple(object_value(c) for c in array(MATRIX["invalid_cases"]))


def test_fixture_covers_every_language_and_quality_cell() -> None:
    pairs = {
        ("jp", "zh-Hant"),
        ("en", "zh-Hant"),
        ("jp", "ja"),
        ("en", "en"),
        ("jp", "en"),
        ("en", "ja"),
    }
    assert len(CASES) == 18
    assert {(c["region"], c["text_language"], c["quality"]) for c in CASES} == {
        (*pair, quality)
        for pair in pairs
        for quality in ("available", "missing", "low_confidence")
    }
    assert len(set(object_value(MATRIX["texts"]).values())) == 5


@pytest.fixture(scope="module")
def language_view() -> View:
    snapshot = object_value(MATRIX["snapshot"])
    return read_snapshot(
        object_value(snapshot["manifest"]),
        {k: canonical(v) for k, v in object_value(snapshot["payloads"]).items()},
    )


@pytest.mark.parametrize("case", CASES, ids=lambda c: string(c["id"]))
def test_language_matrix_exact_text_and_quality(
    language_view: View, case: dict[str, JsonValue]
) -> None:
    pointer = object_value(case["receiver"])
    owners = TextOwners(language_view)
    receiver = owners.get(object_value(pointer["owner"]))
    lang = string(case["text_language"])
    display = display_text(language_view, receiver.row, "effect", lang)
    texts = {r["id"]: r for r in language_view["text_unit"]}
    original = texts[owners.pointer(pointer)]
    selection = next(
        (
            object_value(value)
            for value in array(receiver.row["translations"])
            if object_value(value)["target_lang"] == lang
        ),
        None,
    )
    translation = next(
        (r for r in language_view["translation"] if r["id"] == display.translation_id),
        None,
    )
    expected = object_value(case["expected"])
    assert original["text"] == expected["original"]
    assert display.text == (
        expected["original"]
        if expected["translated"] is None
        else expected["translated"]
    )
    assert display.missing_translation is expected["missing"]
    assert (None if selection is None else selection["basis"]) == expected["basis"]
    assert (translation is not None and translation["low_confidence"]) is expected[
        "low_confidence"
    ]
    assert (None if translation is None else translation["origin"]) == expected[
        "origin"
    ]
    assert (None if translation is None else translation["authority"]) == expected[
        "authority"
    ]
    if translation is None:
        assert selection is None
        assert display.lang == original["lang"]
    else:
        assert display.lang == lang
        assert texts[translation["text_unit_id"]]["text"] == expected["translated"]


@pytest.mark.parametrize("case", INVALID, ids=lambda c: string(c["id"]))
def test_low_confidence_does_not_grant_official_counterpart_authority(
    case: dict[str, JsonValue],
) -> None:
    snapshot = deepcopy(object_value(MATRIX["snapshot"]))
    manifest = object_value(snapshot["manifest"])
    payloads = object_value(snapshot["payloads"])
    changed = 0
    for payload in payloads.values():
        tables = object_value(object_value(payload).get("tables", {}))
        for fragment in array(tables.get("translation", [])):
            for value in array(array(fragment)[4]):
                row = array(value)
                if row[columns("translation").index("id")] == case["translation_id"]:
                    assert row[columns("translation").index("low_confidence")] is True
                    row[columns("translation").index(string(case["field"]))] = case[
                        "value"
                    ]
                    changed += 1
    assert changed == 1
    with pytest.raises(
        ValueError, match="public-annotation/" + string(case["python_reason"])
    ):
        read_snapshot(manifest, _reseal(manifest, payloads))

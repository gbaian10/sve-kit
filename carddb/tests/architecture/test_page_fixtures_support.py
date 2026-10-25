from __future__ import annotations

import json
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

if TYPE_CHECKING:
    from pathlib import Path

from ..support.page_fixtures_support import (
    Case,
    Check,
    FixtureInputs,
    Index,
    PageFixtureError,
    check_projection,
    load_index,
    project,
    read_inputs,
)

TEST_HASH = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
EMPTY_HASH = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
CASE = Case(
    "F01",
    "fixtures/official_jp/synthetic/basic.html",
    TEST_HASH,
    "jp-card",
    "SYN01-001",
    None,
    None,
    None,
    (Check(("name",), "text", EMPTY_HASH),),
)


def test_default_index_contains_the_public_sve_fixtures() -> None:
    index = load_index()
    assert index.commit == "309b766885d49faff9db80f5b97cd56d8601c55b"
    assert len(index.cases) == 16
    assert all(
        case.path.startswith(("fixtures/official_jp/", "fixtures/official_en/"))
        for case in index.cases
    )
    assert len({case.sha256 for case in index.cases}) == 16


def test_fixture_representation_hides_source_bytes() -> None:
    inputs = FixtureInputs({"F01": b"SVE-KIT synthetic private sentinel"})
    assert repr(inputs) == "<fixture inputs>"
    assert inputs["F01"] == b"SVE-KIT synthetic private sentinel"


def index_document() -> dict[str, object]:
    return {
        "version": 1,
        "commit": "f" * 40,
        "cases": [
            {
                "id": "F01",
                "path": CASE.path,
                "sha256": TEST_HASH,
                "parser": CASE.parser,
                "arguments": {"number": "SYN01-001"},
                "checks": [{"path": ["name"], "kind": "text", "expected": EMPTY_HASH}],
            }
        ],
    }


def write_index(root: Path, document: object) -> Path:
    path = root / "index.json"
    path.write_text(json.dumps(document))
    return path


def write_fixture(root: Path, body: bytes = b"test") -> Path:
    path = root / CASE.path
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(body)
    return path


def test_local_hash_verification_does_not_require_git(tmp_path: Path) -> None:
    index = load_index(write_index(tmp_path, index_document()))
    write_fixture(tmp_path)
    assert read_inputs(index, tmp_path) == {"F01": b"test"}
    # A newer local checkout with unchanged bytes must remain usable.
    (tmp_path / ".git").mkdir()
    (tmp_path / ".git/HEAD").write_text("0" * 40)
    assert read_inputs(index, tmp_path) == {"F01": b"test"}


def test_hash_mismatch_is_safe(tmp_path: Path) -> None:
    write_fixture(tmp_path, b"SVE-KIT synthetic private sentinel")
    with pytest.raises(PageFixtureError) as failure:
        read_inputs(Index("f" * 40, (CASE,)), tmp_path)
    assert str(failure.value) == "F01: fixture hash mismatch"


def test_missing_fixture_fails(tmp_path: Path) -> None:
    with pytest.raises(PageFixtureError, match=r"^F01: fixture unavailable$"):
        read_inputs(Index("f" * 40, (CASE,)), tmp_path)


def test_missing_directory_fails(tmp_path: Path) -> None:
    with pytest.raises(
        PageFixtureError, match=r"^page-fixture data directory is unavailable$"
    ):
        read_inputs(Index("f" * 40, (CASE,)), tmp_path / "missing")


def test_symlink_escape_fails(tmp_path: Path) -> None:
    root = tmp_path / "private"
    path = root / CASE.path
    path.parent.mkdir(parents=True)
    outside = tmp_path / "outside.html"
    outside.write_bytes(b"test")
    path.symlink_to(outside)
    with pytest.raises(PageFixtureError, match=r"^F01: unsafe fixture$"):
        read_inputs(Index("f" * 40, (CASE,)), root)


@pytest.mark.parametrize(
    "path",
    [
        "/absolute.html",
        "fixtures/official_jp/../outside.html",
        "fixtures/official_jp//page.html",
        "fixtures/official_jp/page.txt",
        "fixtures/carddb/page.html",
        "other/page.html",
    ],
)
def test_unsafe_index_paths_are_rejected(tmp_path: Path, path: str) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0]["path"] = path
    with pytest.raises(PageFixtureError, match=r"^unsafe page-fixture fixture path$"):
        load_index(write_index(tmp_path, document))


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("sha256", "x" * 64, "invalid page-fixture fixture hash"),
        ("parser", "eval", "unknown page-fixture parser"),
        ("id", "name from source", "invalid page-fixture case ID"),
        ("arguments", {"extra": 1}, "invalid page-fixture index object"),
        ("checks", [], "page-fixture case has no checks"),
    ],
)
def test_invalid_case_contract_fails(
    tmp_path: Path, field: str, value: object, message: str
) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0][field] = value
    with pytest.raises(PageFixtureError) as failure:
        load_index(write_index(tmp_path, document))
    assert str(failure.value) == message


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("commit", "f" * 39, "invalid page-fixture commit pin"),
        ("version", True, "unsupported page-fixture index version"),
        ("version", 2, "unsupported page-fixture index version"),
        ("cases", [], "page-fixture index has no cases"),
    ],
)
def test_invalid_index_contract_fails(
    tmp_path: Path, field: str, value: object, message: str
) -> None:
    document = index_document()
    document[field] = value
    with pytest.raises(PageFixtureError) as failure:
        load_index(write_index(tmp_path, document))
    assert str(failure.value) == message


def test_duplicate_json_keys_fail(tmp_path: Path) -> None:
    path = tmp_path / "index.json"
    path.write_text('{"version":1,"version":1}')
    with pytest.raises(PageFixtureError, match=r"^duplicate page-fixture index key$"):
        load_index(path)


def test_duplicate_case_fails(tmp_path: Path) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases.append(cases[0])
    with pytest.raises(
        PageFixtureError, match=r"^duplicate page-fixture case or fixture$"
    ):
        load_index(write_index(tmp_path, document))


def test_duplicate_check_fails(tmp_path: Path) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0]["checks"] *= 2
    with pytest.raises(PageFixtureError, match=r"^duplicate page-fixture check path$"):
        load_index(write_index(tmp_path, document))


def test_subset_allows_new_fields_and_distinguishes_null_from_empty() -> None:
    checks = (
        Check(("record",), "mapping", None),
        Check(("record", "name"), "text", EMPTY_HASH),
        Check(("record", "note"), "null", None),
    )
    case = replace(CASE, checks=checks)
    check_projection(
        case,
        {
            "record": {
                "name": "",
                "note": None,
                "new_field": "SVE-KIT synthetic extension",
            }
        },
    )
    with pytest.raises(PageFixtureError, match=r"^F01: record\.name: text mismatch$"):
        check_projection(case, {"record": {"name": None, "note": None}})
    with pytest.raises(PageFixtureError, match=r"^F01: record\.note: null mismatch$"):
        check_projection(case, {"record": {"name": "", "note": ""}})


def test_exact_utf8_and_safe_field_failure() -> None:
    check_projection(CASE, {"name": ""})
    for actual in (" ", "\n", "SVE-KIT synthetic private sentinel"):
        with pytest.raises(PageFixtureError) as failure:
            check_projection(CASE, {"name": actual})
        assert str(failure.value) == "F01: name: text mismatch"


def test_ordered_list_and_missing_field() -> None:
    case = replace(
        CASE,
        checks=(
            Check(("items",), "length", 2),
            Check(("items", 0), "text", EMPTY_HASH),
        ),
    )
    check_projection(case, {"items": ["", "SVE-KIT synthetic second"]})
    with pytest.raises(PageFixtureError, match=r"^F01: items\.0: text mismatch$"):
        check_projection(case, {"items": ["SVE-KIT synthetic second", ""]})
    with pytest.raises(PageFixtureError, match=r"^F01: items: length mismatch$"):
        check_projection(case, {"items": [""]})
    with pytest.raises(PageFixtureError, match=r"^F01: items: missing field$"):
        check_projection(case, {})
    with pytest.raises(PageFixtureError, match=r"^F01: items\.0: missing field$"):
        check_projection(replace(case, checks=(case.checks[1],)), {"items": []})


def test_boolean_is_not_integer() -> None:
    integer = replace(CASE, checks=(Check(("count",), "integer", 1),))
    boolean = replace(CASE, checks=(Check(("flag",), "boolean", True),))
    check_projection(integer, {"count": 1})
    check_projection(boolean, {"flag": True})
    with pytest.raises(PageFixtureError, match=r"^F01: count: integer mismatch$"):
        check_projection(integer, {"count": True})
    with pytest.raises(PageFixtureError, match=r"^F01: flag: boolean mismatch$"):
        check_projection(boolean, {"flag": 1})


def test_parser_error_does_not_quote_source_input() -> None:
    case = replace(CASE, parser="jp-card", number="SYN01-001")
    with pytest.raises(PageFixtureError) as failure:
        project(case, b"<html>")
    assert str(failure.value) == "F01: parser rejected fixture"


@pytest.mark.parametrize(
    ("check", "message"),
    [
        ({"path": [], "kind": "null"}, "invalid page-fixture check path"),
        ({"path": [True], "kind": "null"}, "invalid page-fixture index string"),
        ({"path": [-1], "kind": "null"}, "invalid page-fixture index integer"),
        (
            {"path": ["name"], "kind": "text", "expected": "x" * 64},
            "invalid page-fixture text hash",
        ),
        (
            {"path": ["count"], "kind": "integer", "expected": True},
            "invalid page-fixture index integer",
        ),
        (
            {"path": ["flag"], "kind": "boolean", "expected": 1},
            "invalid page-fixture boolean",
        ),
        ({"path": ["name"], "kind": "unknown"}, "unknown page-fixture check kind"),
    ],
)
def test_invalid_check_contract_fails(
    tmp_path: Path, check: dict[str, object], message: str
) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0]["checks"] = [check]
    with pytest.raises(PageFixtureError) as failure:
        load_index(write_index(tmp_path, document))
    assert str(failure.value) == message


@pytest.mark.parametrize(
    "path",
    [
        "fixtures/official_jp/synthetic/basic.html",
        "fixtures/official_jp/card.html",
        "fixtures/official_en/card.html",
    ],
)
def test_index_roots_do_not_cross_storage_boundaries(tmp_path: Path, path: str) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0]["path"] = path
    index = write_index(tmp_path, document)
    assert load_index(index).cases[0].path == path

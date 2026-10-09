import hashlib
import json
import os
import subprocess  # ruff: ignore[suspicious-subprocess-import] -- exercise collection in a fresh synthetic pytest project
import sys
from dataclasses import replace
from pathlib import Path

import pytest

from ..support.private_pages_support import (
    DIRECTORY_ENV,
    MODE_ENV,
    Case,
    Check,
    Index,
    PrivateInputs,
    PrivatePageError,
    check_projection,
    load_index,
    mode,
    project,
    read_inputs,
)

TEST_HASH = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08"
EMPTY_HASH = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
CASE = Case(
    "F01",
    "fixtures/carddb/synthetic/basic.html",
    TEST_HASH,
    "sv1-card",
    None,
    None,
    None,
    None,
    (Check(("name",), "text", EMPTY_HASH),),
)


def test_public_index_needs_no_private_checkout() -> None:
    index = load_index()
    assert index.commit == "309b766885d49faff9db80f5b97cd56d8601c55b"
    assert len(index.cases) == 19
    assert len({case.sha256 for case in index.cases}) == 19


def test_private_fixture_representation_is_safe() -> None:
    inputs = PrivateInputs({"F01": b"SVE-KIT synthetic private sentinel"})
    assert repr(inputs) == "<private inputs>"
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
                "parser": "sv1-card",
                "arguments": {},
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


@pytest.mark.parametrize("value", [None, "excluded", "required"])
def test_mode_is_explicit_with_excluded_default(value: str | None) -> None:
    environment = {} if value is None else {MODE_ENV: value}
    assert mode(environment) == (value or "excluded")


@pytest.mark.parametrize("value", ["", "auto", "Required"])
def test_unknown_mode_fails(value: str) -> None:
    with pytest.raises(
        PrivatePageError, match=r"^private-page mode must be required or excluded$"
    ):
        mode({MODE_ENV: value})


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
    with pytest.raises(PrivatePageError) as failure:
        read_inputs(Index("f" * 40, (CASE,)), tmp_path)
    assert str(failure.value) == "F01: fixture hash mismatch"


def test_missing_fixture_fails(tmp_path: Path) -> None:
    with pytest.raises(PrivatePageError, match=r"^F01: fixture unavailable$"):
        read_inputs(Index("f" * 40, (CASE,)), tmp_path)


def test_missing_directory_fails(tmp_path: Path) -> None:
    with pytest.raises(
        PrivatePageError, match=r"^private-page data directory is unavailable$"
    ):
        read_inputs(Index("f" * 40, (CASE,)), tmp_path / "missing")


def test_symlink_escape_fails(tmp_path: Path) -> None:
    root = tmp_path / "private"
    path = root / CASE.path
    path.parent.mkdir(parents=True)
    outside = tmp_path / "outside.html"
    outside.write_bytes(b"test")
    path.symlink_to(outside)
    with pytest.raises(PrivatePageError, match=r"^F01: unsafe fixture$"):
        read_inputs(Index("f" * 40, (CASE,)), root)


@pytest.mark.parametrize(
    "path",
    [
        "/absolute.html",
        "fixtures/carddb/../outside.html",
        "fixtures/carddb//page.html",
        "fixtures/carddb/page.txt",
        "other/page.html",
    ],
)
def test_unsafe_index_paths_are_rejected(tmp_path: Path, path: str) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0]["path"] = path
    with pytest.raises(PrivatePageError, match=r"^unsafe private-page fixture path$"):
        load_index(write_index(tmp_path, document))


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("sha256", "x" * 64, "invalid private-page fixture hash"),
        ("parser", "eval", "unknown private-page parser"),
        ("id", "name from source", "invalid private-page case ID"),
        ("arguments", {"extra": 1}, "invalid private-page index object"),
        ("checks", [], "private-page case has no checks"),
    ],
)
def test_invalid_case_contract_fails(
    tmp_path: Path, field: str, value: object, message: str
) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0][field] = value
    with pytest.raises(PrivatePageError) as failure:
        load_index(write_index(tmp_path, document))
    assert str(failure.value) == message


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("commit", "f" * 39, "invalid private-page commit pin"),
        ("version", True, "unsupported private-page index version"),
        ("version", 2, "unsupported private-page index version"),
        ("cases", [], "private-page index has no cases"),
    ],
)
def test_invalid_index_contract_fails(
    tmp_path: Path, field: str, value: object, message: str
) -> None:
    document = index_document()
    document[field] = value
    with pytest.raises(PrivatePageError) as failure:
        load_index(write_index(tmp_path, document))
    assert str(failure.value) == message


def test_duplicate_json_keys_fail(tmp_path: Path) -> None:
    path = tmp_path / "index.json"
    path.write_text('{"version":1,"version":1}')
    with pytest.raises(PrivatePageError, match=r"^duplicate private-page index key$"):
        load_index(path)


def test_duplicate_case_fails(tmp_path: Path) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases.append(cases[0])
    with pytest.raises(
        PrivatePageError, match=r"^duplicate private-page case or fixture$"
    ):
        load_index(write_index(tmp_path, document))


def test_duplicate_check_fails(tmp_path: Path) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0]["checks"] *= 2
    with pytest.raises(PrivatePageError, match=r"^duplicate private-page check path$"):
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
    with pytest.raises(PrivatePageError, match=r"^F01: record\.name: text mismatch$"):
        check_projection(case, {"record": {"name": None, "note": None}})
    with pytest.raises(PrivatePageError, match=r"^F01: record\.note: null mismatch$"):
        check_projection(case, {"record": {"name": "", "note": ""}})


def test_exact_utf8_and_safe_field_failure() -> None:
    check_projection(CASE, {"name": ""})
    for actual in (" ", "\n", "SVE-KIT synthetic private sentinel"):
        with pytest.raises(PrivatePageError) as failure:
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
    with pytest.raises(PrivatePageError, match=r"^F01: items\.0: text mismatch$"):
        check_projection(case, {"items": ["SVE-KIT synthetic second", ""]})
    with pytest.raises(PrivatePageError, match=r"^F01: items: length mismatch$"):
        check_projection(case, {"items": [""]})
    with pytest.raises(PrivatePageError, match=r"^F01: items: missing field$"):
        check_projection(case, {})
    with pytest.raises(PrivatePageError, match=r"^F01: items\.0: missing field$"):
        check_projection(replace(case, checks=(case.checks[1],)), {"items": []})


def test_boolean_is_not_integer() -> None:
    integer = replace(CASE, checks=(Check(("count",), "integer", 1),))
    boolean = replace(CASE, checks=(Check(("flag",), "boolean", True),))
    check_projection(integer, {"count": 1})
    check_projection(boolean, {"flag": True})
    with pytest.raises(PrivatePageError, match=r"^F01: count: integer mismatch$"):
        check_projection(integer, {"count": True})
    with pytest.raises(PrivatePageError, match=r"^F01: flag: boolean mismatch$"):
        check_projection(boolean, {"flag": 1})


def test_parser_error_does_not_quote_private_input() -> None:
    with pytest.raises(PrivatePageError) as failure:
        project(CASE, b"SVE-KIT synthetic private sentinel")
    assert str(failure.value) == "F01: parser rejected fixture"


@pytest.fixture(scope="module")
def private_failure_project(tmp_path_factory: pytest.TempPathFactory) -> Path:
    root = tmp_path_factory.mktemp("private-failure-project")
    carddb = Path(__file__).resolve().parents[2]
    body = (
        "<!DOCTYPE html><!--SVE-KIT PRIVATE HEAD--><html><body>"
        '<div class="cardlist-Detail"><div class="cardlist-Detail_Box_Inner">'
        '<p class="ttl">SVE-KIT synthetic card</p><div class="info">'
        "<dl><dt>クラス</dt><dd>ニュートラル</dd></dl>"
        "<dl><dt>カード種類</dt><dd>フォロワー</dd></dl>"
        "<dl><dt>タイプ</dt><dd>-</dd></dl>"
        "<dl><dt>レアリティ</dt><dd>BR</dd></dl></div>"
        '<div class="status-Item status-Item-Cost"><span class="heading">コスト</span>1</div>'
        '<div class="status-Item status-Item-Power"><span class="heading">攻撃力</span>1</div>'
        '<div class="status-Item status-Item-Hp"><span class="heading">体力</span>1</div>'
        '<div class="img"><img src="/wp-content/images/cardlist/SYN01-001.png"></div>'
        '<div class="illustrator"><span class="name">SYN01-001</span>'
        '<span class="heading">SVE-KIT synthetic illustrator</span></div>'
        '<div class="detail"><p>SVE-KIT PRIVATE WORDING</p></div>'
        "</div></div><div>SVE-KIT synthetic layout padding "
        + "layout " * 80
        + "</div><!--SVE-KIT PRIVATE TAIL--></body></html>"
    ).encode()
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0].update(
        sha256=hashlib.sha256(body).hexdigest(),
        parser="jp-card",
        arguments={"number": "SYN01-001"},
        checks=[
            {
                "path": ["record", "faces", 0, "text"],
                "kind": "text",
                "expected": EMPTY_HASH,
            }
        ],
    )
    write_index(root, document)
    write_fixture(root, body)
    (root / "pytest.ini").write_text("[pytest]\n")
    (root / "conftest.py").write_text(
        f"import sys\nsys.path.insert(0, {str(carddb)!r})\npytest_plugins = ('tests.support.private_pages_plugin',)\n"
    )
    (root / "test_mismatch.py").write_text(
        "import os\nfrom dataclasses import replace\nfrom pathlib import Path\n"
        "from tests.support.private_pages_support import load_index\n"
        "from tests.parse import test_private_official_pages as private\n"
        "private.INDEX = load_index(Path('index.json'))\n"
        "if os.environ['SVE_KIT_TEST_FAILURE'] == 'parser':\n"
        "    private.INDEX = replace(private.INDEX, cases=(replace(private.INDEX.cases[0], number='SYN01-999'),))\n"
        "private_inputs = private.private_inputs\n"
        "def test_mismatch(private_inputs):\n"
        "    private.test_private_page(private.INDEX.cases[0], private_inputs)\n"
    )
    return root


@pytest.mark.parametrize(
    ("failure", "message"),
    [
        ("field", "F01: record.faces.0.text: text mismatch"),
        ("parser", "F01: parser rejected fixture"),
    ],
)
@pytest.mark.parametrize("plugins", ["core", "default"])
def test_pytest_failure_traceback_hides_private_page(
    private_failure_project: Path, failure: str, message: str, plugins: str
) -> None:
    environment = dict(os.environ) | {
        MODE_ENV: "required",
        DIRECTORY_ENV: str(private_failure_project),
        "SVE_KIT_TEST_FAILURE": failure,
    }
    environment.pop("PYTEST_DISABLE_PLUGIN_AUTOLOAD", None)
    if plugins == "core":
        environment["PYTEST_DISABLE_PLUGIN_AUTOLOAD"] = "1"
    result = subprocess.run(
        [sys.executable, "-m", "pytest", "-q", "-p", "no:cacheprovider"],
        cwd=private_failure_project,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    output = result.stdout + result.stderr
    assert result.returncode == 1
    assert "1 failed" in output
    assert message in output
    assert "<private inputs>" in output
    assert "b'<" not in output
    assert "SVE-KIT PRIVATE HEAD" not in output
    assert "SVE-KIT PRIVATE TAIL" not in output
    assert "SVE-KIT PRIVATE WORDING" not in output
    assert repr((private_failure_project / CASE.path).read_bytes()) not in output


@pytest.fixture(scope="module")
def pytest_project(tmp_path_factory: pytest.TempPathFactory) -> Path:
    root = tmp_path_factory.mktemp("private-mode-project")
    carddb = Path(__file__).resolve().parents[2]
    (root / "pytest.ini").write_text("[pytest]\n")
    (root / "conftest.py").write_text(
        f"import sys\nsys.path.insert(0, {str(carddb)!r})\npytest_plugins = ('tests.support.private_pages_plugin',)\n"
    )
    (root / "test_public.py").write_text("def test_public():\n    pass\n")
    (root / "test_private_official_pages.py").write_text(
        "import os\nfrom pathlib import Path\nimport pytest\nfrom tests.support.private_pages_support import load_index, read_inputs, DIRECTORY_ENV\npytestmark = pytest.mark.private_pages\ndef test_private():\n    assert read_inputs(load_index(Path('index.json')), Path(os.environ[DIRECTORY_ENV]))['F01'] == b'test'\n"
    )
    write_index(root, index_document())
    write_fixture(root)
    return root


@pytest.mark.parametrize(
    ("value", "passed"), [(None, 1), ("excluded", 1), ("required", 2)]
)
def test_real_collection_modes(
    pytest_project: Path, value: str | None, passed: int
) -> None:
    environment = dict(os.environ)
    environment.pop(MODE_ENV, None)
    if value is not None:
        environment[MODE_ENV] = value
    environment[DIRECTORY_ENV] = str(pytest_project)
    result = subprocess.run(
        [sys.executable, "-m", "pytest", "-q", "-p", "no:xdist"],
        cwd=pytest_project,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0
    assert f"{passed} passed" in result.stdout
    assert result.stdout.count("私有真實頁測試未執行") == (
        0 if value == "required" else 1
    )


@pytest.mark.parametrize("value", ["", "auto"])
def test_real_collection_rejects_unknown_mode(pytest_project: Path, value: str) -> None:
    environment = dict(os.environ) | {MODE_ENV: value}
    result = subprocess.run(
        [sys.executable, "-m", "pytest", "-q", "-p", "no:xdist"],
        cwd=pytest_project,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 4
    assert "private-page mode must be required or excluded" in result.stderr


@pytest.mark.parametrize(
    ("check", "message"),
    [
        ({"path": [], "kind": "null"}, "invalid private-page check path"),
        ({"path": [True], "kind": "null"}, "invalid private-page index string"),
        ({"path": [-1], "kind": "null"}, "invalid private-page index integer"),
        (
            {"path": ["name"], "kind": "text", "expected": "x" * 64},
            "invalid private-page text hash",
        ),
        (
            {"path": ["count"], "kind": "integer", "expected": True},
            "invalid private-page index integer",
        ),
        (
            {"path": ["flag"], "kind": "boolean", "expected": 1},
            "invalid private-page boolean",
        ),
        ({"path": ["name"], "kind": "unknown"}, "unknown private-page check kind"),
    ],
)
def test_invalid_check_contract_fails(
    tmp_path: Path, check: dict[str, object], message: str
) -> None:
    document = index_document()
    cases = document["cases"]
    assert isinstance(cases, list)
    cases[0]["checks"] = [check]
    with pytest.raises(PrivatePageError) as failure:
        load_index(write_index(tmp_path, document))
    assert str(failure.value) == message


def test_explicit_private_selection_is_deselected_in_excluded_mode(
    pytest_project: Path,
) -> None:
    environment = dict(os.environ) | {MODE_ENV: "excluded"}
    environment.pop(DIRECTORY_ENV, None)
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "pytest",
            "-q",
            "-p",
            "no:xdist",
            "test_private_official_pages.py::test_private",
        ],
        cwd=pytest_project,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 5
    assert "1 deselected" in result.stdout
    assert "私有真實頁測試未執行" in result.stdout
    assert "KeyError" not in result.stdout


def test_required_collection_does_not_skip_unavailable_data(
    pytest_project: Path,
) -> None:
    environment = dict(os.environ) | {
        MODE_ENV: "required",
        DIRECTORY_ENV: str(pytest_project / "unavailable"),
    }
    result = subprocess.run(
        [sys.executable, "-m", "pytest", "-q", "-p", "no:xdist"],
        cwd=pytest_project,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 1
    assert "1 failed" in result.stdout
    assert "private-page data directory is unavailable" in result.stdout
    assert "skipped" not in result.stdout
    assert "私有真實頁測試未執行" not in result.stdout

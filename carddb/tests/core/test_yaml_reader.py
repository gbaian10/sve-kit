"""Independent strict-rule examples, writer round trips and production validation."""

import traceback
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
import yamlrocks
from pydantic import JsonValue, RootModel, ValidationError

from sve_carddb.core.json import canonical, digest
from sve_carddb.core.yaml import MAX_BYTES, parse_yaml
from sve_carddb.domains.registry import storage
from sve_carddb.domains.registry.storage import encode, read_yaml

if TYPE_CHECKING:
    from pytest_mock import MockerFixture

AUTHORED = Path(__file__).resolve().parents[3] / "authored"


def test_all_production_files_pass_strict_reader() -> None:
    # One loop instead of one item per file: thousands of items cost more collection time than the reads.
    for path in sorted(AUTHORED.rglob("*.yaml")):
        name = path.relative_to(AUTHORED).as_posix()
        before = path.read_bytes()
        try:
            read_yaml(path)
        except ValueError as error:
            pytest.fail(f"{name}: {error}")
        assert path.read_bytes() == before, name


@pytest.mark.parametrize(
    ("source", "error", "message"),
    [
        pytest.param("a: *undefined", ValueError, "syntax", id="undefined-alias"),
        pytest.param("a: &x [*x]", ValueError, "syntax", id="cyclic-sequence"),
        pytest.param("a: &x {b: *x}", ValueError, "syntax", id="cyclic-map"),
    ],
)
def test_each_syntax_rule_independently(
    tmp_path: Path, source: str, error: type[Exception], message: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(source, encoding="utf-8")
    with pytest.raises(error, match=message):
        read_yaml(path)


@pytest.mark.parametrize("source", [".inf", "-.Inf", "+.INF"])
def test_floating_point_scalars_cannot_reach_canonical_hash(
    tmp_path: Path, source: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: " + source)
    with pytest.raises(ValueError, match="Floating point JSON is forbidden"):
        read_yaml(path)


@pytest.mark.parametrize("length", [MAX_BYTES - 1, MAX_BYTES, MAX_BYTES + 1])
def test_exact_file_size_limit(tmp_path: Path, length: int) -> None:
    path = tmp_path / "input.yaml"
    path.write_bytes(b" " * length)
    if length < MAX_BYTES:
        assert read_yaml(path) is None
    else:
        with pytest.raises(ValueError, match="Oversized"):
            read_yaml(path)


@pytest.mark.parametrize(
    "data", [b"a: \xff", "a: value".encode("utf-16")], ids=["invalid-utf8", "utf16"]
)
def test_only_utf8_input(tmp_path: Path, data: bytes) -> None:
    path = tmp_path / "input.yaml"
    path.write_bytes(data)
    with pytest.raises(UnicodeDecodeError):
        read_yaml(path)


@pytest.mark.parametrize(
    ("source", "expected"),
    [("yes", "yes"), ("YES", "YES"), ("no", "no")],
)
def test_yaml12_scalar_values_and_types(
    tmp_path: Path, source: str, expected: JsonValue
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: " + source)
    decoded = parse_yaml(path.read_bytes())
    assert isinstance(decoded, dict)
    assert decoded["a"] == expected
    assert type(decoded["a"]) is type(expected)
    if isinstance(expected, float):
        with pytest.raises(ValueError, match="Floating point JSON is forbidden"):
            read_yaml(path)
    else:
        value = read_yaml(path)
        assert value == {"a": expected}
        assert canonical(value) == canonical({"a": expected})


@pytest.mark.parametrize("prefix", ["", "%YAML 1.2\n---\n", "\ufeff---\n"])
def test_yaml12_directive_and_utf8_bom(tmp_path: Path, prefix: str) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(prefix + "a: true\nb: 0777")
    assert read_yaml(path) == {"a": True, "b": "0777"}


ROUND_TRIP: list[JsonValue] = ["yes", "on", "0777"]


@pytest.mark.parametrize(
    "value",
    ROUND_TRIP,
    ids=["yes", "on", "0777"],
)
def test_ruamel_encoder_round_trip(tmp_path: Path, value: JsonValue) -> None:
    path = tmp_path / "input.yaml"
    path.write_bytes(encode(RootModel[JsonValue](value)))
    result = read_yaml(path)
    assert result == value
    assert canonical(result) == canonical(value)
    assert digest(canonical(result)) == digest(canonical(value))


def test_strict_json_validation_is_retained(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: value")
    mocker.patch.object(storage, "parse_yaml", return_value={"a": ("tuple",)})
    with pytest.raises(ValidationError, match="invalid-json-value"):
        read_yaml(path)


@pytest.mark.parametrize(
    ("source", "expected"),
    [
        ("a: &x value\nb: *x", {"a": "value", "b": "value"}),
        ("a: &x [true, 2]\nb: *x", {"a": [True, 2], "b": [True, 2]}),
        ("a: &x {k: value}\nb: *x", {"a": {"k": "value"}, "b": {"k": "value"}}),
    ],
    ids=["scalar-alias", "sequence-alias", "mapping-alias"],
)
def test_package_expansion_and_explicit_tags(
    tmp_path: Path, source: str, expected: JsonValue
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(source, encoding="utf-8")
    value = read_yaml(path)
    assert value == expected
    assert canonical(value) == canonical(expected)


@pytest.mark.parametrize("tag", ["!custom", "!include", "!env", "!secret"])
def test_custom_tags_do_not_escape_strict_json_or_access_external_values(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch, mocker: MockerFixture, tag: str
) -> None:
    external = tmp_path / "external.txt"
    external.write_text("synthetic-private-sentinel")
    monkeypatch.setenv("SYNTHETIC_VARIABLE", "synthetic-env-sentinel")
    path = tmp_path / "input.yaml"
    path.write_text(f"a: {tag} {external}")
    read = mocker.spy(Path, "read_bytes")
    with pytest.raises(ValidationError, match="invalid-json-value"):
        read_yaml(path)
    read.assert_called_once_with(path)


@pytest.mark.parametrize(
    "character",
    ["\u0085", "\u2028", "\u2029", "\ufeff", "\t"],
    ids=["nel", "ls", "ps", "bom", "tab"],
)
@pytest.mark.parametrize("style", ["plain", "quoted", "block", "comment"])
def test_legal_raw_characters_are_delegated_to_package(
    tmp_path: Path, character: str, style: str
) -> None:
    text = "x" + character + "y"
    if style == "quoted":
        source, expected = "a: '" + text + "'\n", text
    elif style == "block":
        source, expected = "a: |\n  " + text + "\n", text + "\n"
    elif style == "comment":
        source, expected = "# comment" + character + "\na: value\n", "value"
    else:
        source, expected = "a: " + text + "\n", text
    path = tmp_path / "input.yaml"
    path.write_text(source, encoding="utf-8")
    assert read_yaml(path) == {"a": expected}


def test_parser_diagnostics_do_not_expose_source_text(tmp_path: Path) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("synthetic-secret-sentinel: [unclosed")
    with pytest.raises(ValueError, match=r"^Invalid authored YAML syntax$") as error:
        read_yaml(path)
    exposed = "".join(traceback.format_exception(error.value))
    assert "synthetic-secret-sentinel" not in exposed
    assert "YAMLRocksDecodeError" not in exposed


def test_package_error_with_source_is_suppressed(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: value")
    mocker.patch.object(
        yamlrocks,
        "loads_all",
        side_effect=yamlrocks.YAMLRocksDecodeError("synthetic-private-source-sentinel"),
    )
    with pytest.raises(ValueError, match=r"^Invalid authored YAML syntax$") as error:
        read_yaml(path)
    assert "synthetic-private-source-sentinel" not in "".join(
        traceback.format_exception(error.value)
    )


@pytest.mark.parametrize(
    ("escape", "character"),
    [
        (r"\u0085", "\u0085"),
        (r"\u2028", "\u2028"),
        (r"\u2029", "\u2029"),
        (r"\uFEFF", "\ufeff"),
        (r"\t", "\t"),
    ],
    ids=["nel", "ls", "ps", "bom", "tab"],
)
def test_escaped_characters_preserve_values(
    tmp_path: Path, escape: str, character: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text('a: "x' + escape + 'y"\n')
    value = read_yaml(path)
    assert value == {"a": "x" + character + "y"}
    assert canonical(value) == canonical({"a": "x" + character + "y"})


@pytest.mark.parametrize(
    "character", ["\u0085", "\u2028", "\u2029"], ids=["nel", "ls", "ps"]
)
def test_encoder_raw_breaks_preserve_values(tmp_path: Path, character: str) -> None:
    path = tmp_path / "input.yaml"
    data = encode(RootModel[JsonValue]("x" + character + "y"))
    assert character in data.decode("utf-8")
    path.write_bytes(data)
    assert read_yaml(path) == "x" + character + "y"


def test_encoder_escapes_tab_and_bom_for_round_trip(tmp_path: Path) -> None:
    value = "x\t\ufeffy"
    path = tmp_path / "input.yaml"
    data = encode(RootModel[JsonValue](value))
    assert "\t" not in data.decode("utf-8")
    assert "\ufeff" not in data.decode("utf-8")
    path.write_bytes(data)
    assert read_yaml(path) == value

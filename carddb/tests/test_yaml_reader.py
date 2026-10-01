"""Independent strict-rule examples, writer round trips and production equivalence."""

import importlib
import sys
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
import yaml
from pydantic import JsonValue, RootModel, ValidationError
from ruamel.yaml import YAML
from ruamel.yaml.error import YAMLError
from ruamel.yaml.tokens import AliasToken, AnchorToken, DirectiveToken, TagToken

from sve_carddb.registry import storage, yaml_reader
from sve_carddb.registry.inputs import JSON_VALUE, canonical, digest
from sve_carddb.registry.storage import MAX_BYTES, encode, read_yaml

if TYPE_CHECKING:
    from pytest_mock import MockerFixture

from .yaml_reader_cases import DIFFERENTIAL_CASES

AUTHORED = Path(__file__).resolve().parents[2] / "authored"
LEGACY_REJECTED = {
    "effects/" + name + ".yaml"
    for name in ("BP02", "BP04", "BP09", "BP13", "EBD03", "SD07", "SD08")
}
PRODUCTION = [
    p
    for p in sorted(AUTHORED.rglob("*.yaml"))
    if p.relative_to(AUTHORED).as_posix() not in LEGACY_REJECTED
]


def legacy_read_yaml(path: Path) -> JsonValue:
    # Freeze the pre-#140 reader independently of the new event implementation.
    data = path.read_bytes()
    if len(data) >= MAX_BYTES:
        raise ValueError("Oversized YAML")
    parser = YAML(typ="safe", pure=True)
    parser.version = (1, 2)
    parser.allow_duplicate_keys = False
    rules = parser.resolver.versioned_resolver
    allowed = {"tag:yaml.org,2002:" + name for name in ("bool", "int", "float", "null")}
    for key, patterns in list(rules.items()):
        rules[key] = [(tag, pattern) for tag, pattern in patterns if tag in allowed]
    for token in parser.scan(data.decode("utf-8")):
        if isinstance(token, (AliasToken, AnchorToken, TagToken)):
            raise TypeError("Forbidden YAML token")
        if (
            isinstance(token, DirectiveToken)
            and token.name == "YAML"
            and token.value != (1, 2)
        ):
            raise ValueError("Only YAML 1.2 is supported")
    parsed = parser.load(data)
    _legacy_check_keys(parsed)
    value = JSON_VALUE.validate_python(parsed, strict=True)
    digest(value)
    return value


def _legacy_check_keys(value: object) -> None:
    if isinstance(value, dict):
        for key, child in value.items():
            if not isinstance(key, str):
                raise TypeError("YAML mapping keys must be strings")
            if key == "<<":
                raise ValueError("Merge keys are forbidden")
            _legacy_check_keys(child)
    elif isinstance(value, list):
        for child in value:
            _legacy_check_keys(child)


@pytest.mark.parametrize(
    "path", PRODUCTION, ids=lambda p: p.relative_to(AUTHORED).as_posix()
)
def test_all_production_values_and_hashes_match_legacy(path: Path) -> None:
    before = path.read_bytes()
    old, new = legacy_read_yaml(path), read_yaml(path)
    same_values = new == old
    same_canonical = canonical(new) == canonical(old)
    same_source = path.read_bytes() == before
    # Assertion introspection must not print official text from authored corrections.
    assert same_values, "Production YAML values differ"
    assert same_canonical, "Production canonical bytes differ"
    assert digest(new) == digest(old)
    assert same_source, "Production authored bytes changed"


@pytest.mark.parametrize(
    ("source", "error", "message"),
    [
        pytest.param("a: &unused value", TypeError, "Anchors", id="scalar-anchor"),
        pytest.param("a: &unused [value]", TypeError, "Anchors", id="sequence-anchor"),
        pytest.param(
            "a: &unused {k: value}", TypeError, "Anchors", id="mapping-anchor"
        ),
        pytest.param("a: *undefined", TypeError, "Aliases", id="alias-alone"),
        pytest.param("a: !!str value", TypeError, "tags", id="scalar-tag"),
        pytest.param("a: ! value", TypeError, "tags", id="non-specific-tag"),
        pytest.param("a: !!seq [value]", TypeError, "tags", id="sequence-tag"),
        pytest.param("a: !!map {k: value}", TypeError, "tags", id="mapping-tag"),
        pytest.param(
            "a: !!python/object:os.system {}", TypeError, "tags", id="object-tag"
        ),
        pytest.param("a: 1\na: 2", ValueError, "Duplicate", id="duplicate-key"),
        pytest.param(
            "a: [{b: 1, 'b': 2}]", ValueError, "Duplicate", id="nested-duplicate-key"
        ),
        pytest.param("<<: value", ValueError, "Merge", id="merge-key-alone"),
        pytest.param(
            "a: [{'<<': {k: v}}]", ValueError, "Merge", id="quoted-nested-merge-key"
        ),
        pytest.param("1: value", TypeError, "keys must be strings", id="integer-key"),
        pytest.param(
            "true: value", TypeError, "keys must be strings", id="boolean-key"
        ),
        pytest.param("null: value", TypeError, "keys must be strings", id="null-key"),
        pytest.param(
            "? [a, b]\n: value", TypeError, "keys must be strings", id="collection-key"
        ),
        pytest.param("%YAML 1.1\n---\na: value", ValueError, "1.2", id="yaml-1.1"),
        pytest.param("%YAML 1.3\n---\na: value", ValueError, "syntax", id="yaml-1.3"),
        pytest.param(
            "a: 1\n---\na: 2", TypeError, "one YAML document", id="multiple-documents"
        ),
        pytest.param("[unclosed", ValueError, "syntax", id="malformed"),
    ],
)
def test_each_syntax_rule_independently(
    tmp_path: Path, source: str, error: type[Exception], message: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(source, encoding="utf-8")
    with pytest.raises(error, match=message):
        read_yaml(path)


@pytest.mark.parametrize("source", [".inf", "-.Inf", "+.INF", ".nan", ".NaN", "1e999"])
def test_nonfinite_scalars_cannot_reach_canonical_hash(
    tmp_path: Path, source: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: " + source)
    with pytest.raises(ValueError, match="Out of range float"):
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
    [
        ("yes", "yes"),
        ("YES", "YES"),
        ("no", "no"),
        ("No", "No"),
        ("on", "on"),
        ("ON", "ON"),
        ("off", "off"),
        ("Off", "Off"),
        ("2024-01-01", "2024-01-01"),
        ("2024-01-01T12:34:56Z", "2024-01-01T12:34:56Z"),
        ("1:20", "1:20"),
        ("1:20.5", "1:20.5"),
        ("0777", 777),
        ("-0777", -777),
        ("0o777", 511),
        ("0b101", 5),
        ("0xFF", 255),
        ("+0o17", 15),
        ("-0xF", -15),
        ("0", 0),
        ("1_000", 1000),
        ("1e3", 1000.0),
        ("-2E-2", -0.02),
        ("1.0", 1.0),
        (".5", 0.5),
        (".5e+3", 500.0),
        (".5e3", ".5e3"),
        ("true", True),
        ("TRUE", True),
        ("False", False),
        ("null", None),
        ("NULL", None),
        ("~", None),
        ("", None),
        ('"null"', "null"),
        ('"true"', "true"),
        ('"1e3"', "1e3"),
        ('""', ""),
    ],
)
def test_yaml12_scalar_values_and_types(
    tmp_path: Path, source: str, expected: JsonValue
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: " + source)
    value = read_yaml(path)
    assert isinstance(value, dict)
    assert value["a"] == expected
    assert type(value["a"]) is type(expected)
    assert canonical(value) == canonical(legacy_read_yaml(path))


@pytest.mark.parametrize("prefix", ["", "%YAML 1.2\n---\n", "\ufeff---\n"])
def test_yaml12_directive_and_utf8_bom(tmp_path: Path, prefix: str) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(prefix + "a: true\nb: 0777")
    assert read_yaml(path) == {"a": True, "b": 777}


ROUND_TRIP: list[JsonValue] = [
    "yes",
    "on",
    "0777",
    "1e3",
    "2024-01-01",
    "null",
    "",
    "true",
    "0o77",
    "line one\nline two\n",
    "日本語と emoji 🦊",
    "very long " * 1500,
    {"yes": "no", "true": "false", "0777": "1e3", "nested": [None, True, 0, 1.0]},
]


@pytest.mark.parametrize(
    "value",
    ROUND_TRIP,
    ids=[
        "yes",
        "on",
        "0777",
        "1e3",
        "date",
        "null",
        "empty",
        "true",
        "octal",
        "multiline",
        "unicode",
        "long",
        "nested",
    ],
)
def test_ruamel_encoder_round_trip(tmp_path: Path, value: JsonValue) -> None:
    path = tmp_path / "input.yaml"
    path.write_bytes(encode(RootModel[JsonValue](value)))
    result = read_yaml(path)
    assert result == value
    assert canonical(result) == canonical(value)
    assert digest(result) == digest(value)


def test_c_extension_is_required(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: value")
    monkeypatch.setattr(yaml, "__with_libyaml__", False)
    with pytest.raises(RuntimeError, match="libyaml C extension"):
        read_yaml(path)


def test_import_without_c_extension_fails_explicitly(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    with monkeypatch.context() as patch:
        patch.setitem(sys.modules, "yaml.cyaml", None)
        with pytest.raises(RuntimeError, match="libyaml C extension"):
            importlib.reload(yaml_reader)
    importlib.reload(yaml_reader)


def test_one_c_parser_without_ruamel_or_second_yaml_pass(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: [true, 0777, value]")
    constructor = mocker.spy(yaml_reader, "CSafeLoader")
    mocker.patch.object(YAML, "scan", side_effect=AssertionError("second YAML pass"))
    mocker.patch.object(YAML, "load", side_effect=AssertionError("pure YAML reader"))
    assert read_yaml(path) == {"a": [True, 777, "value"]}
    constructor.assert_called_once()


def test_strict_json_validation_is_retained(
    tmp_path: Path, mocker: MockerFixture
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: value")
    mocker.patch.object(storage, "parse_yaml", return_value={"a": ("tuple",)})
    with pytest.raises(ValidationError, match="invalid-json-value"):
        read_yaml(path)


@pytest.mark.parametrize("name", sorted(LEGACY_REJECTED))
def test_existing_prototypes_with_anchors_remain_rejected(name: str) -> None:
    path = AUTHORED / name
    with pytest.raises(TypeError, match="Forbidden YAML token"):
        legacy_read_yaml(path)
    with pytest.raises(TypeError, match="Anchors"):
        read_yaml(path)


@pytest.mark.parametrize(
    "character", ["\u0085", "\u2028", "\u2029"], ids=["nel", "ls", "ps"]
)
@pytest.mark.parametrize(
    "template",
    ["a: 1{}b: 2\n", "a: 'x{}y'\n", "# comment{}\na: 1\n"],
    ids=["structure", "scalar", "comment"],
)
def test_raw_unicode_breaks_fail_before_c_parser(
    tmp_path: Path, mocker: MockerFixture, character: str, template: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(template.format(character), encoding="utf-8")
    constructor = mocker.spy(yaml_reader, "CSafeLoader")
    with pytest.raises(ValueError, match="Raw Unicode line separators"):
        read_yaml(path)
    constructor.assert_not_called()


@pytest.mark.parametrize(
    "source",
    [
        "\ufeff\ufeffa: 1\n",
        "---\n\ufeffa: 1\n",
        "a: 1\n\ufeffb: 2\n",
        "a: 'x\ufeffy'\n",
        "# comment\ufeff\na: 1\n",
    ],
    ids=["double-leading", "document", "key", "scalar", "comment"],
)
def test_interior_bom_is_rejected(
    tmp_path: Path, mocker: MockerFixture, source: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(source, encoding="utf-8")
    constructor = mocker.spy(yaml_reader, "CSafeLoader")
    with pytest.raises(ValueError, match="BOM is only allowed"):
        read_yaml(path)
    constructor.assert_not_called()


@pytest.mark.parametrize(
    "source",
    [
        "a:\tb\n",
        "a: b\t\n",
        "a: 1 \t# c\n",
        "a: 'x\ty'\n",
        "a: |\n  x\ty\n",
        "# comment\t\na: 1\n",
    ],
    ids=["separator", "trailing", "before-comment", "quoted", "block", "comment"],
)
def test_all_raw_tabs_are_rejected(
    tmp_path: Path, mocker: MockerFixture, source: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text(source, encoding="utf-8")
    constructor = mocker.spy(yaml_reader, "CSafeLoader")
    with pytest.raises(ValueError, match="Raw tab characters"):
        read_yaml(path)
    constructor.assert_not_called()


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
    assert canonical(value) == canonical(legacy_read_yaml(path))


@pytest.mark.parametrize(
    "character", ["\u0085", "\u2028", "\u2029"], ids=["nel", "ls", "ps"]
)
def test_encoder_raw_breaks_fail_explicitly(tmp_path: Path, character: str) -> None:
    path = tmp_path / "input.yaml"
    data = encode(RootModel[JsonValue]("x" + character + "y"))
    assert character in data.decode("utf-8")
    path.write_bytes(data)
    with pytest.raises(ValueError, match="Raw Unicode line separators"):
        read_yaml(path)


def test_encoder_escapes_tab_and_bom_for_round_trip(tmp_path: Path) -> None:
    value = "x\t\ufeffy"
    path = tmp_path / "input.yaml"
    data = encode(RootModel[JsonValue](value))
    assert "\t" not in data.decode("utf-8")
    assert "\ufeff" not in data.decode("utf-8")
    path.write_bytes(data)
    assert read_yaml(path) == value


@pytest.mark.parametrize(
    ("source", "expected"),
    [
        ("1__0", 10),
        ("1_", 1),
        ("+_1", 1),
        ("0x_1", 1),
        ("0b1__0", 2),
        ("0o1_7_", 15),
        ("1_0.5_", 10.5),
    ],
    ids=[
        "double",
        "trailing",
        "after-sign",
        "after-prefix",
        "binary-double",
        "octal-trailing",
        "float-trailing",
    ],
)
def test_underscore_boundaries_match_legacy(
    tmp_path: Path, source: str, expected: JsonValue
) -> None:
    path = tmp_path / "input.yaml"
    path.write_text("a: " + source)
    value = read_yaml(path)
    assert value == {"a": expected}
    assert canonical(value) == canonical(legacy_read_yaml(path))


@pytest.mark.parametrize(
    "source",
    DIFFERENTIAL_CASES,
    ids=[f"case-{index:03}" for index in range(len(DIFFERENTIAL_CASES))],
)
def test_differential_cases_never_loosen_or_rewrite(
    tmp_path: Path, source: str
) -> None:
    path = tmp_path / "input.yaml"
    path.write_bytes(source.encode("utf-8", "surrogatepass"))
    try:
        old = legacy_read_yaml(path)
    except ValueError, TypeError, YAMLError:
        with pytest.raises((ValueError, TypeError), match=r".+"):
            read_yaml(path)
        return
    try:
        new = read_yaml(path)
    except ValueError, TypeError:
        stricter_character = (
            any(c in source for c in ("\u0085", "\u2028", "\u2029", "\t"))
            or "\ufeff" in source[1:]
        )
        assert stricter_character or source.startswith("%FOO bar\n"), (
            "Undocumented stricter input"
        )
    else:
        assert canonical(new) == canonical(old)

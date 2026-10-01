"""Manual-only comparison with the frozen pre-libyaml reader; never run in CI."""

import os
from pathlib import Path

import pytest
from pydantic import JsonValue
from ruamel.yaml import YAML
from ruamel.yaml.error import YAMLError
from ruamel.yaml.tokens import AliasToken, AnchorToken, DirectiveToken, TagToken

from sve_carddb.registry.inputs import JSON_VALUE, canonical, digest
from sve_carddb.registry.storage import MAX_BYTES, read_yaml

from .yaml_reader_cases import DIFFERENTIAL_CASES

pytestmark = pytest.mark.skipif(
    bool(os.environ.get("CI")), reason="Legacy YAML comparison is manual-only"
)

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


@pytest.mark.parametrize("name", sorted(LEGACY_REJECTED))
def test_existing_prototypes_with_anchors_remain_rejected(name: str) -> None:
    path = AUTHORED / name
    with pytest.raises(TypeError, match="Forbidden YAML token"):
        legacy_read_yaml(path)
    with pytest.raises(TypeError, match="Anchors"):
        read_yaml(path)


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

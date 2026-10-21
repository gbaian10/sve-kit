"""Independent P1 vector and closed source-payload boundaries."""

import hashlib
import json
from copy import deepcopy
from pathlib import Path

import pytest
from pydantic import JsonValue, ValidationError

from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.domains.effects.author_source_hash import (
    AuthorSourcePayload,
    source_payload,
    source_version,
)

VECTOR = Path(__file__).with_name("author_source_payload.json")
VERSION = "effect-source-v1:sha256:6d089f3235bb580294945c5c0b5b4afb9d6ce6b2d622ac6b0081476348bbebe6"
type PathParts = tuple[str | int, ...]


def payload() -> dict[str, JsonValue]:
    return object_value(parse(VECTOR.read_bytes()))


def replace(value: JsonValue, path: PathParts, replacement: JsonValue) -> None:
    for part in path[:-1]:
        value = (
            array(value)[part] if isinstance(part, int) else object_value(value)[part]
        )
    last = path[-1]
    if isinstance(last, int):
        array(value)[last] = replacement
    else:
        object_value(value)[last] = replacement


def decode(value: JsonValue) -> AuthorSourcePayload:
    return AuthorSourcePayload.model_validate_json(json.dumps(value))


def test_p1_independent_vector() -> None:
    value = decode(payload())
    assert source_payload(value) == VECTOR.read_bytes().rstrip(b"\n")
    assert source_version(value) == VERSION


@pytest.mark.parametrize(
    ("path", "value"),
    [
        (("normalizer",), "fixture-v2"),
        (("faces", 0, "face_id"), "face:other"),
        (("faces", 0, "revision_id"), "revision:other"),
        (("faces", 0, "texts", 0, "source_hash"), "1" * 64),
        (("faces", 0, "texts", 1, "state"), "present"),
        (("faces", 0, "attributes", "class_code", "value"), "neutral"),
        (("faces", 0, "attributes", "type_code", "value"), "spell"),
        (("faces", 0, "attributes", "cost", "value"), 0),
        (("faces", 0, "attributes", "attack", "value"), 2),
        (("faces", 0, "attributes", "defense", "value"), None),
        (("faces", 0, "attributes", "traits", "value"), ["trait"]),
        (("faces", 0, "attributes", "titles", "value"), ["title"]),
        (("faces", 0, "attributes", "special_kinds", "value"), ["special"]),
        (
            ("faces", 0, "rules_names"),
            [{"rules_name_id": "name:fixture", "role": "primary"}],
        ),
    ],
)
def test_each_source_fact_changes_hash(path: PathParts, value: JsonValue) -> None:
    raw = payload()
    replace(raw, path, value)
    assert source_version(decode(raw)) != VERSION


@pytest.mark.parametrize("field", ["class_code", "cost", "attack", "defense"])
def test_unknown_and_known_null_are_distinct(field: str) -> None:
    raw = payload()
    replace(raw, ("faces", 0, "attributes", field), {"state": "known", "value": None})
    known = source_version(decode(raw))
    replace(raw, ("faces", 0, "attributes", field), {"state": "unknown", "value": None})
    assert source_version(decode(raw)) != known


def test_unknown_text_differs_from_absence() -> None:
    raw = payload()
    replace(
        raw,
        ("faces", 0, "texts", 1),
        {"field": "effect", "ordinal": None, "state": "unknown", "source_hash": None},
    )
    assert source_version(decode(raw)) != VERSION


def test_exact_unicode_and_whitespace_are_not_normalized() -> None:
    versions = set()
    for text in ("é", "e\u0301", "Ａ", "A", "A ", "A\n"):
        raw = payload()
        replace(
            raw,
            ("faces", 0, "texts", 0, "source_hash"),
            hashlib.sha256(text.encode()).hexdigest(),
        )
        versions.add(source_version(decode(raw)))
    assert len(versions) == 6


@pytest.mark.parametrize(
    ("path", "value"),
    [
        (("extra",), 1),
        (("recipe",), "effect-source-v2"),
        (("region",), "en"),
        (("normalizer",), "fixture-v1\n"),
        (("faces",), []),
        (("faces", 0, "face_id"), ""),
        (("faces", 0, "extra"), 1),
        (("faces", 0, "texts"), []),
        (("faces", 0, "texts", 0, "ordinal"), 0),
        (("faces", 0, "texts", 0, "state"), "absent"),
        (("faces", 0, "texts", 0, "state"), "unknown"),
        (("faces", 0, "texts", 0, "source_hash"), None),
        (("faces", 0, "texts", 0, "source_hash"), "A" * 64),
        (("faces", 0, "texts", 1, "source_hash"), "1" * 64),
        (("faces", 0, "texts", 1, "extra"), 1),
        (("faces", 0, "attributes", "extra"), 1),
        (("faces", 0, "attributes", "cost", "extra"), 1),
        (("faces", 0, "attributes", "cost", "value"), True),
        (("faces", 0, "attributes", "cost", "value"), -1),
        (("faces", 0, "attributes", "cost", "value"), 9007199254740992),
        (("faces", 0, "attributes", "cost", "value"), 1.0),
        (("faces", 0, "attributes", "cost", "state"), "unknown"),
        (("faces", 0, "attributes", "type_code", "value"), None),
        (("faces", 0, "attributes", "traits", "value"), ["z", "a"]),
        (("faces", 0, "attributes", "titles", "value"), ["a", "a"]),
        (("faces", 0, "attributes", "special_kinds", "value"), None),
        (("faces", 0, "rules_names"), [{"rules_name_id": "x", "role": "other"}]),
    ],
)
def test_payload_rejects_invalid_fields(path: PathParts, value: JsonValue) -> None:
    raw = payload()
    replace(raw, path, value)
    with pytest.raises(ValidationError):
        decode(raw)


@pytest.mark.parametrize(
    "kind", ["missing", "duplicate", "gap", "reorder", "negative", "bool"]
)
def test_text_sequence_is_complete_and_ordered(kind: str) -> None:
    raw = payload()
    texts = array(object_value(array(raw["faces"])[0])["texts"])
    section: JsonValue = {
        "field": "section",
        "ordinal": 0,
        "state": "present",
        "source_hash": "1" * 64,
    }
    texts.append(section)
    assert source_version(decode(raw)) != VERSION
    if kind == "missing":
        texts.pop(0)
    elif kind == "duplicate":
        texts.append(deepcopy(section))
    elif kind == "reorder":
        texts.reverse()
    else:
        object_value(section)["ordinal"] = {"gap": 1, "negative": -1, "bool": True}[
            kind
        ]
    with pytest.raises(ValidationError):
        decode(raw)


@pytest.mark.parametrize("kind", ["faces", "rules_names", "token_links"])
@pytest.mark.parametrize("duplicate", [False, True])
def test_identity_arrays_require_sorted_unique_order(
    kind: str, duplicate: bool
) -> None:
    raw = payload()
    face = object_value(array(raw["faces"])[0])
    if kind == "faces":
        other = deepcopy(face) | {"face_id": "face:second"}
        values: list[JsonValue] = [face, other]
        raw[kind] = values
    elif kind == "rules_names":
        values = [{"rules_name_id": name, "role": "primary"} for name in ("a", "z")]
        face[kind] = values
    else:
        values = [
            {
                "source_owner": {
                    "kind": "face_revision",
                    "revision_id": "revision:fixture",
                },
                "section": section,
                "token_face_id": "face:fixture",
            }
            for section in (0, 1)
        ]
        raw[kind] = values
    assert source_version(decode(raw)) != VERSION
    values[:] = [values[0], values[0]] if duplicate else list(reversed(values))
    with pytest.raises(ValidationError):
        decode(raw)


def test_token_links_pin_owner_section_and_target() -> None:
    raw = payload()
    faces = array(raw["faces"])
    faces.append(deepcopy(object_value(faces[0])) | {"face_id": "face:token"})
    link: dict[str, JsonValue] = {
        "source_owner": {"kind": "face_revision", "revision_id": "revision:fixture"},
        "section": None,
        "token_face_id": "face:token",
    }
    raw["token_links"] = [link]
    baseline = source_version(decode(raw))
    mutations: tuple[tuple[str, JsonValue], ...] = (
        ("source_owner", {"kind": "face_revision", "revision_id": "revision:other"}),
        ("section", 0),
        ("token_face_id", "face:fixture"),
    )
    for field, replacement in mutations:
        changed = deepcopy(raw)
        replace(changed, ("token_links", 0, field), replacement)
        assert source_version(decode(changed)) != baseline
    link["token_face_id"] = "face:missing"  # ruff: ignore[hardcoded-password-string] -- token is a card kind, not a credential
    with pytest.raises(ValidationError):
        decode(raw)


@pytest.mark.parametrize(
    "field", ["recipe", "normalizer", "region", "faces", "token_links"]
)
def test_root_fields_are_required(field: str) -> None:
    raw = payload()
    del raw[field]
    with pytest.raises(ValidationError):
        decode(raw)


def test_hash_boundary_rejects_unchecked_model_copies() -> None:
    value = decode(payload())
    invalid = value.model_copy(update={"region": "en"})
    with pytest.raises(ValidationError):
        source_payload(invalid)
    face = value.faces[0].model_copy(update={"texts": ()})
    with pytest.raises(ValidationError):
        source_version(value.model_copy(update={"faces": (face,)}))


def test_zero_and_largest_safe_integer_preserve_exact_bytes() -> None:
    raw = payload()
    replace(raw, ("faces", 0, "attributes", "cost", "value"), 9007199254740991)
    replace(raw, ("faces", 0, "attributes", "attack", "value"), 0)
    assert source_payload(decode(raw)) == canonical(raw)

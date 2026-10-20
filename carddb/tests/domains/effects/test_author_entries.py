"""Whole author inputs remain structural, source-qualified expansion is separate."""

import json
from copy import deepcopy
from hashlib import sha256
from pathlib import Path
from typing import TYPE_CHECKING

import pytest

from sve_carddb.contracts.author_leaves import AuthorIndexRecord
from sve_carddb.core.json import canonical, object_value
from sve_carddb.domains.effects.author_inputs import AuthorInputError, AuthorSchemas

from .author_entry_cases import meta, source_use

if TYPE_CHECKING:
    from pydantic import JsonValue

SCHEMAS = Path(__file__).resolve().parents[4] / "dsl"
GOLDEN = (
    Path(__file__).resolve().parents[2] / "fixtures" / "author_entries" / "golden.yaml"
)
HEADS: dict[str, JsonValue] = {
    **{
        key: {"do": []}
        for key in (
            "fanfare",
            "lastword",
            "evolve_time",
            "super_evolve_time",
            "attack_time",
            "race_time",
            "drive_time",
        )
    },
    "auto": {"on": {"phase": {"which": "end", "whose": "self"}}, "do": []},
    "activate": {"cost": {"pp": 1}, "do": []},
    "static": {"effects": [{"keywords": {"list": ["ward"]}}]},
    "play": {"cost_delta": -1},
    "quick": True,
    "keywords": ["ward"],
    "evolve": {"cost": {"pp": 1}},
    "ride": {"cost": {"pp": 1}},
    "feed": {"cost": {"pp": 1}},
    "spell": {"do": []},
}


@pytest.fixture(scope="module")
def schemas() -> AuthorSchemas:
    return AuthorSchemas(SCHEMAS)


def ability(head: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "line": 1,
        "sources": [source_use()],
        "ports": {"imports": {}, "exports": {}},
        **head,
    }


def document(entry: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "format": 1,
        "kind": "author_effect_set",
        "scope": "shared",
        "cards": {"FIX-001": {"meta": meta(), "abilities": [entry]}},
    }


def companion(head: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "frame_id": "frame:" + "1" * 64,
        "ability_scope": "ability_0",
        "body_version": "effect-body-v1:sha256:" + "2" * 64,
        "dsl": "1.0",
        "params": {
            "N": {"type": "Nat", "source": {"slot": "count", "component": "value"}}
        },
        "body": head,
        "ports": {"imports": {}, "exports": {}},
        "written_by": meta()["written_by"],
        "registrations": [],
    }


def shard(record: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "format": 1,
        "kind": "effect_body_shard",
        "scope": "shared",
        "records": [record],
    }


@pytest.mark.parametrize(("key", "payload"), HEADS.items())
def test_all_inline_variants(
    schemas: AuthorSchemas, key: str, payload: JsonValue
) -> None:
    value = document(ability({key: payload}))
    decoded = schemas.author_effect_set(canonical(value))
    assert decoded.model_dump(mode="json") == value
    for field in ("line", "sources", "ports", key):
        bad = ability({key: payload})
        del bad[field]
        with pytest.raises(AuthorInputError):
            schemas.author_effect_set(canonical(document(bad)))
    for extra in ("typo", "generated_by"):
        bad = ability({key: payload}) | {extra: {}}
        with pytest.raises(AuthorInputError):
            schemas.author_effect_set(canonical(document(bad)))
    with pytest.raises(AuthorInputError):
        schemas.author_effect_set(canonical(document(ability({key: None}))))


def test_fixed_goldens_preserve_content_and_order(schemas: AuthorSchemas) -> None:
    value = schemas.author_effect_set(GOLDEN.read_bytes()).model_dump(mode="json")
    cards = value["cards"]
    bp = cards["BP08-038"]["abilities"][0]["fanfare"]
    expected_bp = b'{"fanfare":{"define":{"X":{"minus":[{"count":{"who":"self","zone":"deck"}},5]}},"do":[{"draw":{"n":"X"}}]}}'
    assert sha256(canonical({"fanfare": bp})).digest() == sha256(expected_bp).digest()
    cp = cards["CP04-060"]["abilities"][0]["evolve_time"]
    expected_cp = '{"evolve_time":{"do":[{"search":{"n":1,"result":"searched","sel":{"not_same_name_as_this":true,"trait":"ドラゴンズネスト","who":"self","zone":"deck"},"to":"ex"}},{"apply":{"effect":{"cost_mod":{"delta":-2,"when_playing":{"in":"affected"}}},"on":{"follow":"searched"},"until":"turn"}}],"ub":true}}'.encode()
    assert (
        sha256(canonical({"evolve_time": cp})).digest() == sha256(expected_cp).digest()
    )
    assert cards["ECP01-058"]["deck_rule"] == {"deck": "evolve", "same_name_max": 10}
    face = cards["BP09-SL05"]["faces"]["f:" + "2" * 32]
    expected_face: JsonValue = {
        "do": [
            {"create": {"token": ["スティールナイト", "ナイト"], "n": 1, "to": "field"}}
        ]
    }
    assert (
        sha256(canonical(face["abilities"][0]["evolve_time"])).digest()
        == sha256(canonical(expected_face)).digest()
    )
    assert canonical(
        schemas.document(
            json.dumps(value, ensure_ascii=False, sort_keys=True).encode()
        ).model_dump(mode="json")
    ) == canonical(value)
    reversed_value = deepcopy(value)
    reversed_value["cards"]["CP04-060"]["abilities"][0]["evolve_time"]["do"].reverse()
    assert canonical(
        schemas.author_effect_set(json.dumps(reversed_value).encode()).model_dump(
            mode="json"
        )
    ) != canonical(value)


PARAM_STEP: dict[str, JsonValue] = {"draw": {"n": {"param": "N"}}}
EVENT: dict[str, JsonValue] = {"phase": {"which": "end", "whose": "self"}}
NESTED: list[tuple[dict[str, JsonValue], tuple[str | int, ...]]] = [
    ({"spell": {"do": [PARAM_STEP]}}, ("spell", "do", 0, "draw", "n")),
    (
        {
            "spell": {
                "define": {"X": {"max": [{"minus": [1, {"param": "N"}]}]}},
                "do": [],
            }
        },
        ("spell", "define", "X", "max", 0, "minus", 1),
    ),
    (
        {"spell": {"do": [{"if": {"cond": {"my_turn": True}, "then": [PARAM_STEP]}}]}},
        ("spell", "do", 0, "if", "then", 0, "draw", "n"),
    ),
    (
        {"spell": {"do": [{"delayed": {"on": EVENT, "do": [PARAM_STEP]}}]}},
        ("spell", "do", 0, "delayed", "do", 0, "draw", "n"),
    ),
    (
        {
            "spell": {
                "do": [
                    {
                        "grant": {
                            "on": "this",
                            "abilities": [{"fanfare": {"do": [PARAM_STEP]}}],
                        }
                    }
                ]
            }
        },
        ("spell", "do", 0, "grant", "abilities", 0, "fanfare", "do", 0, "draw", "n"),
    ),
    (
        {
            "static": {
                "effects": [
                    {
                        "grant": {
                            "abilities": [
                                {
                                    "static": {
                                        "effects": [
                                            {
                                                "grant": {
                                                    "abilities": [
                                                        {
                                                            "fanfare": {
                                                                "do": [PARAM_STEP]
                                                            }
                                                        }
                                                    ]
                                                }
                                            }
                                        ]
                                    }
                                }
                            ]
                        }
                    }
                ]
            }
        },
        (
            "static",
            "effects",
            0,
            "grant",
            "abilities",
            0,
            "static",
            "effects",
            0,
            "grant",
            "abilities",
            0,
            "fanfare",
            "do",
            0,
            "draw",
            "n",
        ),
    ),
    (
        {
            "spell": {
                "do": [
                    {
                        "draw": {
                            "n": 1,
                            "instead": [
                                {"if": {"my_turn": True}, "replace_with": PARAM_STEP}
                            ],
                        }
                    }
                ]
            }
        },
        ("spell", "do", 0, "draw", "instead", 0, "replace_with", "draw", "n"),
    ),
    (
        {
            "spell": {
                "do": [
                    {
                        "draw": {
                            "n": 1,
                            "instead": [
                                {"if": {"my_turn": True}, "n": 2, "after": [PARAM_STEP]}
                            ],
                        }
                    }
                ]
            }
        },
        ("spell", "do", 0, "draw", "instead", 0, "after", 0, "draw", "n"),
    ),
]


@pytest.mark.parametrize(
    ("head", "path"),
    NESTED,
    ids=[
        "draw",
        "alias",
        "if",
        "delayed",
        "grant",
        "nested-static",
        "replacement",
        "after",
    ],
)
@pytest.mark.parametrize("face", [False, True])
def test_inline_param_is_local_error_at_wire_path(
    schemas: AuthorSchemas,
    head: dict[str, JsonValue],
    path: tuple[str | int, ...],
    face: bool,
) -> None:
    entry = ability(head)
    value = document(entry)
    prefix: tuple[str | int, ...] = ("cards", "FIX-001", "abilities", 0)
    if face:
        card = object_value(object_value(value["cards"])["FIX-001"])
        del card["abilities"]
        card["faces"] = {"f:" + "1" * 32: {"abilities": [entry]}}
        prefix = ("cards", "FIX-001", "faces", "f:" + "1" * 32, "abilities", 0)
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(json.dumps(value, indent=2).encode())
    diagnostic = caught.value.diagnostic
    assert diagnostic.reason == "inline_param_reference"
    assert diagnostic.path == (*prefix, *path)
    assert diagnostic.line is not None
    assert diagnostic.column is not None
    assert diagnostic.card_no == "FIX-001"
    assert diagnostic.scope == "shared"
    assert diagnostic.source is not None
    accepted = schemas.effect_body_shard(canonical(shard(companion(head))))
    assert accepted.records[0]["body"]


def test_generated_by_and_companion_are_structural_only(schemas: AuthorSchemas) -> None:
    generated: dict[str, JsonValue] = {
        "macro": {
            "frame_id": "frame:" + "1" * 64,
            "ability_scope": "ability_0",
            "applicability_scope": "fixture",
        },
        "version": "effect-body-v1:sha256:" + "2" * 64,
        "params": {"N": {"type": "Nat", "value": 1}},
    }
    entry: dict[str, JsonValue] = {
        "line": 1,
        "sources": [source_use()],
        "generated_by": generated,
    }
    value = document(entry)
    assert schemas.document(canonical(value)).model_dump(mode="json") == value
    extras: list[dict[str, JsonValue]] = [
        {"ports": {"imports": {}, "exports": {}}},
        {"spell": {"do": []}},
    ]
    for extra in extras:
        with pytest.raises(AuthorInputError):
            schemas.author_effect_set(canonical(document(entry | extra)))
    for field in ("macro", "version", "params"):
        broken = deepcopy(generated)
        del broken[field]
        with pytest.raises(AuthorInputError):
            schemas.author_effect_set(
                canonical(document(entry | {"generated_by": broken}))
            )


@pytest.mark.parametrize(
    ("mutation", "reason", "path"),
    [
        ({"format": True}, "unsupported_format", ("format",)),
        ({"format": "1"}, "unsupported_format", ("format",)),
        ({"format": 2}, "unsupported_format", ("format",)),
        ({"kind": "effect_set"}, "unsupported_kind", ("kind",)),
        ({"scope": "en_override"}, "en_source_contract_unavailable", ("scope",)),
    ],
)
def test_explicit_envelope_rejections(
    schemas: AuthorSchemas,
    mutation: dict[str, JsonValue],
    reason: str,
    path: tuple[str, ...],
) -> None:
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(
            canonical(document(ability({"quick": True})) | mutation)
        )
    assert caught.value.diagnostic.reason == reason
    assert caught.value.diagnostic.path == path


@pytest.mark.parametrize(
    ("field", "value", "reason"),
    [
        ("dsl", "1.1", "unsupported_dsl"),
        ("verified_by_exam", [{"result": "pass"}], "unsupported_exam_evidence"),
    ],
)
def test_unsupported_metadata_is_located(
    schemas: AuthorSchemas, field: str, value: JsonValue, reason: str
) -> None:
    data = document(ability({"quick": True}))
    card = object_value(object_value(data["cards"])["FIX-001"])
    object_value(card["meta"])[field] = value
    with pytest.raises(AuthorInputError) as caught:
        schemas.document(canonical(data))
    assert caught.value.diagnostic.reason == reason
    assert caught.value.diagnostic.path == ("cards", "FIX-001", "meta", field)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("line", 0),
        ("line", True),
        ("section", -1),
        ("sources", []),
        ("sources", [source_use(), source_use()]),
        ("imports", {}),
    ],
)
def test_ability_boundaries(
    schemas: AuthorSchemas, field: str, value: JsonValue
) -> None:
    with pytest.raises(AuthorInputError):
        schemas.author_effect_set(
            canonical(document(ability({"quick": True}) | {field: value}))
        )


def test_companion_duplicates_and_metadata_exclusion(schemas: AuthorSchemas) -> None:
    record = companion({"spell": {"do": []}})
    value = shard(record)
    value["records"] = [record, record]
    with pytest.raises(AuthorInputError) as caught:
        schemas.effect_body_shard(canonical(value))
    assert caught.value.diagnostic.reason == "duplicate_body"
    record["registrations"] = [
        {"id": "same", "domain": {"regions": ["jp"]}, "reviews": []}
    ] * 2
    with pytest.raises(AuthorInputError) as caught:
        schemas.effect_body_shard(canonical(shard(record)))
    assert caught.value.diagnostic.reason == "duplicate_registration"
    for forbidden in ("line", "sources", "ports", "generated_by", "meta"):
        bad = companion({"spell": {"do": []}, forbidden: {}})
        with pytest.raises(AuthorInputError):
            schemas.effect_body_shard(canonical(shard(bad)))


def test_no_expanded_profile_or_unknown_resource_fallback(
    schemas: AuthorSchemas, tmp_path: Path
) -> None:
    with pytest.raises(AuthorInputError):
        schemas.document(b'{"format":1,"kind":"expanded","cards":{}}')
    for path in SCHEMAS.glob("*.schema.json"):
        (tmp_path / path.name).write_bytes(path.read_bytes())
    resource = tmp_path / "author.schema.json"
    data = json.loads(resource.read_bytes())
    data["$ref"] = "urn:unregistered:never-fetch"
    resource.write_text(json.dumps(data))
    with pytest.raises(ValueError, match="Unresolved author schema reference"):
        AuthorSchemas(tmp_path)


@pytest.mark.parametrize(
    "raw",
    [b"[", b"{x: 1, x: 2}", b"{1: value}", b"{format: 2}"],
    ids=["syntax", "duplicate", "key", "version"],
)
def test_selected_index_context_survives_boundary_errors(
    schemas: AuthorSchemas, raw: bytes
) -> None:
    context = AuthorIndexRecord(
        card_id="c:" + "1" * 32,
        scope="shared",
        file="effects/fixture.yaml",
        kind="author_effect_set",
        record_key="FIX-001",
    )
    for read in (schemas.document, schemas.author_effect_set):
        with pytest.raises(AuthorInputError) as caught:
            read(raw, context=context)
        diagnostic = caught.value.diagnostic
        assert (diagnostic.card_id, diagnostic.card_no, diagnostic.scope) == (
            context.card_id,
            "FIX-001",
            "shared",
        )


@pytest.mark.parametrize("valid_source", [False, True])
def test_schema_errors_only_attach_valid_source(
    schemas: AuthorSchemas, valid_source: bool
) -> None:
    entry = ability({"quick": True})
    entry["ports"] = {"imports": {}, "exports": {}, "typo": True}
    if not valid_source:
        entry["sources"] = [{"untrusted": "not source evidence"}]
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(canonical(document(entry)))
    diagnostic = caught.value.diagnostic
    assert diagnostic.stage == "schema"
    assert (diagnostic.source is not None) is valid_source


@pytest.mark.parametrize("face", [False, True])
def test_unknown_key_inside_inline_head_is_located(
    schemas: AuthorSchemas, face: bool
) -> None:
    entry = ability({"fanfare": {"do": [{"draw": {"nn": 1}}]}})
    value = document(entry)
    prefix: tuple[str | int, ...] = ("cards", "FIX-001", "abilities", 0)
    if face:
        card = object_value(object_value(value["cards"])["FIX-001"])
        del card["abilities"]
        card["faces"] = {"f:" + "1" * 32: {"abilities": [entry]}}
        prefix = ("cards", "FIX-001", "faces", "f:" + "1" * 32, "abilities", 0)
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(json.dumps(value, indent=2).encode())
    diagnostic = caught.value.diagnostic
    assert (diagnostic.stage, diagnostic.reason) == ("schema", "additionalProperties")
    assert diagnostic.path == (*prefix, "fanfare", "do", 0, "draw", "nn")
    assert diagnostic.line is not None


def test_alias_param_cursor_uses_wire_key(schemas: AuthorSchemas) -> None:
    raw = canonical(
        document(
            ability(
                {
                    "spell": {
                        "define": {"X": {"max": [{"minus": [1, {"param": "N"}]}]}},
                        "do": [],
                    }
                }
            )
        )
    )
    # JSON is valid YAML; the parser points to the mapping value after the array comma.
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(raw)
    diagnostic = caught.value.diagnostic
    assert diagnostic.path == (
        "cards",
        "FIX-001",
        "abilities",
        0,
        "spell",
        "define",
        "X",
        "max",
        0,
        "minus",
        1,
    )
    assert diagnostic.line == 1
    assert diagnostic.column == raw.index(b'{"param":"N"}') + 1


@pytest.mark.parametrize(
    ("card_no", "file", "face", "line", "digest"),
    [
        (
            "BP08-038",
            "BP08/000-099.json",
            0,
            1,
            "7a884eac36c1801277243b0bd7489062e396130b52be10bf3ab09ea4869904cb",
        ),
        (
            "CP04-060",
            "CP04/000-099.json",
            0,
            0,
            "86fa086090511c9105cfc1d9210f2102f69ef7351e6930dcd9dd89a8f53f3350",
        ),
        (
            "BP09-SL05",
            "BP09/SL/000-099.json",
            1,
            1,
            "298430d53c7ea027f74e3175d1d374c5fcd40f7a9fa1bb8e9db53b85cce26b80",
        ),
        (
            "ECP01-058",
            "ECP01/000-099.json",
            0,
            0,
            "c5b5200342452cde425e90b5db756548f926d3122c1d6e1f118a89fb9bcfc3f8",
        ),
    ],
)
def test_golden_content_retains_frozen_line_evidence(
    card_no: str, file: str, face: int, line: int, digest: str
) -> None:
    value = json.loads((SCHEMAS.parent / "card-data" / "jp" / file).read_bytes())
    card = next(card for card in value["cards"] if card["card_no"] == card_no)
    hashes = {
        sha256(
            document["record"]["faces"][face]["text"].split("\n")[line].encode()
        ).hexdigest()
        for document in card["documents"].values()
    }
    assert hashes == {digest}


@pytest.mark.parametrize(
    "fields",
    [
        {},
        {"abilities": []},
        {"faces": {}},
        {"faces": {"f:" + "1" * 32: {"abilities": []}}},
    ],
)
def test_empty_containers_preserve_authored_shape(
    schemas: AuthorSchemas, fields: dict[str, JsonValue]
) -> None:
    value: dict[str, JsonValue] = {
        "format": 1,
        "kind": "author_effect_set",
        "scope": "shared",
        "cards": {"FIX-001": {"meta": meta(), **fields}},
    }
    assert schemas.author_effect_set(canonical(value)).model_dump(mode="json") == value


def test_selected_context_does_not_misidentify_another_card(
    schemas: AuthorSchemas,
) -> None:
    context = AuthorIndexRecord(
        card_id="c:" + "1" * 32,
        scope="shared",
        file="effects/fixture.yaml",
        kind="author_effect_set",
        record_key="OTHER",
    )
    value = document(ability({"spell": {"do": [PARAM_STEP]}}))
    with pytest.raises(AuthorInputError) as caught:
        schemas.author_effect_set(canonical(value), context=context)
    assert caught.value.diagnostic.card_id is None
    assert caught.value.diagnostic.card_no == "FIX-001"

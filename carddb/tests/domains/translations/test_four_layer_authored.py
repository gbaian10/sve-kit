"""New authored admission rejects duplicate keys, invalid targets and stale pins."""

from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.contracts.four_layer import Frame, hash_payload
from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.domains.translations.four_layer_authored import (
    TargetRecord,
    from_files,
    read_inputs,
    shard,
)
from sve_carddb.domains.translations.four_layer_normalizer import VERSION

if TYPE_CHECKING:
    from pathlib import Path


def definition() -> dict[str, JsonValue]:
    frame = Frame.model_validate_json(
        canonical(
            {
                "id": "frame:" + "0" * 64,
                "content_hash": "0" * 64,
                "source": {
                    "source_lang": "ja",
                    "canonical_hash": digest("仮N".encode())[7:],
                    "normalizer_version": VERSION,
                },
                "role": "body",
                "semantic_variant": {
                    "state": "pending",
                    "key": None,
                    "scope": {
                        "owner": {
                            "kind": "face_revision",
                            "revision_id": "revision:fixture",
                        },
                        "field": "effect",
                        "ordinal": None,
                        "source_hash": digest("仮２".encode())[7:],
                        "line_ordinal": 0,
                        "role": "body",
                        "segments": [{"start": 0, "end": 2}],
                    },
                },
                "leaf_schema": {
                    "format": 2,
                    "slots": [
                        {
                            "name": "n",
                            "type": "Nat",
                            "role": "count",
                            "domain": {"values": [], "min": 0, "max": 9007199254740991},
                            "required": True,
                            "occurrences": [{"start": 1, "end": 2}],
                        }
                    ],
                },
                "projection": {
                    "projection_kind": "pending",
                    "discriminator": None,
                    "scopes": [],
                    "imports": [],
                    "exports": [],
                },
            }
        )
    )
    checksum = hash_payload(frame.payload("仮N"))
    frame = frame.model_copy(
        update={"id": "frame:" + checksum, "content_hash": checksum}
    )
    return {"kind": "sentence_template", "data": frame.model_dump(mode="json")}


def target(identifier: JsonValue) -> dict[str, JsonValue]:
    return {
        "kind": "template_translation",
        "data": {
            "template_id": identifier,
            "lang": "zh-Hant",
            "target": {
                "format": 1,
                "nodes": [
                    {"kind": "Literal", "text": "自編"},
                    {"kind": "LeafRef", "slot": "n"},
                ],
            },
        },
    }


def file(path: str, records: list[JsonValue]) -> tuple[str, bytes, bytes]:
    content = canonical({"format": 3, "kind": "translation_shard", "records": records})
    return path, content, content


def valid_files() -> tuple[tuple[str, bytes, bytes], ...]:
    frame = definition()
    data = frame["data"]
    assert isinstance(data, dict)
    return (
        file("translations/templates/definitions/001.yaml", [frame]),
        file("translations/templates/values/001.yaml", [target(data["id"])]),
    )


def test_new_authored_tree_loads_once_with_typed_targets_and_default_quality(
    tmp_path: Path,
) -> None:
    for name, raw, _ in valid_files():
        path = tmp_path / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(raw)
    path = tmp_path / "translations/forms/001.yaml"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(
        canonical({"format": 3, "kind": "translation_shard", "records": []})
    )
    result = read_inputs(tmp_path)
    selected = next(r for r in result.records if isinstance(r, TargetRecord))
    assert selected.origin == "project"
    assert not selected.low_confidence
    assert selected.data.target.nodes[0].kind == "Literal"
    assert selected.data.target.nodes[1].kind == "LeafRef"
    assert (
        selected.record_key
        == canonical([selected.kind, selected.data.template_id, "zh-Hant"]).decode()
    )
    assert path.relative_to(tmp_path).as_posix() in {f[0] for f in result.files}


@pytest.mark.parametrize("format_value", [2, True, "3", None])
def test_old_and_noninteger_authored_formats_are_rejected(
    format_value: JsonValue,
) -> None:
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        shard(
            canonical(
                {"format": format_value, "kind": "translation_shard", "records": []}
            )
        )


@pytest.mark.parametrize(
    "legacy", ["source_exception", "template_adoption", "unregistered_kind"]
)
def test_removed_kinds_are_errors_even_in_an_otherwise_empty_tree(legacy: str) -> None:
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        shard(
            canonical(
                {
                    "format": 3,
                    "kind": "translation_shard",
                    "records": [{"kind": legacy, "data": {}}],
                }
            )
        )


def test_duplicate_json_and_selection_keys_cannot_replace_previous_values() -> None:
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        shard(b'{"format":2,"format":3,"kind":"translation_shard","records":[]}')
    frame = definition()
    with pytest.raises(ValueError, match="Duplicate four-layer authored selection key"):
        from_files(
            (
                file("translations/templates/definitions/001.yaml", [frame]),
                file("translations/templates/definitions/002.yaml", [frame]),
            )
        )
    with pytest.raises(ValueError, match="Duplicate four-layer authored selection key"):
        from_files(
            (file("translations/templates/definitions/001.yaml", [frame, frame]),)
        )


def test_missing_required_leaf_and_old_placeholder_text_are_rejected() -> None:
    records = valid_files()
    frame = definition()
    data = frame["data"]
    assert isinstance(data, dict)
    invalid = target(data["id"])
    target_data = invalid["data"]
    assert isinstance(target_data, dict)
    target_data["target"] = {
        "format": 1,
        "nodes": [{"kind": "Literal", "text": "{{n}}"}],
    }
    with pytest.raises(ValueError, match="every required leaf"):
        from_files(
            (records[0], file("translations/templates/values/001.yaml", [invalid]))
        )
    target_data.pop("target")
    target_data["text"] = "旧字串"
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        from_files(
            (records[0], file("translations/templates/values/001.yaml", [invalid]))
        )


def test_wrong_area_and_unregistered_normalizer_are_rejected() -> None:
    frame = definition()
    with pytest.raises(ValueError, match="outside its kind's area"):
        from_files((file("translations/templates/values/001.yaml", [frame]),))
    data = frame["data"]
    assert isinstance(data, dict)
    source = data["source"]
    assert isinstance(source, dict)
    source["normalizer_version"] = "unregistered-jp-v1"
    with pytest.raises(ValueError, match="Unsupported four-layer authored normalizer"):
        from_files((file("translations/templates/definitions/001.yaml", [frame]),))


def test_shard_quality_is_strict_and_record_keys_are_derived() -> None:
    for change in (
        {"low_confidence": 1},
        {"origin": "reviewed"},
        {"record_key": "forged"},
        {"note": None},
    ):
        record = deepcopy(definition())
        record.update(change)
        with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
            shard(file("translations/templates/definitions/001.yaml", [record])[2])


def test_symlink_input_cannot_read_outside_the_declared_tree(tmp_path: Path) -> None:
    root = tmp_path / "tree"
    root.mkdir()
    outside = tmp_path / "foreign"
    outside.mkdir()
    (root / "translations").symlink_to(outside, target_is_directory=True)
    with pytest.raises(ValueError, match="Symlink authored"):
        read_inputs(root)


def override(identifier: JsonValue) -> dict[str, JsonValue]:
    return {
        "kind": "translation_override",
        "data": {
            "context_key": {"source_unit_id": "t:ja:fixture", "variant": "default"},
            "lang": "zh-Hant",
            "action": "pin",
            "templates": [
                {"template_id": identifier, "lang": "zh-Hant", "variant_key": "default"}
            ],
            "terms": [],
            "reason": "自編案例指定既有可重用譯文",
        },
    }


def test_pin_selects_an_actual_reusable_value_and_checks_its_language() -> None:
    files = valid_files()
    identifier = object_value(definition()["data"])["id"]
    selection = override(identifier)
    extra = file("translations/overrides/fixture/001.yaml", [selection])
    assert len(from_files((*files, extra)).records) == 3
    with pytest.raises(ValueError, match="existing reusable template"):
        from_files((files[0], extra))
    object_value(selection["data"])["lang"] = "en"
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        from_files(
            (*files, file("translations/overrides/fixture/001.yaml", [selection]))
        )


def test_named_target_variant_uses_the_contract_code_grammar() -> None:
    identifier = object_value(definition()["data"])["id"]
    variant = target(identifier)
    variant["kind"] = "template_translation_variant"
    object_value(variant["data"])["variant_key"] = "display.formal-v1"
    selected = override(identifier)
    data = object_value(selected["data"])
    templates = data["templates"]
    assert isinstance(templates, list)
    object_value(templates[0])["variant_key"] = "display.formal-v1"
    records = from_files(
        (
            *valid_files(),
            file("translations/templates/values/002.yaml", [variant]),
            file("translations/overrides/fixture/001.yaml", [selected]),
        )
    ).records
    assert len(records) == 4


@pytest.mark.parametrize("action", ["suppress", "default"])
def test_nonpin_override_cannot_hide_a_selection(action: str) -> None:
    selection = override(object_value(definition()["data"])["id"])
    object_value(selection["data"])["action"] = action
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        from_files(
            (
                *valid_files(),
                file("translations/overrides/fixture/001.yaml", [selection]),
            )
        )
    object_value(selection["data"])["templates"] = []
    assert (
        len(
            from_files(
                (
                    *valid_files(),
                    file("translations/overrides/fixture/001.yaml", [selection]),
                )
            ).records
        )
        == 3
    )


def term_choice(value: JsonValue) -> list[JsonValue]:
    return [
        {
            "kind": "glossary_term",
            "data": {
                "id": "term:fixture.keyword",
                "category": "keyword",
                "concept_key": "fixture.keyword",
                "source_ref": None,
                "source_span": None,
                "authored_source_ja": "自編語",
                "missing_source_reason": "合成測試不引用官方卡文",
            },
        },
        {
            "kind": "glossary_choice",
            "data": {
                "term_id": "term:fixture.keyword",
                "lang": "zh-Hant",
                "value": value,
                "concept_evidence": [],
            },
        },
    ]


@pytest.mark.parametrize("value", [None, {"kind": "authored", "text": "自編詞"}])
def test_term_pin_rejects_a_withdrawn_choice(value: JsonValue) -> None:
    choice = term_choice(value)
    selection = override(object_value(definition()["data"])["id"])
    data = object_value(selection["data"])
    data["templates"] = []
    data["terms"] = [
        {"term_id": "term:fixture.keyword", "lang": "zh-Hant", "variant_key": "default"}
    ]
    files = (
        file("translations/glossary/fixture/001.yaml", choice),
        file("translations/overrides/fixture/001.yaml", [selection]),
    )
    if value is None:
        with pytest.raises(ValueError, match="existing usable glossary"):
            from_files(files)
    else:
        assert len(from_files(files).records) == 3


def test_manual_match_requires_its_frame_role_and_complete_values() -> None:
    identifier = object_value(definition()["data"])["id"]
    match: dict[str, JsonValue] = {
        "frame_id": identifier,
        "source_span": {
            "role": "body",
            "segments": [{"start": 0, "end": 2}],
            "anchor": None,
        },
        "values": {"n": 2},
    }
    record: dict[str, JsonValue] = {
        "kind": "template_match",
        "data": {
            "context_key": {"source_unit_id": "t:ja:fixture", "variant": "default"},
            "source_hash": digest("仮２".encode()),
            "matches": [match],
        },
    }
    files = valid_files()
    assert (
        len(
            from_files(
                (*files, file("translations/overrides/fixture/001.yaml", [record]))
            ).records
        )
        == 3
    )
    match["values"] = {}
    with pytest.raises(ValueError, match="missing or unknown leaf"):
        from_files((*files, file("translations/overrides/fixture/001.yaml", [record])))
    match["values"] = {"n": 2}
    object_value(match["source_span"])["role"] = "layout"
    with pytest.raises(ValueError, match="exact frame role"):
        from_files((*files, file("translations/overrides/fixture/001.yaml", [record])))


def test_form_references_resolve_the_same_language_and_registered_signature() -> None:
    files = valid_files()
    identifier = object_value(definition()["data"])["id"]
    selected = target(identifier)
    object_value(selected["data"])["target"] = {
        "format": 1,
        "nodes": [
            {
                "kind": "Form",
                "form_id": "keyword.display",
                "args": {"keyword": {"slot": "n"}},
            }
        ],
    }
    with pytest.raises(ValueError, match="form in its own language"):
        from_files(
            (files[0], file("translations/templates/values/001.yaml", [selected]))
        )
    form: dict[str, JsonValue] = {
        "kind": "translation_form",
        "data": {
            "id": "keyword.display",
            "lang": "zh-Hant",
            "rule": "unregistered.rule.v1",
            "signature": [{"name": "keyword", "type": "Concept", "role": "keyword"}],
            "cases": {"default": [{"kind": "Label", "arg": "keyword"}]},
        },
    }
    with pytest.raises(ValueError, match="Unknown four-layer form rule"):
        from_files((*files, file("translations/forms/001.yaml", [form])))

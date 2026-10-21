"""Source, port and writer shapes do not establish review or execution admission."""

import json

import pytest
from pydantic import ConfigDict, JsonValue, RootModel, TypeAdapter, ValidationError

from sve_carddb.contracts import author_evidence
from sve_carddb.contracts.author_evidence import (
    ImportMap,
    Meta,
    Ports,
    SourceUse,
    WrittenBy,
)
from sve_carddb.contracts.author_queries import NumericExpr
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_validation import Violation, inline_parameters
from sve_carddb.domains.registry import records

from .author_entry_cases import meta, review, source_use


@pytest.mark.parametrize(
    "value",
    [
        {"imports": {}, "exports": {}},
        {"imports": {"input": {"path": [], "bind": "target"}}, "exports": {}},
        {
            "imports": {},
            "exports": {"out": {"path": ["spell", "do", 0], "bind": "result"}},
        },
    ],
)
def test_ports_preserve_paths(value: dict[str, JsonValue]) -> None:
    decoded = Ports.model_validate_json(canonical(value), strict=True)
    assert decoded.model_dump(mode="json") == value


@pytest.mark.parametrize("part", [-1, True, 0.5, {}, None])
def test_port_path_rejects_non_structural_parts(part: JsonValue) -> None:
    value = {"imports": {}, "exports": {"out": {"path": [part], "bind": "result"}}}
    with pytest.raises(ValidationError):
        Ports.model_validate_json(json.dumps(value), strict=True)


def test_imports_only_declare_at_root() -> None:
    with pytest.raises(ValidationError):
        Ports.model_validate_json(
            '{"imports":{"input":{"path":[0],"bind":"target"}},"exports":{}}'
        )


def test_source_shape_does_not_claim_card_admission() -> None:
    value = source_use()
    occurrence = value["occurrence"]
    assert isinstance(occurrence, dict)
    occurrence["owner"] = {"kind": "qa_version", "qa_version_id": "fixture"}
    occurrence["field"] = "answer"
    decoded = SourceUse.model_validate_json(canonical(value))
    assert decoded.occurrence.owner.kind == "qa_version"


@pytest.mark.parametrize(
    "candidate",
    [
        "f:" + "0123456789abcdef" * 2,
        "f:" + "A" * 32,
        "f:" + "a" * 31,
        "f:" + "a" * 32 + "\n",
        "c:" + "a" * 32,
        "f:" + "g" * 32,
    ],
)
def test_author_face_id_accepts_exactly_registry_face_ids(candidate: str) -> None:
    def accepts(adapter: TypeAdapter[str]) -> bool:
        try:
            adapter.validate_python(candidate, strict=True)
        except ValidationError:
            return False
        return True

    config = ConfigDict(regex_engine="python-re")
    assert accepts(TypeAdapter(author_evidence.FaceId, config=config)) == accepts(
        TypeAdapter(records.FaceId, config=config)
    )


def test_nonempty_exam_and_nullable_rekey_are_not_supported_shapes() -> None:
    value = meta()
    value["verified_by_exam"] = [{"pass": True}]
    with pytest.raises(ValidationError):
        Meta.model_validate_json(canonical(value))
    item = review()
    item["rekey"] = None
    value = meta()
    value["reviewed_by"] = [item]
    with pytest.raises(ValidationError):
        Meta.model_validate_json(canonical(value))


def test_inline_visitor_preserves_alias_and_root_paths() -> None:
    adapter: TypeAdapter[NumericExpr] = TypeAdapter(
        NumericExpr, config=ConfigDict(regex_engine="python-re")
    )
    expression = adapter.validate_json('{"max":[{"minus":[1,{"param":"offset"}]}]}')
    wrapped = RootModel[object](expression)
    assert list(inline_parameters(wrapped, ("define", "X"))) == [
        Violation("inline_param_reference", ("define", "X", "max", 0, "minus", 1))
    ]
    assert list(inline_parameters({"param": "not a typed reference"}, ())) == []


@pytest.mark.parametrize(
    "date", ["2026-02-29", "0000-01-01", "2026-10-21\n", "2026-1-01"]
)
def test_writer_date_is_real_exact_calendar_date(date: str) -> None:
    with pytest.raises(ValidationError):
        WrittenBy.model_validate_json(
            json.dumps(
                {
                    "actor": {"who": "fixture", "model": None},
                    "date": date,
                }
            )
        )


def test_leap_date_and_author_order_are_preserved() -> None:
    value = meta()
    writers: list[JsonValue] = [
        {"actor": {"who": "z", "model": None}, "date": "2024-02-29"},
        {"actor": {"who": "a", "model": None}, "date": "2024-02-29"},
    ]
    value["written_by"] = writers
    value["rulings"] = ["R-0002", "R-0001", "R-0002"]
    decoded = Meta.model_validate_json(canonical(value))
    assert decoded.model_dump(mode="json") == value


def test_optional_ability_import_map_is_nonempty() -> None:
    adapter = TypeAdapter(ImportMap, config=ConfigDict(regex_engine="python-re"))
    with pytest.raises(ValidationError):
        adapter.validate_json("{}")
    assert (
        adapter.validate_json(
            '{"input":{"binding_id":"fixture","ability_scope":"ability_0","port":"out"}}'
        )["input"].port
        == "out"
    )

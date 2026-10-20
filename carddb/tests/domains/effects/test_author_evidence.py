"""Local evidence invariants never imply source or execution admission."""

import json
from copy import deepcopy

import pytest
from pydantic import ConfigDict, JsonValue, RootModel, TypeAdapter, ValidationError

from sve_carddb.contracts import author_evidence
from sve_carddb.contracts.author_evidence import (
    Domain,
    ImportMap,
    InlineDependencies,
    MacroDependencies,
    Meta,
    Ports,
    SourceUse,
    WrittenBy,
)
from sve_carddb.contracts.author_queries import NumericExpr
from sve_carddb.core.json import canonical
from sve_carddb.domains.effects.author_validation import (
    Violation,
    evidence_violations,
    inline_parameters,
)
from sve_carddb.domains.registry import records

from .author_entry_cases import dependencies, meta, review, source_use
from .author_evidence_cases import EvidenceCase, cases

CASES = cases()


@pytest.mark.parametrize("case", CASES, ids=lambda case: case.name)
def test_each_evidence_variant_roundtrips(case: EvidenceCase) -> None:
    decoded = case.adapter.validate_json(canonical(case.value), strict=True)
    assert case.adapter.dump_python(decoded, mode="json", by_alias=True) == case.value


@pytest.mark.parametrize("case", CASES, ids=lambda case: case.name)
def test_each_evidence_variant_rejects_unknown_key(case: EvidenceCase) -> None:
    with pytest.raises(ValidationError):
        case.adapter.validate_json(
            canonical(case.value | {"typo": "rejected"}), strict=True
        )


@pytest.mark.parametrize(
    ("case", "field"),
    [(case, field) for case in CASES for field in case.required],
    ids=[f"{case.name}-{field}" for case in CASES for field in case.required],
)
def test_evidence_required_field_and_type(case: EvidenceCase, field: str) -> None:
    value = deepcopy(case.value)
    del value[field]
    with pytest.raises(ValidationError):
        case.adapter.validate_json(canonical(value), strict=True)
    value[field] = 0
    with pytest.raises(ValidationError):
        case.adapter.validate_json(canonical(value), strict=True)


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


@pytest.mark.parametrize("regions", [["jp", "en"], ["jp", "jp"]])
def test_domain_requires_original_sorted_unique_order(regions: list[str]) -> None:
    value = Domain.model_validate_json(json.dumps({"regions": regions}))
    assert list(evidence_violations(value)) == [
        Violation("unordered_or_duplicate_regions", ("regions",))
    ]


def test_model_review_identity_ignores_display_name() -> None:
    first = review()
    first["reviewer"] = {"who": "first", "model": "Codex gpt-6-astra"}
    second = deepcopy(first)
    second["reviewer"] = {"who": "second", "model": "Codex gpt-6-astra"}
    value = meta()
    value["reviewed_by"] = [first, second]
    assert list(evidence_violations(Meta.model_validate_json(canonical(value)))) == [
        Violation("duplicate_review", ("reviewed_by", 1))
    ]


def test_review_kind_conflict_does_not_hide_behind_changed_version() -> None:
    value = meta()
    changed = dependencies() | {
        "body_version": "effect-body-v1:sha256:" + "9" * 64,
        "applicability_scope": "fixture",
        "domain_hash": "sha256:" + "4" * 64,
    }
    value["reviewed_by"] = [review(), mechanical(changed)]
    assert list(evidence_violations(Meta.model_validate_json(canonical(value)))) == [
        Violation("review_kind_conflict", ("reviewed_by", 1))
    ]


def mechanical(use_dependencies: dict[str, JsonValue]) -> dict[str, JsonValue]:
    return {
        "kind": "macro_mechanical",
        "body": {
            "frame_id": "frame:" + "1" * 64,
            "ability_scope": "ability_0",
            "body_version": "effect-body-v1:sha256:" + "9" * 64,
            "applicability_scope": "fixture",
        },
        "use_dependencies": use_dependencies,
    }


def test_macro_mechanical_rejects_inline_use_dependencies() -> None:
    value = meta()
    value["reviewed_by"] = [mechanical(dependencies())]
    with pytest.raises(ValidationError):
        Meta.model_validate_json(canonical(value))


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


def test_disputed_review_needs_reason() -> None:
    item = review()
    item["verdict"] = "disputed"
    value = meta()
    value["reviewed_by"] = [item]
    assert list(evidence_violations(Meta.model_validate_json(canonical(value)))) == [
        Violation("disputed_reason_required", ("reviewed_by", 0, "reason"))
    ]


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
    assert list(evidence_violations(decoded)) == []
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


@pytest.mark.parametrize("field", ["applicability_scope", "domain_hash"])
def test_inline_and_macro_dependency_domains_cannot_be_mixed(field: str) -> None:
    value = dependencies()
    value[field] = "fixture" if field == "applicability_scope" else "sha256:" + "0" * 64
    for adapter in (InlineDependencies, MacroDependencies):
        with pytest.raises(ValidationError):
            adapter.model_validate_json(canonical(value))


@pytest.mark.parametrize("field", ["interfaces", "rulings"])
def test_dependencies_require_canonical_byte_order(field: str) -> None:
    value = dependencies()
    if field == "interfaces":
        values: list[JsonValue] = [
            {"frame_id": "frame:" + digit * 64, "interface_key": "0" * 64}
            for digit in ("2", "1")
        ]
    else:
        values = [{"id": "second", "revision": 1}, {"id": "first", "revision": 1}]
    value[field] = values
    decoded = InlineDependencies.model_validate_json(canonical(value))
    assert list(evidence_violations(decoded)) == [
        Violation("unordered_or_duplicate_dependencies", (field,))
    ]
    value[field] = sorted(values, key=canonical)
    assert not list(
        evidence_violations(InlineDependencies.model_validate_json(canonical(value)))
    )
    value[field] = [values[0], values[0]]
    assert list(
        evidence_violations(InlineDependencies.model_validate_json(canonical(value)))
    )


def test_rekey_after_cannot_duplicate_another_review() -> None:
    old = dependencies()
    old["source"] = "effect-source-v1:sha256:" + "9" * 64
    first = review()
    first["dependencies"] = old
    first["rekey"] = {
        "original": {"input_hash": "sha256:" + "0" * 64, "record_key": "fixture"},
        "before": old,
        "after": dependencies(),
        "mapping_recipe": {"id": "fixture", "hash": "sha256:" + "1" * 64},
    }
    value = meta()
    value["reviewed_by"] = [first, review()]
    assert list(evidence_violations(Meta.model_validate_json(canonical(value)))) == [
        Violation("duplicate_review", ("reviewed_by", 1))
    ]


def test_dependency_imports_reject_embedded_upstream_snapshot() -> None:
    value = dependencies()
    value["imports"] = {
        "input": {
            "binding_id": "fixture",
            "ability_scope": "ability_0",
            "port": "out",
            "dependency_key": "sha256:" + "0" * 64,
            "dependencies": dependencies(),
        }
    }
    with pytest.raises(ValidationError):
        InlineDependencies.model_validate_json(canonical(value))

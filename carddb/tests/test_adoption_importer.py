"""End-to-end signed synthetic receipts and independent projection counterexamples."""

import copy
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import CompiledSchema, create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_importer import import_adoptions
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, object_value

from .adoption_fixtures import (
    Case,
    commit,
    dependency,
    envelope,
    fields,
    index,
    make_case,
    successor,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("adoption-build") / "repository")


@pytest.fixture
def case(tmp_path: Path, baseline: Case) -> Case:
    repository = tmp_path / "repository"
    shutil.copytree(baseline.repository, repository)
    return replace(
        baseline,
        repository=repository,
        root=repository / "authored",
        review=copy.deepcopy(baseline.review),
        normalizer=copy.deepcopy(baseline.normalizer),
    )


def test_new_adoptions_enter_one_build_with_actual_decisions_and_f1(
    case: Case, schema: CompiledSchema
) -> None:
    with create_database(schema) as db:
        inputs = import_adoptions(db, case.inputs(), build=case.build(), stores={})
        assert not inputs.uses
        assert len(db.rows("language")) == 3
        assert len(db.rows("vocabulary")) == 1
        assert len(db.rows("search_alias")) == 1
        assert len(db.rows("text_symbol")) == 1
        assert len(db.rows("decision")) == 4
        assert len(db.rows("decision_source")) == 4
        assert len(db.rows("source_record")) == 5
        assert db.rows("search_alias")[0].values["normalized"] == "synthetic "
        assert db.rows("text_symbol")[0].values["decision_id"] in {
            r.values["id"] for r in db.rows("decision")
        }
        import_adoptions(db, case.inputs(), build=case.build(), stores={})
        assert len(db.rows("source_record")) == 5


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("normalized", "cannot be reproduced"),
        ("normalizer_config", "config hash mismatch"),
        ("normalizer_hash", "program/config hash mismatch"),
        ("normalizer_path", "Pinned immutable dependency unavailable"),
        ("dependency_missing", "direct dependency closure"),
        ("dependency_spoof", "direct dependency closure"),
        ("target_missing", "verified effective projection"),
        ("unknown_lang", "verified effective projection"),
        ("ja_zh", "Traditional Chinese"),
        ("zh_order", "approved UI fallback order"),
        ("fallback_duplicate", "invalid fallback closure"),
        ("fallback_self", "invalid fallback closure"),
        ("language_withdrawn", "Initial adoption requires value"),
        ("symbol_parameter", "undeclared parameter"),
        ("symbol_lang", "localization language mismatch"),
    ],
)
def test_semantic_failures_roll_back_all_audit_and_rows(  # ruff: ignore[complex-structure,too-many-branches] -- each branch isolates one signed semantic constraint
    case: Case, mutation: str, message: str, schema: CompiledSchema
) -> None:
    area = (
        "aliases"
        if mutation
        in {
            "normalized",
            "normalizer_config",
            "normalizer_hash",
            "normalizer_path",
            "dependency_missing",
            "dependency_spoof",
            "target_missing",
            "unknown_lang",
        }
        else "symbols"
        if mutation.startswith("symbol_")
        else "languages"
    )
    name = f"catalog-adoptions/{area}/shared/001.yaml"
    raw = object_value(read_yaml(case.root / name))
    members = array(raw["records"])
    item, data, _ = fields(raw)
    if mutation in {"ja_zh", "fallback_self", "fallback_duplicate"}:
        item = next(
            object_value(r)
            for r in members
            if object_value(object_value(r)["data"])["subject"] == {"code": "ja"}
        )
        data = object_value(item["data"])
        fallback = (
            ["zh-Hant"]
            if mutation == "ja_zh"
            else ["ja"]
            if mutation == "fallback_self"
            else ["en", "en"]
        )
        object_value(data["value"])["fallback_order"] = list(fallback)
        data["dependencies"] = sorted(
            [dependency("language", code=lang) for lang in set(fallback)],
            key=canonical,
        )
    elif mutation == "zh_order":
        item = next(
            object_value(r)
            for r in members
            if object_value(object_value(r)["data"])["subject"] == {"code": "zh-Hant"}
        )
        object_value(object_value(item["data"])["value"])["fallback_order"] = [
            "en",
            "ja",
        ]
    elif mutation == "language_withdrawn":
        data["value"] = None
        data["dependencies"] = []
    elif mutation == "normalized":
        object_value(data["value"])["normalized"] = "trimmed"
    elif mutation.startswith("normalizer_"):
        pin = object_value(object_value(data["value"])["normalizer"])
        if mutation == "normalizer_config":
            pin["config"] = {"unicode_version": "wrong"}
        elif mutation == "normalizer_hash":
            pin["code_hash"] = "sha256:" + "e" * 64
        else:
            pin["code_path"] = "carddb/src/sve_carddb/routes/other.py"
    elif mutation in {"dependency_missing", "dependency_spoof"}:
        data["dependencies"] = (
            []
            if mutation == "dependency_missing"
            else [dependency("card", id="follower")]
        )
    elif mutation in {"target_missing", "unknown_lang"}:
        subject = object_value(data["subject"])
        subject["code" if mutation == "target_missing" else "lang"] = (
            "missing" if mutation == "target_missing" else "fr"
        )
        item["record_key"] = canonical(["search_alias_adoption", subject, 1]).decode()
        data["dependencies"] = sorted(
            [
                dependency("vocabulary", kind="type", code=str(subject["code"])),
                dependency("language", code=str(subject["lang"])),
            ],
            key=canonical,
        )
    else:
        loc = object_value(object_value(data["value"])["source_localization"])
        object_value(loc["tooltip"])["text"] = (
            "{missing}" if mutation == "symbol_parameter" else "Synthetic"
        )
        if mutation == "symbol_lang":
            object_value(loc["tooltip"])["lang"] = "en"
    write(case.root, name, envelope(members, case.review))
    index(case.root)
    revised = replace(case, revision=commit(case.repository))
    with create_database(schema) as db:
        with pytest.raises(ValueError, match=message):
            import_adoptions(db, revised.inputs(), build=revised.build(), stores={})
        for table in (
            "decision",
            "source_record",
            "language",
            "vocabulary",
            "search_alias",
            "text_symbol",
        ):
            assert not db.rows(table)


@pytest.mark.parametrize("mutation", ["configuration", "uncommitted"])
def test_authored_and_build_pins_are_mandatory(
    case: Case, mutation: str, schema: CompiledSchema
) -> None:
    build = case.build()
    if mutation == "configuration":
        build = BuildContext.from_inputs(
            build.program_revision, {"synthetic.lock": b"lock"}, {}
        )
    else:
        path = case.root / "catalog-adoptions/symbols/shared/001.yaml"
        path.write_bytes(path.read_bytes() + b"\n")
    with create_database(schema) as db:
        with pytest.raises(
            ValueError, match=r"configuration|immutable authored revision"
        ):
            import_adoptions(db, case.inputs(), build=build, stores={})
        assert not db.rows("source_record")


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_t0()


def test_withdrawal_retains_permanent_vocab_key_and_removes_alias(
    case: Case, schema: CompiledSchema
) -> None:
    successor(case.root, "vocabulary", None)
    successor(case.root, "aliases", None)
    revised = replace(case, revision=commit(case.repository))
    with create_database(schema) as db:
        import_adoptions(db, revised.inputs(), build=revised.build(), stores={})
        assert not db.rows("search_alias")
        assert len(db.rows("vocabulary")) == 1
        assert db.rows("vocabulary")[0].values["active"] is False
        assert db.rows("vocabulary")[0].values["code"] == "follower"
        assert len(db.rows("decision")) == 6


def test_unrelated_label_revision_does_not_require_alias_resigning(
    case: Case, schema: CompiledSchema
) -> None:
    value = object_value(
        fields(
            object_value(
                read_yaml(case.root / "catalog-adoptions/vocabulary/shared/001.yaml")
            )
        )[1]["value"]
    )
    object_value(value["label"])["text"] = "Synthetic revised label"
    successor(case.root, "vocabulary", value)
    revised = replace(case, revision=commit(case.repository))
    with create_database(schema) as db:
        import_adoptions(db, revised.inputs(), build=revised.build(), stores={})
        assert len(db.rows("search_alias")) == 1
        assert any(
            r.values["text"] == "Synthetic revised label" for r in db.rows("text_unit")
        )


def test_active_reference_blocks_withdrawn_target(
    case: Case, schema: CompiledSchema
) -> None:
    successor(case.root, "vocabulary", None)
    revised = replace(case, revision=commit(case.repository))
    with create_database(schema) as db:
        with pytest.raises(ValueError, match="withdrawn or inactive"):
            import_adoptions(db, revised.inputs(), build=revised.build(), stores={})
        assert not db.rows("language")

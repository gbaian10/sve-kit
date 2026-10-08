"""Native current vocabulary/languages have provenance without review decisions."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_loader import load_adoptions
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.offline import _populate_adoptions, _prepare_catalog
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import commit, make_case
from .current_catalog_fixtures import current_case

if TYPE_CHECKING:
    from pathlib import Path

    from .adoption_fixtures import Case


@pytest.fixture(scope="module")
def current_baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return current_case(
        make_case(tmp_path_factory.mktemp("catalog-current") / "repository")
    )


def test_current_catalog_has_provenance_without_adoption_decisions(
    current_baseline: Case,
) -> None:
    case = current_baseline
    snapshot = load_adoptions(case.root, entry="catalog-adoptions")
    assert len(snapshot.current_records()) == 4
    with create_database(compile_current_build()) as db:
        with db.transaction():
            _populate_adoptions(
                db,
                case.inputs(),
                build=case.build(),
                stores={},
                prepared=_prepare_catalog(case.inputs(), case.build(), {}),
                sources=Sources({}, case.inputs().repository, case.build()),
            )
        assert len(db.rows("vocabulary")) == 1
        assert len(db.rows("language")) == 3
        assert not db.rows("decision")
        for table in ("vocabulary", "language"):
            assert all(
                r.values["authored_source_id"] is not None for r in db.rows(table)
            )
            assert all(r.values["low_confidence"] is False for r in db.rows(table))


def vocabulary_with_label(
    base: Case, tmp_path: Path, label: dict[str, JsonValue]
) -> Case:
    root = tmp_path / "repository"
    shutil.copytree(base.repository, root)
    case = replace(base, repository=root, root=root / "authored")
    path = "catalog-adoptions/vocabulary/shared/001.yaml"
    raw = object_value(read_yaml(case.root / path))
    row = next(
        object_value(r)
        for r in array(raw["records"])
        if object_value(r)["kind"] == "vocabulary_adoption"
    )
    object_value(object_value(row["data"])["value"])["translations"] = [label]
    (case.root / path).write_bytes(canonical(raw))
    index_path = case.root / "catalog-adoptions/index.yaml"
    index = object_value(read_yaml(index_path))
    object_value(index["includes"])[path] = digest(canonical(raw))
    index_path.write_bytes(canonical(index))
    return replace(case, revision=commit(root))


def test_vocabulary_label_translation_is_a_use_of_the_japanese_label(
    current_baseline: Case, tmp_path: Path
) -> None:
    case = vocabulary_with_label(
        current_baseline,
        tmp_path,
        {
            "lang": "zh-Hant",
            "text": "從者",
            "origin": "machine",
            "low_confidence": True,
        },
    )
    schema = compile_current_build(("t0", "translation_evidence", "translation_names"))
    with create_database(schema) as db:
        with db.transaction():
            _populate_adoptions(
                db,
                case.inputs(),
                build=case.build(),
                stores={},
                prepared=_prepare_catalog(case.inputs(), case.build(), {}),
                sources=Sources({}, case.inputs().repository, case.build()),
            )
        vocabulary = db.rows("vocabulary")[0].values
        use = db.rows("translation_use")[0].values
        assert (use["vocabulary_kind"], use["vocabulary_code"], use["field"]) == (
            vocabulary["kind"],
            vocabulary["code"],
            "label",
        )
        context = db.rows("translation_context")[0].values
        assert context["source_unit_id"] == vocabulary["label_unit_id"]
        translation = db.rows("translation")[0].values
        assert translation["text"] == "從者"
        assert (translation["origin"], translation["low_confidence"]) == (
            "machine",
            True,
        )


@pytest.mark.parametrize("lang", ["ja"])
def test_label_translation_cannot_replace_the_japanese_base(
    current_baseline: Case, tmp_path: Path, lang: str
) -> None:
    case = vocabulary_with_label(
        current_baseline,
        tmp_path,
        {"lang": lang, "text": "x", "origin": "machine", "low_confidence": False},
    )
    with pytest.raises(ValueError, match="Invalid"):
        load_adoptions(case.root, entry="catalog-adoptions").current_records()


def test_current_fallback_is_still_checked(
    current_baseline: Case, tmp_path: Path
) -> None:
    root = tmp_path / "repository"
    shutil.copytree(current_baseline.repository, root)
    case = replace(current_baseline, repository=root, root=root / "authored")
    path = "catalog-adoptions/languages/shared/001.yaml"
    raw = object_value(read_yaml(case.root / path))
    row = next(
        object_value(r)
        for r in array(raw["records"])
        if object_value(object_value(r)["data"])["subject"] == {"code": "ja"}
    )
    object_value(object_value(row["data"])["value"])["fallback_order"] = ["zh-Hant"]
    (case.root / path).write_bytes(canonical(raw))
    index_path = case.root / "catalog-adoptions/index.yaml"
    index = object_value(read_yaml(index_path))
    object_value(index["includes"])[path] = digest(canonical(raw))
    index_path.write_bytes(canonical(index))
    case = replace(case, revision=commit(root))
    with (
        create_database(compile_current_build()) as db,
        pytest.raises(
            ValueError,
            match=r"^Japanese/English UI cannot fall back to Traditional Chinese$",
        ),
    ):
        with db.transaction():
            _populate_adoptions(
                db,
                case.inputs(),
                build=case.build(),
                stores={},
                prepared=_prepare_catalog(case.inputs(), case.build(), {}),
                sources=Sources({}, case.inputs().repository, case.build()),
            )


@pytest.mark.parametrize("mutation", ["empty_entries", "duplicate_entries"])
def test_current_inputs_require_explicit_immutable_pins(
    current_baseline: Case, mutation: str
) -> None:
    inputs = current_baseline.inputs()
    if mutation == "short_sha":
        inputs = replace(inputs, authored_revision=inputs.authored_revision[:8])
        message = "^Adoption authored revision must be a full Git SHA$"
    else:
        inputs = replace(
            inputs, entries=() if mutation == "empty_entries" else inputs.entries * 2
        )
        message = "^Adoption entries must be explicitly enabled, sorted and unique$"
    with pytest.raises(ValueError, match=message):
        inputs.load()


@pytest.mark.parametrize("mutation", ["configuration"])
def test_native_current_entry_rejects_unpinned_catalog(
    current_baseline: Case, tmp_path: Path, mutation: str
) -> None:
    root = tmp_path / "repository"
    shutil.copytree(current_baseline.repository, root)
    case = replace(current_baseline, repository=root, root=root / "authored")
    build = case.build()
    if mutation == "configuration":
        build = BuildContext.from_inputs(build.program_revision, {})
        message = "^Build configuration does not pin adoption inputs$"
    else:
        path = case.root / "catalog-adoptions/languages/shared/001.yaml"
        path.write_bytes(path.read_bytes() + b"\n")
        message = "^Adoption bytes differ from immutable authored revision$"
    with create_database(compile_current_build()) as db:
        with pytest.raises(ValueError, match=message), db.transaction():
            _populate_adoptions(
                db,
                case.inputs(),
                build=build,
                stores={},
                prepared=_prepare_catalog(case.inputs(), build, {}),
                sources=Sources({}, case.inputs().repository, build),
            )
        assert not db.rows("source_record")
        assert not db.rows("language")
        assert not db.rows("vocabulary")


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("self", "Adopted language has invalid fallback closure"),
        ("duplicate", "Adopted language has invalid fallback closure"),
        ("unregistered", "Adopted language has invalid fallback closure"),
        ("ja_policy", "Adopted language violates approved UI fallback order"),
        ("zh_policy", "Adopted language violates approved UI fallback order"),
    ],
)
def test_current_ui_fallback_closure_and_policy(
    current_baseline: Case, tmp_path: Path, mutation: str, message: str
) -> None:
    root = tmp_path / "repository"
    shutil.copytree(current_baseline.repository, root)
    case = replace(current_baseline, repository=root, root=root / "authored")
    path = "catalog-adoptions/languages/shared/001.yaml"
    payload = object_value(read_yaml(case.root / path))
    rows = array(payload["records"])
    if mutation == "unregistered":
        payload["records"] = [
            row
            for row in rows
            if object_value(object_value(object_value(row)["data"])["subject"])["code"]
            != "en"
        ]
    else:
        code = "zh-Hant" if mutation == "zh_policy" else "ja"
        row = next(
            object_value(row)
            for row in rows
            if object_value(object_value(object_value(row)["data"])["subject"])["code"]
            == code
        )
        object_value(object_value(row["data"])["value"])["fallback_order"] = list[
            JsonValue
        ](
            {
                "self": ["ja", "en"],
                "duplicate": ["en", "en"],
                "ja_policy": [],
                "zh_policy": ["en", "ja"],
            }[mutation]
        )
    (case.root / path).write_bytes(canonical(payload))
    index_path = case.root / "catalog-adoptions/index.yaml"
    index = object_value(read_yaml(index_path))
    object_value(index["includes"])[path] = digest(canonical(payload))
    index_path.write_bytes(canonical(index))
    case = replace(case, revision=commit(root))
    with create_database(compile_current_build()) as db:
        with pytest.raises(ValueError, match="^" + message + "$"), db.transaction():
            _populate_adoptions(
                db,
                case.inputs(),
                build=case.build(),
                stores={},
                prepared=_prepare_catalog(case.inputs(), case.build(), {}),
                sources=Sources({}, case.inputs().repository, case.build()),
            )
        assert not db.rows("source_record")

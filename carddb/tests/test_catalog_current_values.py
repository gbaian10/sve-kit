"""Current vocabulary/languages coexist with unchanged nontranslation adoptions."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.catalog.adoption_importer import import_adoptions
from sve_carddb.catalog.adoption_loader import load_adoptions
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value

from .adoption_fixtures import commit, make_case

if TYPE_CHECKING:
    from pathlib import Path

    from .adoption_fixtures import Case


@pytest.fixture(scope="module")
def current_baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    case = make_case(tmp_path_factory.mktemp("catalog-current") / "repository")
    index_path = case.root / "catalog-adoptions/index.yaml"
    index = object_value(read_yaml(index_path))
    includes = object_value(index["includes"])
    for path in includes:
        if "/vocabulary/" not in path and "/languages/" not in path:
            continue
        raw = object_value(read_yaml(case.root / path))
        rows: list[JsonValue] = []
        for raw_item in array(raw["records"]):
            item = object_value(raw_item)
            data = object_value(item["data"])
            row = {
                "record_key": canonical([item["kind"], data["subject"]]).decode(),
                "kind": item["kind"],
                "data": {
                    "subject": data["subject"],
                    "value": data["value"],
                    "evidence": item["evidence"],
                },
                "origin": "project",
                "low_confidence": False,
                "note": "",
            }
            rows.append(row)
        payload: dict[str, JsonValue] = {
            "catalog_adoption_format": 2,
            "kind": "catalog_adoption_shard",
            "records": rows,
        }
        (case.root / path).write_bytes(canonical(payload))
        includes[path] = digest(canonical(payload))
    index["catalog_adoption_format"] = 2
    index_path.write_bytes(canonical(index))
    return replace(case, revision=commit(case.repository))


def test_current_catalog_preserves_other_adoption_decisions(
    current_baseline: Case,
) -> None:
    case = current_baseline
    snapshot = load_adoptions(case.root, entry="catalog-adoptions")
    assert len(snapshot.current_records()) == 4
    with create_database(compile_current_build()) as db:
        import_adoptions(db, case.inputs(), build=case.build(), stores={})
        assert len(db.rows("vocabulary")) == 1
        assert len(db.rows("language")) == 3
        assert {r.values["category"] for r in db.rows("decision")} == {
            "text_symbol_adoption",
            "search_alias_adoption",
        }
        for table in ("vocabulary", "language"):
            assert all(
                r.values["authored_source_id"] is not None for r in db.rows(table)
            )
            assert all(r.values["low_confidence"] is False for r in db.rows(table))


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
        import_adoptions(db, case.inputs(), build=case.build(), stores={})

"""Family labels validate the shared authored closure before writing public translations."""

from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import canonical
from sve_carddb.domains.translations.four_layer_authored import read_inputs
from sve_carddb.domains.translations.labels import apply

from ...support.build_db_fixtures import seed

if TYPE_CHECKING:
    from pathlib import Path


def authored(root: Path, records: list[dict[str, JsonValue]]) -> None:
    path = root / "translations/labels/products/001.yaml"
    path.parent.mkdir(parents=True)
    path.write_bytes(
        canonical(
            {
                "format": 3,
                "kind": "translation_shard",
                "records": list[JsonValue](records),
            }
        )
    )


def label(code: str = "TEST") -> dict[str, JsonValue]:
    return {
        "kind": "product_family_label",
        "origin": "project",
        "low_confidence": False,
        "data": {"public_code": code, "lang": "zh-Hant", "text": "試作卡包"},
    }


@pytest.mark.parametrize("field", ["public_code", "lang", "text"])
def test_missing_label_field_is_rejected(tmp_path: Path, field: str) -> None:
    record = label()
    data = record["data"]
    assert isinstance(data, dict)
    del data[field]
    authored(tmp_path, [record])
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        read_inputs(tmp_path)


@pytest.mark.parametrize("cross_shard", [False, True])
def test_duplicate_public_code_and_language_is_rejected(
    tmp_path: Path, *, cross_shard: bool
) -> None:
    authored(tmp_path, [label()] if cross_shard else [label(), label()])
    if cross_shard:
        path = tmp_path / "translations/labels/products/002.yaml"
        path.write_bytes((path.with_name("001.yaml")).read_bytes())
    with pytest.raises(ValueError, match="Duplicate four-layer authored selection key"):
        read_inputs(tmp_path)


@pytest.mark.parametrize("value", ["", "   ", " 試作"])
def test_blank_or_padded_label_is_rejected(tmp_path: Path, value: str) -> None:
    record = label()
    data = record["data"]
    assert isinstance(data, dict)
    data["text"] = value
    authored(tmp_path, [record])
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        read_inputs(tmp_path)


def test_family_label_cannot_claim_official_authority(tmp_path: Path) -> None:
    record = label()
    record["origin"] = "official"
    authored(tmp_path, [record])
    with pytest.raises(ValueError, match="Invalid four-layer authored shard"):
        read_inputs(tmp_path)


def test_unknown_family_is_rejected_before_any_label_is_written(tmp_path: Path) -> None:
    authored(tmp_path, [label(), label("ABSENT")])
    inputs = read_inputs(tmp_path)
    with create_database(
        compile_build(("t0", "translation_evidence", "translation_names"))
    ) as db:
        seed(db)
        with (
            db.transaction(),
            pytest.raises(ValueError, match="Unknown family label public_code: ABSENT"),
        ):
            apply(db, inputs)
        assert db.rows("translation_use") == ()


def test_family_label_populates_translation_tables(tmp_path: Path) -> None:
    authored(tmp_path, [label()])
    with create_database(
        compile_build(("t0", "translation_evidence", "translation_names"))
    ) as db:
        seed(db)
        with db.transaction():
            apply(db, read_inputs(tmp_path))
        assert db.rows("translation_use")[0].values["product_family_id"] == "family"
        assert db.rows("translation_use")[0].values["field"] == "label"
        assert db.rows("translation")[0].values["text"] == "試作卡包"
        assert db.rows("translation")[0].values["authority"] == "unofficial"
        assert len(db.rows("translation_selection")) == 1


def test_families_with_the_same_original_keep_distinct_labels(tmp_path: Path) -> None:
    other = label("OTHER")
    data = other["data"]
    assert isinstance(data, dict)
    data["text"] = "另一卡包"
    authored(tmp_path, [label(), other])
    with create_database(
        compile_build(("t0", "translation_evidence", "translation_names"))
    ) as db:
        seed(db)
        with db.transaction():
            db.insert(
                "product_family",
                dict(db.rows("product_family")[0].values)
                | {
                    "id": "other-family",
                    "code": "other",
                    "public_code": "OTHER",
                },
            )
            apply(db, read_inputs(tmp_path))
        assert len(db.rows("translation_context")) == 2
        assert {row.values["text"] for row in db.rows("translation")} == {
            "試作卡包",
            "另一卡包",
        }

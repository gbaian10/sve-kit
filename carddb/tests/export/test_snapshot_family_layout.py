"""Family catalog boundaries and finite regional reconstruction through the public reader."""

from copy import deepcopy
from typing import TYPE_CHECKING

import pytest
from jsonschema import ValidationError
from pydantic import JsonValue

from sve_carddb.core.json import array, object_value, parse
from sve_carddb.export.reader import read_snapshot

if TYPE_CHECKING:
    from sve_carddb.export.transport import Snapshot

from ..support.snapshot_contract_fixtures import _reseal
from .test_snapshot_export import exported as exported  # ruff: ignore[useless-import-alias] -- one independently verified synthetic projection
from .test_snapshot_export import logical as logical  # ruff: ignore[useless-import-alias] -- shared projection fixture


def mutable(snapshot: Snapshot) -> tuple[dict[str, JsonValue], dict[str, JsonValue]]:
    return deepcopy(snapshot.manifest), {
        key: parse(blob.raw) for key, blob in snapshot.payloads.items()
    }


def test_catalog_contains_all_printing_locations_and_preserves_unknown_region(
    exported: Snapshot,
) -> None:
    view = read_snapshot(
        exported.manifest, {key: blob.raw for key, blob in exported.payloads.items()}
    )
    files = [object_value(raw) for raw in array(exported.manifest["files"])]
    assert any(
        file["purpose"] == "region-state" and file["region"] == "en" for file in files
    )
    assert not any(
        file["purpose"] == "catalog" and file["region"] == "en" for file in files
    )
    assert {
        object_value(row)["region"] for row in array(view["card"][0]["regions"])
    } == {"jp", "en"}
    assert {file["purpose"] for file in files} >= {
        "catalog",
        "detail",
        "print-detail",
        "engine",
    }


@pytest.mark.parametrize(
    ("field", "value"), [("purpose", "unknown-purpose"), ("region", "cn")]
)
def test_unknown_file_scope_is_rejected(
    exported: Snapshot, field: str, value: str
) -> None:
    manifest = deepcopy(exported.manifest)
    object_value(array(manifest["files"])[0])[field] = value
    with pytest.raises(ValidationError):
        read_snapshot(
            manifest, {key: blob.raw for key, blob in exported.payloads.items()}
        )


def test_conflicting_regional_fixed_identity_is_rejected(exported: Snapshot) -> None:
    manifest, values = mutable(exported)
    target = next(
        value for key, value in values.items() if key.startswith("region-state/")
    )
    card = array(array(object_value(object_value(target)["tables"])["card"])[0])
    array(array(card[4])[0])[1] = "double_faced"
    with pytest.raises(ValueError, match="Conflicting fixed regional fields"):
        read_snapshot(manifest, _reseal(manifest, values))


def test_dictionary_copy_must_have_identical_complete_value(exported: Snapshot) -> None:
    manifest, values = mutable(exported)
    catalog = next(
        object_value(value)
        for key, value in values.items()
        if key.startswith("catalog/jp/")
    )
    fragment = array(array(object_value(catalog["tables"])["text_unit"])[0])
    array(array(fragment[4])[0])[2] = "different synthetic name"
    with pytest.raises(ValueError, match="Conflicting dictionary copy"):
        read_snapshot(manifest, _reseal(manifest, values))


@pytest.mark.parametrize(
    "mutation", ["duplicate_base", "out_of_range", "duplicate_index", "missing_index"]
)
def test_exact_base_and_complete_row_indices_are_required(
    exported: Snapshot, mutation: str
) -> None:
    manifest, values = mutable(exported)
    detail = next(
        object_value(value)
        for key, value in values.items()
        if key.startswith("print-detail/")
    )
    fragments = array(object_value(detail["tables"])["printing"])
    fragment = array(fragments[0])
    rows = array(fragment[4])
    if mutation == "duplicate_base":
        fragments.append(deepcopy(fragment))
    elif mutation == "out_of_range":
        array(rows[0])[0] = 999
    elif mutation == "duplicate_index":
        rows.append(deepcopy(rows[0]))
    else:
        rows.clear()
    with pytest.raises(
        ValueError,
        match=r"Duplicate fragment identity|row_index|Rows must be sorted with unique keys",
    ):
        read_snapshot(manifest, _reseal(manifest, values))

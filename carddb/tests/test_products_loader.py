"""Counterexamples isolate every catalog constraint."""

import json
import shutil
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue, ValidationError
from ruamel.yaml.error import YAMLError

from sve_carddb.core.json import canonical, digest
from sve_carddb.domains.products import load_products
from sve_carddb.domains.products.models import FamilyRecord
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import read_yaml

from .product_fixtures import (
    envelope,
    family,
    first_record,
    inclusion,
    install,
    obj,
    product,
    reference,
)
from .product_fixtures import product_root as product_root  # ruff: ignore[useless-import-alias] -- shared fixture
from .registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- shared fixture dependency
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared fixture dependency

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.products.loader import ProductSnapshot

NAME = "products/family/BP02/001.yaml"


def load(root: Path) -> ProductSnapshot:
    return load_products(root, registry=load_registry(root))


def shard(root: Path) -> dict[str, JsonValue]:
    return obj(read_yaml(root / NAME))


def test_complete_immutable_snapshot_retains_state_and_bytes(
    product_root: Path,
) -> None:
    before = {p: p.read_bytes() for p in product_root.rglob("*.yaml")}
    snapshot = load(product_root)
    assert len(snapshot.records) == 3
    loaded = next(s for s in snapshot.shards if s.path == NAME)
    assert loaded.content_hash == digest(canonical(json.loads(loaded.content)))
    record = snapshot.records['["product_family","BP02"]']
    assert isinstance(record, FamilyRecord)
    assert record.state == "confirmed"
    assert not record.note
    with pytest.raises(ValidationError, match="frozen"):
        record.data.id = "changed"  # type: ignore[misc]  # invalid runtime mutation
    with pytest.raises(TypeError):
        snapshot.records[record.record_key] = record  # type: ignore[index]  # read-only mapping
    assert before == {p: p.read_bytes() for p in before}
    assert "Synthetic family" not in json.dumps(snapshot.report())


def test_omitted_empty_note_preserves_record_identity(product_root: Path) -> None:
    value = shard(product_root)
    first_record(value)["note"] = ""
    install(product_root, NAME, value)
    before = load(product_root)
    del first_record(value)["note"]
    install(product_root, NAME, value)
    after = load(product_root)
    assert after.records == before.records
    assert after.report() == before.report()


def test_yaml_presentation_does_not_change_records(product_root: Path) -> None:
    path = product_root / NAME
    path.write_text(
        "# Only presentation changed\n" + path.read_text(), encoding="utf-8"
    )
    assert len(load(product_root).records) == 3


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("state", "sampled"),
        ("state", None),
        ("note", None),
        ("reviewed_by", "reviewer"),
        ("decision_id", "d:" + "0" * 64),
    ],
)
def test_each_review_field(product_root: Path, field: str, value: JsonValue) -> None:
    raw = shard(product_root)
    first_record(raw)[field] = value
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


def test_missing_state_is_not_read_as_confirmed(product_root: Path) -> None:
    raw = shard(product_root)
    del first_record(raw)["state"]
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("id", ""),
        ("code", "BP02"),
        ("code", "bp02\n"),
        ("code", 2),
        ("public_code", ""),
        ("kind", "unknown"),
        ("name", {"lang": "ja", "text": ""}),
        ("name", {"lang": "invalid_tag", "text": "Synthetic"}),
        ("name", {"lang": "ja"}),
    ],
)
def test_each_family_data_field(
    product_root: Path, field: str, value: JsonValue
) -> None:
    raw = shard(product_root)
    obj(first_record(raw)["data"])[field] = value
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


@pytest.mark.parametrize("level", ["shard", "record", "data", "name"])
@pytest.mark.parametrize("operation", ["missing", "extra"])
def test_closed_complete_field_sets(
    product_root: Path, level: str, operation: str
) -> None:
    raw = shard(product_root)
    record = first_record(raw)
    data = obj(record["data"])
    target = {
        "shard": raw,
        "record": record,
        "data": data,
        "name": obj(data["name"]),
    }[level]
    if operation == "extra":
        target["unknown"] = None
    else:
        del target[next(iter(target))]
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


@pytest.mark.parametrize("value", [True, 2, "1"])
def test_explicit_version_type(product_root: Path, value: JsonValue) -> None:
    raw = shard(product_root)
    raw["format"] = value
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("filing_key", "PR", "filing key"),
        ("record_key", "product_family:BP02", "Invalid product authored fields"),
        (
            "record_key",
            '[ "product_family", "BP02" ]',
            "Invalid product authored fields",
        ),
        ("evidence", [reference(), reference()], "Duplicate product evidence"),
    ],
)
def test_record_constraints(
    product_root: Path, field: str, value: JsonValue, message: str
) -> None:
    raw = shard(product_root)
    first_record(raw)[field] = value
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match=message):
        load(product_root)


def test_shard_requires_records(product_root: Path) -> None:
    raw = shard(product_root)
    raw["records"] = []
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


@pytest.mark.parametrize("field", ["code", "public_code", "id"])
def test_global_family_uniqueness(product_root: Path, field: str) -> None:
    copy = family("BP02" if field == "id" else "NEW")
    obj(copy["data"])[field] = {"code": "bp02", "public_code": "BP02", "id": "BP02"}[
        field
    ]
    obj(copy["data"])["kind"] = "other"
    install(
        product_root,
        "products/family/" + str(copy["filing_key"]) + "/002.yaml",
        envelope([copy]),
    )
    with pytest.raises(
        ValueError, match=r"Duplicate product record|Duplicate product family"
    ):
        load(product_root)


def test_unsorted_records_are_rejected(product_root: Path) -> None:
    left, right = family("LEFT"), family("RIGHT")
    left["filing_key"] = right["filing_key"] = "BP02"
    install(product_root, NAME, envelope([right, left]))
    with pytest.raises(ValueError, match="sorted and unique"):
        load(product_root)


@pytest.mark.parametrize(
    "path",
    [
        "products/index.yaml",
        "products/extra.yaml",
        "products/extra.yml",
        "products/family/BP02/01.yaml",
        "products/other/BP02/001.yaml",
        "products/family/BP02/001.yml",
        "products/family/BP02/nested/001.yaml",
    ],
)
def test_only_exact_allowed_paths(product_root: Path, path: str) -> None:
    target = product_root / path
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text("{}", encoding="utf-8")
    with pytest.raises(ValueError, match="Unexpected product input path"):
        load(product_root)


@pytest.mark.parametrize("kind", ["shard", "directory", "dangling", "extra"])
def test_symlinks_never_enter_catalog(
    product_root: Path, tmp_path: Path, kind: str
) -> None:
    if kind == "directory":
        path = product_root / "products/family/BP02"
    else:
        path = product_root / (
            NAME if kind == "shard" else "products/family/BP02/999.yaml"
        )
    target = tmp_path / ("target-dir" if kind == "directory" else "target.yaml")
    if path.exists():
        path.rename(target)
    elif kind != "dangling":
        target.write_text("{}", encoding="utf-8")
    path.symlink_to(target)
    with pytest.raises(ValueError, match="Symlinks"):
        load(product_root)


def test_missing_directory_never_reads_as_empty(product_root: Path) -> None:
    shutil.rmtree(product_root / "products")
    with pytest.raises(ValueError, match="Missing product input directory"):
        load(product_root)


@pytest.mark.parametrize(
    "text",
    [
        "x: 1\nx: 2\n",
        "x: &x [*x]\n",
        "x: !custom hi\n",
        "---\nx: 1\n---\ny: 2\n",
        "1: value\n",
        "%YAML 1.1\n---\nx: 1\n",
        "x: .nan\n",
    ],
)
def test_yaml_boundary_is_used(product_root: Path, text: str) -> None:
    (product_root / NAME).write_text(text, encoding="utf-8")
    with pytest.raises((ValueError, TypeError, YAMLError)):
        load(product_root)


def test_oversized_yaml_stops_before_parse(product_root: Path) -> None:
    (product_root / NAME).write_bytes(b" " * 1_048_576)
    with pytest.raises(ValueError, match="Oversized"):
        load(product_root)


def test_global_products_and_inclusions_are_typed_before_projection(
    product_root: Path,
) -> None:
    registry = load_registry(product_root)
    printing = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, PrintingData) and r.data.region == "jp"
    )
    install(product_root, "products/product/unassigned/001.yaml", envelope([product()]))
    install(
        product_root,
        "products/inclusion/unassigned/001.yaml",
        envelope([inclusion(printing.id)]),
    )
    assert len(load_products(product_root, registry=registry).records) == 5


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("id", "Example"),
        ("id", "example\n"),
        ("id", " example"),
        ("id", "éxample"),
        ("id", 1),
        ("region", "cn"),
        ("product_type", "Pack"),
        ("product_type", None),
        ("name", {"lang": "ja", "text": ""}),
        ("released_on", "2026-02-30"),
        ("released_on", "2026-01-01"),
        ("date_precision", "day"),
        ("date_precision", "month"),
        ("date_precision", "year"),
    ],
)
def test_product_domains_validated_even_without_db_projection(
    product_root: Path, field: str, value: JsonValue
) -> None:
    record = product(region="en")
    obj(record["data"])[field] = value
    install(product_root, "products/product/unassigned/001.yaml", envelope([record]))
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


@pytest.mark.parametrize("area", ["product", "inclusion"])
def test_nonfamily_evidence_required(product_root: Path, area: str) -> None:
    record = product() if area == "product" else inclusion("p:" + "0" * 32)
    record["evidence"] = []
    install(product_root, f"products/{area}/unassigned/001.yaml", envelope([record]))
    with pytest.raises(ValueError, match="evidence must be nonempty"):
        load(product_root)


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("store_id", "../bad"),
        ("batch_id", "hash"),
        ("source_version_id", "src:unknown"),
        ("locator", ""),
        ("role", ""),
    ],
)
def test_each_evidence_field(product_root: Path, field: str, value: JsonValue) -> None:
    raw = shard(product_root)
    ref = reference()
    ref[field] = value
    first_record(raw)["evidence"] = [ref]
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="Invalid product authored"):
        load(product_root)


def test_product_fk_requires_family(product_root: Path) -> None:
    record = product()
    obj(record["data"])["family_id"] = "MISSING"
    install(product_root, "products/product/unassigned/001.yaml", envelope([record]))
    with pytest.raises(ValueError, match="Missing referenced product family"):
        load(product_root)


@pytest.mark.parametrize("case", ["product", "printing", "region"])
def test_inclusion_fk_and_region_constraints(product_root: Path, case: str) -> None:
    registry = load_registry(product_root)
    printing = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, PrintingData) and r.data.region == "jp"
    )
    if case != "product":
        install(
            product_root,
            "products/product/unassigned/001.yaml",
            envelope([product(region="en" if case == "region" else "jp")]),
        )
    record = inclusion("p:" + "0" * 32 if case == "printing" else printing.id)
    install(product_root, "products/inclusion/unassigned/001.yaml", envelope([record]))
    with pytest.raises(
        ValueError, match=r"Missing referenced product/printing|regions disagree"
    ):
        load_products(product_root, registry=registry)


@pytest.mark.parametrize(
    ("date", "precision", "raw"),
    [
        ("2024-02-29", "day", "2024年2月29日"),
        (None, "month", "2026年9月"),
        (None, "year", "2026年"),
        (None, "unknown", "Undetermined"),
    ],
)
def test_valid_date_precisions_preserve_original_text(
    product_root: Path, date: str | None, precision: str, raw: str
) -> None:
    record = product()
    obj(record["data"]).update(released_on=date, date_precision=precision, date_raw=raw)
    install(product_root, "products/product/unassigned/001.yaml", envelope([record]))
    loaded = load(product_root).records['["product","example"]']
    from sve_carddb.domains.products.models import ProductRecord  # ruff: ignore[import-outside-top-level] -- narrow union assertion

    assert isinstance(loaded, ProductRecord)
    assert loaded.data.released_on == date
    assert loaded.data.date_raw == raw


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("product_id", "Example"),
        ("printing_id", "invalid"),
        ("first_available_on", "2026-02-30"),
        ("first_available_on", "2026-09-30"),
        ("first_available_precision", "day"),
        ("first_available_precision", "month"),
        ("first_available_precision", "year"),
        ("first_available_raw", "Unknown"),
        ("inclusion_kind", "booster"),
        ("note", {"lang": "ja"}),
    ],
)
def test_each_inclusion_domain_and_override(
    product_root: Path, field: str, value: JsonValue
) -> None:
    registry = load_registry(product_root)
    printing = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, PrintingData) and r.data.region == "jp"
    )
    install(product_root, "products/product/unassigned/001.yaml", envelope([product()]))
    record = inclusion(printing.id)
    obj(record["data"])[field] = value
    install(product_root, "products/inclusion/unassigned/001.yaml", envelope([record]))
    with pytest.raises(ValueError, match="Invalid product authored"):
        load_products(product_root, registry=registry)


def test_path_kind_is_checked_independently_of_record_fields(
    product_root: Path,
) -> None:
    raw = envelope([product()])
    first_record(raw)["filing_key"] = "BP02"
    install(product_root, NAME, raw)
    with pytest.raises(ValueError, match="kind/filing key"):
        load(product_root)


def test_duplicate_record_is_rejected_before_any_projection(
    product_root: Path,
) -> None:
    install(product_root, "products/family/BP02/002.yaml", shard(product_root))
    with pytest.raises(ValueError, match="Duplicate product record"):
        load(product_root)

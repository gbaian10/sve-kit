"""Family evidence must resolve to sealed bytes and first-receipt provenance."""

import json
from dataclasses import replace
from datetime import timedelta
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.products import load_products, populate_families
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import (
    ArchiveError,
    seal_batch,
    verify_batch,
)

from ...ingest.test_source_archive import _put, _put_zst, _store
from ...support.product_fixtures import (
    LANGUAGES,
    envelope,
    family,
    first_record,
    install,
    obj,
)
from ...support.product_fixtures import product_root as product_root  # ruff: ignore[useless-import-alias] -- shared fixture
from ...support.registry_preview_fixtures import BUILD, REVISION
from ...support.registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- shared fixture dependency
from ..registry.test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared fixture dependency

if TYPE_CHECKING:
    from pathlib import Path

NAME = "products/family/BP02/001.yaml"


@pytest.fixture
def referenced_family(product_root: Path, tmp_path: Path) -> tuple[Path, str, str]:
    store = _store(tmp_path / "evidence")
    resource = _put_zst(
        store,
        "https://example.invalid/family",
        "raw/card.html.zst",
        b"<html>Synthetic product evidence</html>",
        Kind.CARD,
    )
    stored = (store.data_root / resource.path).read_bytes()
    resource = replace(
        resource,
        first_fetched_at=resource.first_fetched_at - timedelta(days=1),
        etag="first-etag",
    )
    _put(store, resource, stored)
    seal_batch(store)
    resource = replace(
        resource,
        etag="later-etag",
        last_checked_at=resource.last_checked_at + timedelta(days=1),
    )
    _put(store, resource, stored)
    batch = seal_batch(store)
    inventory = verify_batch(store.root, store.store_id, batch.batch_id)
    source = inventory.entries[0].source_version_id
    raw = obj(read_yaml(product_root / NAME))
    first_record(raw)["evidence"] = [
        {
            "batch_id": batch.batch_id,
            "source_version_id": source,
            "locator": "product block 0",
            "role": "family review",
        }
    ]
    install(product_root, NAME, raw)
    return store.root, batch.batch_id, source


def test_family_evidence_uses_descriptor_raw_hash_and_original_locator(
    product_root: Path, referenced_family: tuple[Path, str, str]
) -> None:
    root, batch, source = referenced_family
    inventory = verify_batch(root, "test-store", batch)
    catalog = load_products(product_root, registry=load_registry(product_root))
    before = {p: p.read_bytes() for p in root.rglob("*") if p.is_file()}
    with create_database(compile_build()) as db, db.transaction():
        inputs = populate_families(
            db,
            catalog,
            build=BUILD,
            authored_revision=REVISION,
            languages=LANGUAGES,
            stores={"test-store": root},
        )
        raw = next(
            r.values for r in db.rows("source_record") if r.values["id"] == source
        )
        assert raw["kind"] == "official_page"
        assert raw["sha256"] == inventory.entries[0].blob.sha256
        assert raw["raw_locator"] == "test-store:" + inventory.entries[0].blob.path
        assert raw["url"] == "https://example.invalid/family"
        assert raw["fetched_at"] == "2026-09-29T00:00:00Z"
        assert raw["etag"] == "first-etag"
        [use] = [use for use in inputs.uses if use.source.id == source]
        assert use.usage == "product_evidence_closure"
        assert json.loads(use.locator)["locator"] == "product block 0"
    assert before == {p: p.read_bytes() for p in before}


@pytest.mark.parametrize(
    "case",
    [
        "unconfigured",
        "unsealed",
        "absent_version",
        "corrupt_raw",
        "corrupt_descriptor",
        "corrupt_receipt",
        "corrupt_manifest",
    ],
)
def test_each_evidence_closure_failure_is_atomic(
    product_root: Path, referenced_family: tuple[Path, str, str], case: str
) -> None:
    root, batch, _ = referenced_family
    inventory = verify_batch(root, "test-store", batch)
    entry = inventory.entries[0]
    if case == "unsealed":
        (root / "batches" / batch.removeprefix("sha256:") / "seal.json").unlink()
    elif case == "absent_version":
        raw = obj(read_yaml(product_root / NAME))
        evidence = first_record(raw)["evidence"]
        assert isinstance(evidence, list)
        obj(evidence[0])["source_version_id"] = "src:v1:" + "0" * 64
        install(product_root, NAME, raw)
    elif case.startswith("corrupt"):
        path = {
            "corrupt_raw": root / entry.blob.path,
            "corrupt_descriptor": root
            / "descriptors"
            / (entry.descriptor_sha256.removeprefix("sha256:") + ".json"),
            "corrupt_receipt": root
            / "receipts"
            / (entry.receipt_id.removeprefix("sha256:") + ".json"),
            "corrupt_manifest": root
            / "batches"
            / batch.removeprefix("sha256:")
            / "manifest.sqlite",
        }[case]
        path.write_bytes(b"Corrupt synthetic evidence")
    catalog = load_products(product_root, registry=load_registry(product_root))
    with create_database(compile_build()) as db:
        with pytest.raises((ValueError, ArchiveError)), db.transaction():
            populate_families(
                db,
                catalog,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
                stores={} if case == "unconfigured" else {"test-store": root},
            )
        for table in ("language", "source_record", "product_family"):
            assert not db.rows(table)


def test_shared_evidence_uses_are_deduplicated_and_distinct_locators_survive(
    product_root: Path, referenced_family: tuple[Path, str, str]
) -> None:
    root, _, source = referenced_family
    raw = obj(read_yaml(product_root / NAME))
    first = first_record(raw)
    refs = first["evidence"]
    assert isinstance(refs, list)
    refs.append(obj(refs[0]).copy() | {"locator": "product block 1"})
    shared = family("BP02_extra")
    shared["filing_key"] = "BP02"
    shared["evidence"] = first["evidence"]
    # Keep fixture serialization independent of Python object sharing.

    raw = obj(json.loads(json.dumps(envelope([first, shared]))))
    install(product_root, NAME, raw)
    catalog = load_products(product_root, registry=load_registry(product_root))
    with create_database(compile_build()) as db, db.transaction():
        inputs = populate_families(
            db,
            catalog,
            build=BUILD,
            authored_revision=REVISION,
            languages=LANGUAGES,
            stores={"test-store": root},
        )
        locators = [
            json.loads(use.locator)["locator"]
            for use in inputs.uses
            if use.source.id == source
        ]
        assert sorted(locators) == ["product block 0", "product block 1"]

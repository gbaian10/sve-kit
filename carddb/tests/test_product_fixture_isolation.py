"""Destructive consumers cannot change later synthetic fixture copies."""

from dataclasses import FrozenInstanceError
from typing import TYPE_CHECKING

import pytest
from pydantic import ValidationError

from sve_carddb.products import load_products
from sve_carddb.registry.review import Correction
from sve_carddb.registry.snapshot import load_registry

from .fixture_files import FrozenFiles, freeze_files, restore_files
from .product_identity_fixtures import NAME, add_page, commit, html

if TYPE_CHECKING:
    from pathlib import Path

    from .product_identity_fixtures import IdentityTemplate
    from .registry_snapshot_fixtures import RegistryTemplate


def test_identity_copies_isolate_checkout_archive_and_git(
    identity_template: IdentityTemplate, tmp_path: Path
) -> None:
    first = identity_template.copy(tmp_path / "first")
    before_checkout = freeze_files(identity_template.root.parent)
    before_archive = freeze_files(identity_template.store.parent)
    added = add_page(first, html(number="TEST-002"))
    assert added.card_no == "TEST-002"
    (first.root / NAME).write_bytes(b"Destroyed synthetic shard")
    assert commit(first.root) != identity_template.revision
    next((first.store / "raw").rglob("*.raw")).write_bytes(b"Corrupt bytes")
    first.pages = ()
    first.revision = "changed"

    second = identity_template.copy(tmp_path / "second")
    assert freeze_files(second.root.parent) == before_checkout
    assert freeze_files(second.store.parent) == before_archive
    assert freeze_files(identity_template.root.parent) == before_checkout
    assert freeze_files(identity_template.store.parent) == before_archive
    assert second.pages == identity_template.pages
    assert second.revision == identity_template.revision
    assert second.official().products


def test_shared_identity_values_reject_nested_mutation(
    identity_template: IdentityTemplate, tmp_path: Path
) -> None:
    copy = identity_template.copy(tmp_path / "consumer")
    source = copy.pages[0].source
    with pytest.raises(ValidationError, match="frozen"):
        source.archive.batch_id = "sha256:" + "0" * 64  # type: ignore[misc]  # exercise frozen nested provenance
    evidence = next(iter(copy.preview.evidence.values()))
    with pytest.raises(TypeError):
        copy.preview.evidence["jp", "changed"] = evidence  # type: ignore[index]  # exercise immutable evidence mapping
    record = next(iter(copy.catalog.records.values()))
    with pytest.raises(TypeError):
        copy.catalog.records["changed"] = record  # type: ignore[index]  # exercise immutable catalog mapping
    with pytest.raises(FrozenInstanceError):
        identity_template.revision = "changed"  # type: ignore[misc]  # exercise immutable template holder
    assert copy.pages == identity_template.pages


def test_registry_and_product_files_restore_independent_consumers(
    registry_template: RegistryTemplate,
    product_files: FrozenFiles,
    tmp_path: Path,
) -> None:
    first, second = tmp_path / "first", tmp_path / "second"
    files = (*registry_template.files, *product_files)
    restore_files(files, first)
    for path in first.rglob("*.yaml"):
        path.write_bytes(b"Destroyed synthetic input")
    corrections = [
        Correction.model_validate_json(value) for value in registry_template.corrections
    ]
    corrections[0].corrected_value = "polluted"
    restore_files(files, second)
    assert freeze_files(second) == tuple(sorted(files))
    assert load_products(second, registry=load_registry(second)).records
    assert (
        Correction.model_validate_json(registry_template.corrections[0]).corrected_value
        != "polluted"
    )

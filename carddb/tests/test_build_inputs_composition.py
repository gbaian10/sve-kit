"""Shared sealed evidence never changes permanent identities or adoption gates."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import create_database
from sve_carddb.build.source_rows import insert_raw_sources, source_values
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.provenance import SourceUse, input_record
from sve_carddb.products import (
    import_product_preview,
    load_products,
    populate_product_preview,
)
from sve_carddb.registry.preview import plan_preview
from sve_carddb.registry.records import AllocationData, PrintingData

from .build_input_fixtures import (
    expected_uses,
    frozen_provider,
    reference_identity_source,
)
from .product_fixtures import LANGUAGES
from .product_fixtures import product_root as product_root  # ruff: ignore[useless-import-alias] -- shared fixture
from .registry_preview_fixtures import BUILD, REVISION
from .registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- fixture dependency
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- fixture dependency

if TYPE_CHECKING:
    from collections.abc import Iterable
    from pathlib import Path

    from sve_carddb.core.provenance import BuildContext, InputRecord
    from sve_carddb.registry.review import Inputs

    from .identity_evidence_fixtures import MemoryEvidence


def _omit_first_use(build: BuildContext, uses: Iterable[SourceUse]) -> InputRecord:
    return input_record(build, tuple(uses)[1:])


def _jp_only(provider: MemoryEvidence) -> MemoryEvidence:
    return replace(
        provider,
        cards={key: value for key, value in provider.cards.items() if key[0] == "jp"},
    )


@pytest.mark.parametrize("preexisting", [False, True])
def test_family_and_identity_share_sealed_raw_and_keep_every_use(
    product_root: Path, inputs: Inputs, tmp_path: Path, preexisting: bool
) -> None:
    provider, store = frozen_provider(inputs, tmp_path / "frozen")
    reference_identity_source(product_root, provider)
    plan = plan_preview(product_root, provider, regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    before = {path: path.read_bytes() for path in product_root.rglob("*.yaml")}
    expected_uses(catalog, plan, store)
    shared = provider.cards["jp", "BP02-071"].source
    with create_database(compile_build()) as db:
        if preexisting:
            with db.transaction():
                insert_raw_sources(db, (shared,))
        record = import_product_preview(
            db,
            catalog,
            plan,
            authored_revision=REVISION,
            build=BUILD,
            languages=LANGUAGES,
            stores={"test-store": store},
        )
        sources = [
            row.values
            for row in db.rows("source_record")
            if row.values["id"] == shared.id
        ]
        assert sources == [source_values(shared)]
        shared_uses = [use for use in record.uses if use.source.id == shared.id]
        assert {use.usage for use in shared_uses} == {
            "product_evidence_closure",
            "registry_observation",
        }
        assert {use.source.parser_version for use in shared_uses} == {
            "archive-closure-v1",
            "synthetic-json-html-v1",
        }
        assert not db.rows("decision_source")
        assert {
            row.values["parser_version"]
            for row in db.rows("source_record")
            if row.values["kind"] == "authored"
        } == {"registry-envelope-v1", "product-authored-v1"}
        allocations = {
            r.data.printing_id: r.data.int_id
            for r in plan.included("card_int_id")
            if isinstance(r.data, AllocationData)
        }
        assert {
            r.values["printing_id"]: r.values["int_id"] for r in db.rows("card_int_id")
        } == allocations
        owners = {
            r.data.id: r.data.home_set_id
            for r in plan.included("printing")
            if isinstance(r.data, PrintingData)
        }
        assert {
            r.values["id"]: r.values["home_set_id"] for r in db.rows("printing")
        } == owners
        assert plan.snapshot.files.index().next_int_id == {"jp": 20003, "en": 60003}
    assert before == {path: path.read_bytes() for path in before}


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("url", "https://example.invalid/conflict"),
        ("sha256", "sha256:" + "0" * 64),
        ("kind", "official_api"),
        ("raw_locator", "test-store:raw/conflict"),
        ("fetched_at", "2026-10-01T00:00:00Z"),
        ("etag", "wrong"),
        ("last_modified", "wrong"),
    ],
)
def test_each_conflict_rolls_back_family_audit_and_identity(
    product_root: Path, inputs: Inputs, tmp_path: Path, field: str, value: str
) -> None:
    provider, store = frozen_provider(inputs, tmp_path / "frozen")
    reference_identity_source(product_root, provider)
    cards = dict(provider.cards)
    item = cards["jp", "BP02-071"]
    cards["jp", "BP02-071"] = replace(
        item, source=item.source.model_copy(update={field: value})
    )
    plan = plan_preview(product_root, replace(provider, cards=cards), regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build()) as db:
        with pytest.raises(ValueError, match="Conflicting raw source metadata"):
            import_product_preview(
                db,
                catalog,
                plan,
                authored_revision=REVISION,
                build=BUILD,
                languages=LANGUAGES,
                stores={"test-store": store},
            )
        for table in (
            "source_record",
            "decision",
            "decision_source",
            "product_family",
            "card",
            "printing",
            "card_int_id",
            "language",
            "text_unit",
        ):
            assert not db.rows(table)


def test_product_usage_does_not_bypass_en_identity_gate(
    product_root: Path, inputs: Inputs, tmp_path: Path
) -> None:
    provider, store = frozen_provider(inputs, tmp_path / "frozen")
    reference_identity_source(product_root, provider)
    cards = dict(provider.cards)
    item = cards["en", "BP02-070EN"]
    cards["en", "BP02-070EN"] = replace(
        item,
        observation=item.observation.model_copy(
            update={"rules_hash": "sha256:" + "0" * 64}
        ),
    )
    plan = plan_preview(
        product_root, replace(provider, cards=cards), regions=("jp", "en")
    )
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build(("en", "related"))) as db:
        record = import_product_preview(
            db,
            catalog,
            plan,
            authored_revision=REVISION,
            build=BUILD,
            languages=LANGUAGES,
            stores={"test-store": store},
        )
        product_use = SourceUse(
            source=item.source.model_copy(update={"parser_version": "product-v1"}),
            usage="product_observation",
            locator="product block 0",
        )
        with db.transaction():
            insert_raw_sources(db, (product_use.source,))
            input_record(BUILD, (*record.uses, product_use))
        assert "BP02-070EN" not in {r.values["card_no"] for r in db.rows("printing")}
        assert any(
            p.record_key.startswith("printing:") and "observation_mismatch" in p.reasons
            for p in plan.projections
        )


@pytest.mark.parametrize("late_failure", [False, True])
def test_official_product_references_share_identity_source_without_new_decision(
    product_root: Path,
    inputs: Inputs,
    tmp_path: Path,
    late_failure: bool,
) -> None:
    provider, store = frozen_provider(inputs, tmp_path / "frozen")
    reference_identity_source(product_root, provider)
    plan = plan_preview(product_root, provider, regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    source = provider.cards["jp", "BP02-071"].source.model_copy(
        update={"parser_version": "synthetic-product-parser-v1"}
    )
    uses = (
        SourceUse(
            source=source, usage="product_observation", locator="product block 0"
        ),
        SourceUse(
            source=source,
            usage="printing_product_observation",
            locator="product block 0",
        ),
    )
    with create_database(compile_build()) as db:

        def populate() -> None:
            # The generic source boundary is under test; this is not the product importer.
            record = populate_product_preview(
                db,
                catalog,
                plan,
                authored_revision=REVISION,
                build=BUILD,
                languages=LANGUAGES,
                stores={"test-store": store},
            )
            decisions = len(db.rows("decision"))
            insert_raw_sources(db, (source,))
            db.insert(
                "product",
                {
                    "id": "synthetic-product",
                    "region": "jp",
                    "family_id": None,
                    "name_unit_id": db.rows("text_unit")[0].values["id"],
                    "product_type": "other",
                    "date_precision": "unknown",
                    "source_id": source.id,
                },
            )
            db.insert(
                "printing_product",
                {
                    "printing_id": db.rows("printing")[0].values["id"],
                    "product_id": "synthetic-product",
                    "inclusion_kind": "other",
                    "source_id": source.id,
                },
            )
            input_record(BUILD, (*record.uses, *uses))
            assert len(db.rows("decision")) == decisions
            if late_failure:
                raise ValueError("Synthetic late product failure")

        if late_failure:
            with (
                pytest.raises(ValueError, match="late product failure"),
                db.transaction(),
            ):
                populate()
            for table in (
                "source_record",
                "decision",
                "decision_source",
                "product_family",
                "card",
                "printing",
                "product",
                "printing_product",
                "text_unit",
                "language",
                "card_int_id",
            ):
                assert not db.rows(table)
        else:
            with db.transaction():
                populate()
            assert len(db.rows("product")) == 1
            assert len(db.rows("printing_product")) == 1

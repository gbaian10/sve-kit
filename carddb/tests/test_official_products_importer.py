"""Real importer composition over synthetic sealed input, without fabricated adoption."""

import shutil
import sqlite3
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_bundle import publish_bundle, verify_bundle
from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import input_record
from sve_carddb.extract.compare_en import parse_card
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.products import (
    import_product_preview,
    load_products,
    official_importer,
    populate_product_preview,
    product_preview_uses,
)
from sve_carddb.products.archive import FrozenProducts
from sve_carddb.products.plan import plan_official_products
from sve_carddb.registry.build import build
from sve_carddb.registry.inputs import Mapping as CardMapping
from sve_carddb.registry.preview import FrozenEN, FrozenJP, FrozenRegions, plan_preview
from sve_carddb.registry.preview.evidence import (
    CardEvidence,
    FaceEvidence,
    MemoryEvidence,
)
from sve_carddb.registry.records import AllocationData, PrintingData
from sve_carddb.registry.review import Inputs, Receipt
from sve_carddb.registry.storage import plan_files, write_files
from sve_carddb.source_archive import ArchiveError

from .product_identity_fixtures import (
    LANGUAGES,
    IdentityFixture,
    add_page,
    commit,
    html,
    identity_envelope,
    identity_record,
    install_identity,
)
from .product_identity_fixtures import identity_fixture as identity_fixture  # ruff: ignore[useless-import-alias] -- shared fixture

if TYPE_CHECKING:
    from collections.abc import Iterable, Mapping
    from pathlib import Path

    from sve_carddb.build_db import Database, Value
    from sve_carddb.build_inputs import BuildContext, InputRecord, SourceUse
    from sve_carddb.products.official import ProductPage
    from sve_carddb.products.plan import OfficialProducts
    from sve_carddb.registry.records import Region


def changed_page(fixture: IdentityFixture, raw: bytes) -> ProductPage:

    page = add_page(fixture, raw, number="TEST-001")
    card = legacy_projection(extract_card(raw, number="TEST-001"))
    evidence = CardEvidence.from_card(
        page.source.model_copy(update={"parser_version": "synthetic-jp-identity-v1"}),
        "jp",
        card,
        (FaceEvidence("LG", None),),
    )
    fixture.preview = plan_preview(
        fixture.root,
        MemoryEvidence({("jp", "TEST-001"): evidence}),
        regions=fixture.preview.regions,
    )
    return page


def populate(
    fixture: IdentityFixture, db: Database, official: OfficialProducts
) -> InputRecord:
    return populate_product_preview(
        db,
        fixture.catalog,
        fixture.preview,
        authored_revision=fixture.revision,
        build=fixture.context(official.identities),
        languages=LANGUAGES,
        stores={"test-store": fixture.store},
        official=official,
    )


def imported(
    fixture: IdentityFixture, db: Database, official: OfficialProducts
) -> InputRecord:
    return import_product_preview(
        db,
        fixture.catalog,
        fixture.preview,
        authored_revision=fixture.revision,
        build=fixture.context(official.identities),
        languages=LANGUAGES,
        stores={"test-store": fixture.store},
        official=official,
    )


def test_complete_graph_keeps_ids_owners_dates_region_and_raw_provenance(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    official = fixture.official()
    with create_database(compile_build()) as db:
        record = imported(fixture, db, official)
        assert len(db.rows("product")) == 1
        assert len(db.rows("printing_product")) == 1
        product = db.rows("product")[0].values
        assert product["id"] == "permanent-example"
        assert product["region"] == "jp"
        assert product["family_id"] is None
        assert product["released_on"] == "2026-09-30"
        assert product["date_precision"] == "day"
        assert product["date_raw"] == "2026-09-30"
        assert product["product_code"] == "Test-A"
        assert product["source_id"] == fixture.pages[0].source.id
        inclusion = db.rows("printing_product")[0].values
        assert inclusion["source_id"] == product["source_id"]
        assert inclusion["inclusion_kind"] == "other"
        assert inclusion["first_available_precision"] is None
        assert inclusion["first_available_on"] is None
        assert inclusion["first_available_raw"] is None
        original = {
            item.data.id: item.data.home_set_id
            for item in fixture.preview.included("printing")
            if isinstance(item.data, PrintingData)
        }
        assert {
            row.values["id"]: row.values["home_set_id"] for row in db.rows("printing")
        } == original
        allocations = {
            item.data.printing_id: item.data.int_id
            for item in fixture.preview.snapshot.records.values()
            if isinstance(item.data, AllocationData)
        }
        assert {
            row.values["printing_id"]: row.values["int_id"]
            for row in db.rows("card_int_id")
        } == allocations
        raw = [
            row.values
            for row in db.rows("source_record")
            if row.values["kind"] != "authored"
        ]
        assert raw == [fixture.pages[0].source.values()]
        assert {use.usage for use in record.uses} == {
            "registry_observation",
            "product_identity_evidence_closure",
            "official_product_identity",
            "official_product_page",
            "official_product",
            "official_printing_product",
        }
        assert {
            row.values["parser_version"]
            for row in db.rows("source_record")
            if row.values["kind"] == "authored"
        } == {"registry-envelope-v1", "product-authored-v1", "product-identity-v1"}
        assert {row.values["category"] for row in db.rows("decision")} == {
            "identity_registry",
            "product_catalog",
            "product_identity",
        }
        assert all(
            row.values["source_id"] == fixture.pages[0].source.id
            for row in db.rows("decision_source")
            if str(row.values["role"]).startswith("product_identity_evidence:")
        )
        db.verify()


def test_frozen_product_batch_is_exact_read_only(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    before = {
        path: path.read_bytes() for path in fixture.store.rglob("*") if path.is_file()
    }
    assert (
        FrozenProducts(fixture.store, "test-store", fixture.batch, region="jp").pages()
        == fixture.pages
    )
    assert before == {path: path.read_bytes() for path in before}
    with pytest.raises(ValueError, match="source identity mismatch"):
        FrozenProducts(fixture.store, "test-store", fixture.batch, region="en").pages()


@pytest.mark.parametrize(
    ("date", "precision"),
    [(None, "unknown"), ("TBA", "unknown"), ("2026-09", "month"), ("2026", "year")],
)
def test_date_precision_survives_db_without_guessing(
    identity_fixture: IdentityFixture, date: str | None, precision: str
) -> None:
    fixture = identity_fixture
    page = changed_page(fixture, html(date=date))
    official = plan_official_products(fixture.load(), (page,), fixture.preview)
    with create_database(compile_build()) as db:
        imported(fixture, db, official)
        product = db.rows("product")[0].values
        assert product["released_on"] is None
        assert product["date_precision"] == precision
        assert product["date_raw"] == date
        assert (
            db.rows("printing_product")[0].values["first_available_precision"] is None
        )


@pytest.mark.parametrize(
    "name", ["Promo Cards", "PRカード", "Unknown Synthetic Edition"]
)
def test_missing_distribution_type_never_comes_from_owner(
    identity_fixture: IdentityFixture, name: str
) -> None:
    fixture = identity_fixture
    page = changed_page(fixture, html(name=name, date=None))
    official = plan_official_products(fixture.load(), (page,), fixture.preview)
    assert len(official.products) == len(official.inclusions) == 1
    assert official.products[0].data.product_type == "booster"
    with create_database(compile_build()) as db:
        imported(fixture, db, official)
        assert len(db.rows("printing")) == len(db.rows("product")) == 1
        assert db.rows("product")[0].values["product_type"] == "booster"
        assert db.rows("printing_product")[0].values["inclusion_kind"] == "other"


def test_zero_match_keeps_every_read_use_without_allocating_product(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    page = changed_page(fixture, html(links=("/products/new/",)))
    official = plan_official_products(fixture.load(), (page,), fixture.preview)
    assert any(
        isinstance(item, dict) and item["reason"] == "missing_product_identity"
        for item in official.diagnostics
    )
    with create_database(compile_build()) as db:
        record = imported(fixture, db, official)
        assert not db.rows("product")
        assert not db.rows("printing_product")
        assert {use.usage for use in record.uses} >= {
            "official_product",
            "official_printing_product",
        }
        assert len(db.rows("printing")) == 1


def test_same_card_number_from_different_raw_version_excludes_inclusion(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    page = add_page(fixture, html() + b"\n", number="TEST-001")
    assert page.card_no == fixture.pages[0].card_no
    assert page.source.id != fixture.pages[0].source.id
    official = plan_official_products(fixture.load(), (page,), fixture.preview)
    assert len(official.products) == 1
    assert not official.inclusions
    assert any(
        isinstance(item, dict) and item["reason"] == "printing_source_mismatch"
        for item in official.diagnostics
    )
    with create_database(compile_build()) as db:
        imported(fixture, db, official)
        assert len(db.rows("printing")) == len(db.rows("product")) == 1
        assert not db.rows("printing_product")


@pytest.mark.parametrize("change", ["name", "date"])
def test_conflicting_content_is_not_selected_by_source_order(
    identity_fixture: IdentityFixture, change: str
) -> None:
    fixture = identity_fixture
    changed = changed_page(
        fixture,
        html(
            name="Different title" if change == "name" else "Booster Pack Synthetic",
            date="2025-01-01" if change == "date" else "2026-09-30",
        ),
    )
    for pages in ((fixture.pages[0], changed), (changed, fixture.pages[0])):
        with pytest.raises(
            ValueError, match="Conflicting official product observation"
        ):
            plan_official_products(fixture.load(), pages, fixture.preview)


def test_identical_observations_deduplicate_without_dropping_reads(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    official = plan_official_products(
        fixture.load(), (fixture.pages[0], fixture.pages[0]), fixture.preview
    )
    assert len(official.products) == len(official.inclusions) == 1
    with create_database(compile_build()) as db:
        imported(fixture, db, official)
        assert len(db.rows("product")) == len(db.rows("printing_product")) == 1


@pytest.mark.parametrize(
    "failure", ["late_insert", "foreign_key", "metadata", "region", "source", "context"]
)
def test_each_late_failure_rolls_back_complete_graph(
    identity_fixture: IdentityFixture, monkeypatch: pytest.MonkeyPatch, failure: str
) -> None:
    fixture = identity_fixture
    official = fixture.official()
    schema = compile_build()
    with create_database(schema) as db:
        original_insert = db.insert

        def failing_insert(table: str, values: Mapping[str, Value]) -> None:
            if table == "printing_product":
                if failure == "late_insert":
                    raise ValueError("Synthetic late failure")
                if failure == "foreign_key":
                    values = dict(values) | {"source_id": "missing-source"}
            original_insert(table, values)

        if failure in {"late_insert", "foreign_key"}:
            monkeypatch.setattr(db, "insert", failing_insert)
        elif failure == "metadata":
            official = replace(
                official,
                pages=(
                    replace(
                        official.pages[0],
                        source=official.pages[0].source.model_copy(
                            update={"etag": "conflicting-etag"}
                        ),
                    ),
                ),
            )
        elif failure == "region":
            observed = official.products[0]
            official = replace(
                official,
                products=(
                    replace(
                        observed, data=observed.data.model_copy(update={"region": "en"})
                    ),
                ),
            )
        elif failure == "source":
            observed_inclusion = official.inclusions[0]
            official = replace(
                official,
                inclusions=(
                    replace(
                        observed_inclusion,
                        page=replace(
                            observed_inclusion.page,
                            source=observed_inclusion.page.source.model_copy(
                                update={"id": "src:v1:" + "0" * 64}
                            ),
                        ),
                    ),
                ),
            )
        else:
            identities = replace(official.identities, index_hash="sha256:" + "0" * 64)
            # The context pins the old hash; use the original fixture context explicitly.
            official = replace(official, identities=identities)
        context = fixture.context(fixture.load())
        with pytest.raises((ValueError, sqlite3.IntegrityError)):
            import_product_preview(
                db,
                fixture.catalog,
                fixture.preview,
                authored_revision=fixture.revision,
                build=context,
                languages=LANGUAGES,
                stores={"test-store": fixture.store},
                official=official,
            )
        for table in schema.tables:
            assert not db.rows(table.name)


@pytest.mark.parametrize(
    "usage",
    [
        "product_identity_evidence_closure",
        "official_product_identity",
        "official_product_page",
        "official_product",
        "official_printing_product",
    ],
)
def test_each_omitted_actual_use_fails_and_rolls_back(
    identity_fixture: IdentityFixture, monkeypatch: pytest.MonkeyPatch, usage: str
) -> None:
    fixture = identity_fixture

    def omit(context: BuildContext, uses: Iterable[SourceUse]) -> InputRecord:
        return input_record(context, (use for use in uses if use.usage != usage))

    monkeypatch.setattr(official_importer, "input_record", omit)
    schema = compile_build()
    with create_database(schema) as db:
        with pytest.raises(ValueError, match="use closure"):
            imported(fixture, db, fixture.official())
        for table in schema.tables:
            assert not db.rows(table.name)


def test_complete_bundle_saved_reverified_and_never_overwritten(
    identity_fixture: IdentityFixture, tmp_path: Path
) -> None:
    fixture = identity_fixture
    official = fixture.official()
    context = fixture.context(official.identities)
    expected = product_preview_uses(
        fixture.catalog,
        fixture.preview,
        {"test-store": fixture.store},
        official=official,
    )
    destination = tmp_path / "bundle"
    schema = compile_build()
    record = publish_bundle(
        schema,
        destination,
        context,
        expected,
        lambda db: populate(fixture, db, official),
        {"official": official.report()},
        stores={"test-store": fixture.store},
    )
    assert (
        verify_bundle(
            schema, destination, context, expected, stores={"test-store": fixture.store}
        )
        == record
    )
    before = {path: path.read_bytes() for path in destination.iterdir()}
    with pytest.raises(FileExistsError, match="exists"):
        publish_bundle(
            schema,
            destination,
            context,
            expected,
            lambda db: populate(fixture, db, official),
            {},
            stores={"test-store": fixture.store},
        )
    assert before == {path: path.read_bytes() for path in before}


def test_missing_raw_after_planning_publishes_nothing(
    identity_fixture: IdentityFixture, tmp_path: Path
) -> None:
    fixture = identity_fixture
    official = fixture.official()
    expected = product_preview_uses(
        fixture.catalog,
        fixture.preview,
        {"test-store": fixture.store},
        official=official,
    )
    next((fixture.store / "raw").rglob("*.raw")).unlink()
    destination = tmp_path / "bundle"
    with pytest.raises((ArchiveError, FileNotFoundError)):
        publish_bundle(
            compile_build(),
            destination,
            fixture.context(official.identities),
            expected,
            lambda db: populate(fixture, db, official),
            {},
            stores={"test-store": fixture.store},
        )
    assert not destination.exists()


@pytest.mark.parametrize(
    "state",
    [
        "matched",
        "observation_mismatch",
        "missing_source",
        "outside_output_regions",
        "frozen_regions",
    ],
)
def test_english_inclusions_obey_existing_identity_gate(
    identity_fixture: IdentityFixture, state: str
) -> None:

    fixture = identity_fixture
    raw = html(
        number="TEST-001EN",
        links=("/products/en/", "/cards/searchresults?expansion=EN"),
    )
    raw = (
        raw.replace("クラス".encode(), b"Class")
        .replace("エルフ".encode(), b"Forestcraft")
        .replace("カード種類".encode(), b"Card Type")
        .replace("フォロワー".encode(), b"Follower")
        .replace("タイプ".encode(), b"Trait")
        .replace("レアリティ".encode(), b"Rarity")
    )
    page = add_page(fixture, raw, region="en", number="TEST-001EN")
    en_card = parse_card(raw, "TEST-001EN")
    jp_card = legacy_projection(extract_card(html(), number="TEST-001"))
    inputs = Inputs(
        jp={jp_card.number: jp_card},
        en={en_card.number: en_card},
        mapping=CardMapping(
            targets={en_card.number: jp_card.number},
            original_art={en_card.number},
            reskins={},
        ),
        receipt=Receipt(
            policy="identity-init-2026-09-28-v1",
            reviewed_by="synthetic-reviewer",
            reviewed_on="2026-09-30",
            input_hashes={"jp": "sha256:" + "1" * 64},
        ),
    )
    for directory in ("ids", "registry"):
        shutil.rmtree(fixture.root / directory)
    write_files(
        plan_files(fixture.root, build(inputs, {}), "synthetic-reviewer", "2026-09-30")
    )
    jp_evidence = CardEvidence.from_card(
        fixture.pages[0].source, "jp", jp_card, (FaceEvidence("LG", None),)
    )
    cards: dict[tuple[Region, str], CardEvidence] = {
        ("jp", jp_card.number): jp_evidence
    }
    if state != "missing_source":
        if state == "observation_mismatch":
            changed_raw = raw.replace(b"Synthetic rule.", b"Different synthetic rule.")
            en_card = parse_card(changed_raw, "TEST-001EN")
            page = add_page(fixture, changed_raw, region="en", number="TEST-001EN")
        cards["en", en_card.number] = CardEvidence.from_card(
            page.source, "en", en_card, (FaceEvidence("LG", None),)
        )
    if state == "frozen_regions":
        assert page.source.archive is not None
        frozen = FrozenRegions(
            jp=FrozenJP(
                fixture.store,
                "test-store",
                fixture.batch,
                parser_version="frozen-jp-test",
            ),
            en=FrozenEN(
                fixture.store,
                "test-store",
                page.source.archive.batch_id,
                parser_version="frozen-en-test",
            ),
        )
        fixture.preview = plan_preview(fixture.root, frozen, regions=("jp", "en"))
    else:
        fixture.preview = plan_preview(
            fixture.root,
            MemoryEvidence(cards),
            regions=("jp",) if state == "outside_output_regions" else ("jp", "en"),
        )
    fixture.catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    install_identity(
        fixture.root,
        identity_envelope([identity_record(page, product_id="english-id")]),
        name="product-identities/en/001.yaml",
    )
    fixture.revision = commit(fixture.root)
    fixture.pages = (*fixture.pages, page)
    official = fixture.official()
    en_inclusions = [item for item in official.inclusions if item.page.region == "en"]
    adopted = state in {"matched", "frozen_regions"}
    assert bool(en_inclusions) == adopted
    if state == "outside_output_regions":
        assert len(official.products) == 1
        assert any(
            isinstance(item, dict) and item["reason"] == "outside_output_regions"
            for item in official.diagnostics
        )
    elif not adopted:
        assert any(
            isinstance(item, dict) and item["reason"] == "printing_gate:" + state
            for item in official.diagnostics
        )
    with create_database(compile_build(("art", "en", "related"))) as db:
        record = imported(fixture, db, official)
        assert len(db.rows("printing_product")) == (2 if adopted else 1)
        assert len(db.rows("printing")) == (2 if adopted else 1)
        assert any(
            use.source.id == page.source.id and use.usage == "official_printing_product"
            for use in record.uses
        )
        assert len(db.rows("product")) == (
            1 if state == "outside_output_regions" else 2
        )
        if state == "frozen_regions":
            registry_uses = [
                use for use in record.uses if use.usage == "registry_observation"
            ]
            assert {use.source.parser_version for use in registry_uses} == {
                "frozen-jp-test",
                "frozen-en-test",
            }
            assert all(
                row.values["parser_version"] is None
                for row in db.rows("source_record")
                if row.values["kind"] != "authored"
            )
            assert {item.data.inclusion_kind for item in official.inclusions} == {
                "other"
            }


@pytest.mark.parametrize(
    "kind",
    ["booster", "promo", "deck", "collaboration", "special", "special_pack", "other"],
)
def test_confirmed_public_code_supplies_type_without_title_or_family_relation(
    identity_fixture: IdentityFixture, kind: str
) -> None:
    from .product_fixtures import envelope, family, install, obj  # ruff: ignore[import-outside-top-level] -- independent family input signing

    fixture = identity_fixture
    record = family("TEST")
    obj(record["data"]).update(public_code="Test-A", kind=kind)
    install(fixture.root, "products/family/TEST/001.yaml", envelope([record]))
    fixture.catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    fixture.revision = commit(fixture.root)
    page = changed_page(
        fixture, html(name="Unrecognized title with Pack Deck Set Bundle")
    )
    official = plan_official_products(fixture.load(), (page,), fixture.preview)
    assert official.products[0].data.product_type == kind
    assert official.products[0].data.family_id is None
    assert official.inclusions[0].data.inclusion_kind == "other"
    with create_database(compile_build()) as db:
        imported(fixture, db, official)
        assert db.rows("product")[0].values["product_type"] == kind
        assert db.rows("product")[0].values["family_id"] is None
        assert db.rows("printing_product")[0].values["inclusion_kind"] == "other"


@pytest.mark.parametrize("public_code", ["test-a", "test", "Test", "BP12-BP13"])
def test_unmatched_public_code_keeps_product_with_null_type_and_other_inclusion(
    identity_fixture: IdentityFixture, public_code: str
) -> None:
    from .product_fixtures import envelope, family, install, obj  # ruff: ignore[import-outside-top-level] -- independent family input signing

    fixture = identity_fixture
    record = family("TEST")
    obj(record["data"])["public_code"] = public_code
    install(fixture.root, "products/family/TEST/001.yaml", envelope([record]))
    fixture.catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    fixture.revision = commit(fixture.root)
    official = fixture.official()
    assert len(official.products) == len(official.inclusions) == 1
    assert official.products[0].data.product_type is None
    assert official.products[0].data.family_id is None
    assert any(
        isinstance(item, dict)
        and item["reason"] == "product_type_unknown"
        and item["excludes"] is None
        for item in official.diagnostics
    )
    with create_database(compile_build()) as db:
        imported(fixture, db, official)
        assert db.rows("product")[0].values["product_type"] is None
        assert db.rows("printing_product")[0].values["inclusion_kind"] == "other"


def test_product_code_conflict_is_independent_of_same_name_and_date(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    from .product_fixtures import envelope, family, install, obj  # ruff: ignore[import-outside-top-level] -- distinct codes with equal confirmed kind

    second_family = family("OTHER")
    obj(second_family["data"])["public_code"] = "Other"
    install(fixture.root, "products/family/OTHER/001.yaml", envelope([second_family]))
    fixture.catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    changed = add_page(
        fixture,
        html(
            number="TEST-002",
            links=("/products/synthetic/", "/cardlist/cardsearch?expansion=Other"),
        ),
    )
    install_identity(
        fixture.root,
        identity_envelope([identity_record(changed)]),
        name="product-identities/jp/002.yaml",
    )
    fixture.revision = commit(fixture.root)
    original = fixture.pages[0]
    assert original.blocks[0].name == changed.blocks[0].name
    assert original.blocks[0].date_raw == changed.blocks[0].date_raw
    for pages in ((original, changed), (changed, original)):
        with pytest.raises(ValueError, match="Conflicting official product content"):
            plan_official_products(fixture.load(), pages, fixture.preview)


def test_importer_source_guard_does_not_depend_on_planner_guard(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    original = fixture.official()
    changed = add_page(fixture, html() + b"\n", number="TEST-001")
    forged = replace(
        original,
        pages=(changed,),
        products=(replace(original.products[0], page=changed),),
        inclusions=(replace(original.inclusions[0], page=changed),),
    )
    with create_database(compile_build()) as db:
        with pytest.raises(
            ValueError,
            match="Official inclusion source differs from adopted printing source",
        ):
            imported(fixture, db, forged)
        assert not db.rows("printing")
        assert not db.rows("product")


def test_importer_authored_revision_must_equal_identity_revision(
    identity_fixture: IdentityFixture,
) -> None:
    fixture = identity_fixture
    official = fixture.official()
    with create_database(compile_build()) as db:
        with pytest.raises(
            ValueError, match="Product identity authored revision mismatch"
        ):
            import_product_preview(
                db,
                fixture.catalog,
                fixture.preview,
                authored_revision="0" * 40,
                build=fixture.context(official.identities),
                languages=LANGUAGES,
                stores={"test-store": fixture.store},
                official=official,
            )
        assert not db.rows("printing")
        assert not db.rows("product")


def test_proposed_family_does_not_supply_official_product_type(
    identity_fixture: IdentityFixture,
) -> None:
    from .product_fixtures import decision, envelope, family, install, obj, sign  # ruff: ignore[import-outside-top-level] -- valid proposed input keeps the confirmed owner parent

    fixture = identity_fixture
    confirmed = family("TEST")
    obj(confirmed["data"])["public_code"] = "Different"
    install(fixture.root, "products/family/TEST/001.yaml", envelope([confirmed]))
    proposed = family("PROPOSED")
    obj(proposed["data"]).update(public_code="Test-A", kind="deck")
    shard = envelope([proposed])
    decision(shard).update(
        state="proposed", reviewed_by=None, reviewed_at=None, reviewed_precision=None
    )
    sign(shard)
    install(fixture.root, "products/family/PROPOSED/001.yaml", shard)
    fixture.catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    fixture.revision = commit(fixture.root)
    official = fixture.official()
    assert official.products[0].data.product_type is None
    with create_database(compile_build()) as db:
        imported(fixture, db, official)
        assert len(db.rows("product_family")) == 1
        assert db.rows("product")[0].values["product_type"] is None


def test_compound_expansion_is_not_split_to_find_a_family_kind(
    identity_fixture: IdentityFixture,
) -> None:
    from .product_fixtures import envelope, family, install, obj  # ruff: ignore[import-outside-top-level] -- only one component has confirmed family evidence

    fixture = identity_fixture
    record = family("TEST")
    obj(record["data"])["public_code"] = "BP12"
    install(fixture.root, "products/family/TEST/001.yaml", envelope([record]))
    fixture.catalog = load_products(fixture.root, registry=fixture.preview.snapshot)
    page = changed_page(
        fixture,
        html(links=("/products/compound/", "/cardlist/cardsearch?expansion=BP12-BP13")),
    )
    install_identity(
        fixture.root,
        identity_envelope([identity_record(page)]),
        name="product-identities/jp/002.yaml",
    )
    fixture.revision = commit(fixture.root)
    official = plan_official_products(fixture.load(), (page,), fixture.preview)
    assert official.products[0].data.product_code == "BP12-BP13"
    assert official.products[0].data.product_type is None
    assert official.products[0].data.family_id is None
    assert official.inclusions[0].data.inclusion_kind == "other"

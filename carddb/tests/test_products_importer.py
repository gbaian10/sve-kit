"""Family provenance, text reuse and all-or-nothing regional identity composition."""

import hashlib
import sqlite3
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build import Json, create_database, rebuild_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.domains.products import (
    Language,
    import_product_preview,
    load_products,
    populate_families,
    populate_product_preview,
)
from sve_carddb.domains.registry.preview import plan_preview
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import read_yaml

from .product_fixtures import (
    LANGUAGES,
    envelope,
    family,
    first_record,
    inclusion,
    install,
    items,
    obj,
    product,
)
from .product_fixtures import product_root as product_root  # ruff: ignore[useless-import-alias] -- shared fixture
from .registry_preview_fixtures import BUILD, REVISION, evidence
from .registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- shared fixture dependency
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- shared fixture dependency

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build.database import Database
    from sve_carddb.domains.registry.review import Inputs

NAME = "products/family/BP02/001.yaml"
AUDIT = (
    "language",
    "text_unit",
    "source_record",
    "product_family",
    "card",
    "face",
    "printing",
    "card_int_id",
)


def test_confirmed_family_and_identity_are_one_graph(
    product_root: Path, inputs: Inputs
) -> None:
    plan = plan_preview(product_root, evidence(inputs, en=False), regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    before = {p: p.read_bytes() for p in product_root.rglob("*.yaml")}
    with create_database(compile_build()) as db:
        import_product_preview(
            db,
            catalog,
            plan,
            build=BUILD,
            authored_revision=REVISION,
            languages=LANGUAGES,
        )
        assert len(db.rows("product_family")) == 3
        assert len(db.rows("text_unit")) == 1
        assert len(db.rows("printing")) == 2
        assert {row.values["int_id"] for row in db.rows("card_int_id")} == {
            20001,
            20002,
        }
        expected = {
            r.data.id: r.data.home_set_id
            for r in plan.included("printing")
            if isinstance(r.data, PrintingData)
        }
        assert {
            row.values["id"]: row.values["home_set_id"] for row in db.rows("printing")
        } == expected
        text = db.rows("text_unit")[0].values
        digest = hashlib.sha256(b"Synthetic family").hexdigest()
        assert text == {
            "id": "t:ja:" + digest[:16],
            "lang": "ja",
            "text": "Synthetic family",
            "content_hash": "sha256:" + digest,
        }
        sources = {
            row.values["authored_path"]: row.values
            for row in db.rows("source_record")
            if row.values["parser_version"] == "product-authored-v3"
        }
        assert not db.rows("decision")
        assert not db.rows("decision_source")
        for shard in catalog.shards:
            assert sources["authored/" + shard.path]["sha256"] == shard.content_hash
            assert sources["authored/" + shard.path]["authored_revision"] == REVISION
        assert not db.rows("product")
        assert not db.rows("printing_product")
    assert before == {p: p.read_bytes() for p in before}


def test_late_identity_failure_rolls_back_families_too(
    product_root: Path, inputs: Inputs
) -> None:
    plan = plan_preview(product_root, evidence(inputs), regions=("jp", "en"))
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build()) as db:
        with pytest.raises(KeyError, match="art"):
            import_product_preview(
                db,
                catalog,
                plan,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
        for table in AUDIT:
            assert not db.rows(table)


@pytest.mark.parametrize("parent", ["missing", "proposed"])
def test_missing_or_proposed_parent_keeps_existing_preview_rejection(
    product_root: Path, inputs: Inputs, parent: str
) -> None:
    path = product_root / NAME
    if parent == "missing":
        path.unlink()
    else:
        raw = obj(read_yaml(path))
        first_record(raw)["state"] = "proposed"
        install(product_root, NAME, raw)
    plan = plan_preview(product_root, evidence(inputs), regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build()) as db:
        with pytest.raises(ValueError, match="Missing product_family"):
            import_product_preview(
                db,
                catalog,
                plan,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
        for table in AUDIT:
            assert not db.rows(table)


def test_proposed_stays_a_candidate_without_adopted_parent(product_root: Path) -> None:
    raw = obj(read_yaml(product_root / NAME))
    first_record(raw)["state"] = "proposed"
    install(product_root, NAME, raw)
    catalog = load_products(product_root, registry=load_registry(product_root))
    assert any(
        obj(item)["disposition"] == "candidate"
        for item in items(catalog.report()["records"])
    )
    with create_database(compile_build()) as db, db.transaction():
        populate_families(
            db, catalog, build=BUILD, authored_revision=REVISION, languages=LANGUAGES
        )
        assert "BP02" not in {r.values["id"] for r in db.rows("product_family")}
        assert len(db.rows("product_family")) == 2
        assert not db.rows("decision")
        assert len(db.rows("source_record")) == 3


@pytest.mark.parametrize("kind", ["product", "inclusion"])
def test_unsupported_db_projection_is_explicit_and_atomic(
    product_root: Path, inputs: Inputs, kind: str
) -> None:
    plan = plan_preview(product_root, evidence(inputs), regions=("jp",))
    install(product_root, "products/product/unassigned/001.yaml", envelope([product()]))
    if kind == "inclusion":
        printing = next(
            r.data
            for r in plan.snapshot.records.values()
            if isinstance(r.data, PrintingData) and r.data.region == "jp"
        )
        install(
            product_root,
            "products/inclusion/unassigned/001.yaml",
            envelope([inclusion(printing.id)]),
        )
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build()) as db:
        with pytest.raises(
            ValueError,
            match="Product/inclusion DB projection is not implemented: "
            + ("product" if kind == "product" else "printing_product"),
        ):
            import_product_preview(
                db,
                catalog,
                plan,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
        for table in AUDIT:
            assert not db.rows(table)


@pytest.mark.parametrize("revision", ["", "a" * 7, "Z" * 40, "a" * 40 + "\n"])
def test_revision_is_full_and_exact(
    product_root: Path, inputs: Inputs, revision: str
) -> None:
    plan = plan_preview(product_root, evidence(inputs), regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build()) as db:
        with pytest.raises(ValueError, match="full Git commit"):
            import_product_preview(
                db,
                catalog,
                plan,
                build=BUILD,
                authored_revision=revision,
                languages=LANGUAGES,
            )
        assert not db.rows("language")


def test_inconsistent_registry_input_cannot_be_composed(
    product_root: Path, inputs: Inputs
) -> None:
    plan = plan_preview(product_root, evidence(inputs), regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build()) as db:
        with pytest.raises(ValueError, match="same registry input"):
            import_product_preview(
                db,
                replace(catalog, registry_index_content=b"different"),
                plan,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
        assert not db.rows("language")


@pytest.mark.parametrize("language", ["ja", "fr"])
def test_language_must_be_registered(product_root: Path, language: str) -> None:
    raw = obj(read_yaml(product_root / NAME))
    obj(obj(first_record(raw)["data"])["name"])["lang"] = language
    install(product_root, NAME, raw)
    catalog = load_products(product_root, registry=load_registry(product_root))
    with create_database(compile_build()) as db:
        with (
            pytest.raises(ValueError, match="language is not registered"),
            db.transaction(),
        ):
            populate_families(
                db,
                catalog,
                build=BUILD,
                authored_revision=REVISION,
                languages=() if language == "ja" else LANGUAGES,
            )
        assert not db.rows("language")
        assert not db.rows("product_family")


def test_existing_language_and_exact_text_are_reused(product_root: Path) -> None:
    catalog = load_products(product_root, registry=load_registry(product_root))
    digest = hashlib.sha256(b"Synthetic family").hexdigest()
    with create_database(compile_build()) as db:
        with db.transaction():
            db.insert(
                "language",
                {"code": "ja", "display_name": "Japanese", "fallback_order": Json([])},
            )
            db.insert(
                "text_unit",
                {
                    "id": "t:ja:" + digest[:16],
                    "lang": "ja",
                    "text": "Synthetic family",
                    "content_hash": "sha256:" + digest,
                },
            )
        with db.transaction():
            populate_families(
                db,
                catalog,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
        assert len(db.rows("language")) == 1
        assert len(db.rows("text_unit")) == 1


@pytest.mark.parametrize("collision", ["short_id", "full_hash"])
def test_each_text_collision_compares_exact_bytes(
    product_root: Path, collision: str
) -> None:
    catalog = load_products(product_root, registry=load_registry(product_root))
    digest = hashlib.sha256(b"Synthetic family").hexdigest()
    with create_database(compile_build()) as db:
        with db.transaction():
            db.insert(
                "language",
                {"code": "ja", "display_name": "Japanese", "fallback_order": Json([])},
            )
            db.insert(
                "text_unit",
                {
                    "id": "t:ja:"
                    + (digest[:16] if collision == "short_id" else "0" * 16),
                    "lang": "ja",
                    "text": "Different exact bytes",
                    "content_hash": "sha256:"
                    + (digest if collision == "full_hash" else "0" * 64),
                },
            )
        with pytest.raises(ValueError, match="collision"), db.transaction():
            populate_families(
                db,
                catalog,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
        assert len(db.rows("text_unit")) == 1
        assert not db.rows("source_record")
        assert not db.rows("product_family")


def test_names_are_not_normalized_and_languages_do_not_share_keys(
    product_root: Path,
) -> None:
    install(
        product_root,
        "products/family/BP02/001.yaml",
        envelope([family("BP02", text="é")]),
    )
    install(
        product_root,
        "products/family/PR/001.yaml",
        envelope([family("PR", text="e\u0301")]),
    )
    other = family("GF01", text="é")
    obj(obj(other["data"])["name"])["lang"] = "fr"
    install(product_root, "products/family/GF01/001.yaml", envelope([other]))
    catalog = load_products(product_root, registry=load_registry(product_root))
    with create_database(compile_build()) as db, db.transaction():
        populate_families(
            db,
            catalog,
            build=BUILD,
            authored_revision=REVISION,
            languages=(
                *LANGUAGES,
                Language(code="fr", fallback_order=(), display_name="French"),
            ),
        )
        assert len(db.rows("text_unit")) == 3


def test_language_config_conflict_does_not_overwrite(product_root: Path) -> None:
    catalog = load_products(product_root, registry=load_registry(product_root))
    with create_database(compile_build()) as db:
        with db.transaction():
            db.insert(
                "language",
                {
                    "code": "ja",
                    "display_name": "Existing name",
                    "fallback_order": Json([]),
                },
            )
        with pytest.raises(ValueError, match="Conflicting language"), db.transaction():
            populate_families(
                db,
                catalog,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
        assert db.rows("language")[0].values["display_name"] == "Existing name"


def test_final_verification_failure_rolls_back_both_importers(
    product_root: Path, inputs: Inputs
) -> None:
    plan = plan_preview(product_root, evidence(inputs), regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    with create_database(compile_build()) as db:
        with pytest.raises(sqlite3.IntegrityError), db.transaction():  # ruff: ignore[pytest-raises-with-multiple-statements] -- exercise commit-time verification after both importers write
            populate_product_preview(
                db,
                catalog,
                plan,
                build=BUILD,
                authored_revision=REVISION,
                languages=LANGUAGES,
            )
            db.insert(
                "card_int_id",
                {
                    "printing_id": "missing",
                    "int_id": 20123,
                },
            )
        for table in AUDIT:
            assert not db.rows(table)


def test_rebuild_failure_preserves_old_destination(
    product_root: Path, inputs: Inputs, tmp_path: Path
) -> None:
    plan = plan_preview(product_root, evidence(inputs), regions=("jp", "en"))
    catalog = load_products(product_root, registry=plan.snapshot)
    destination = tmp_path / "build.sqlite"
    destination.write_bytes(b"old destination")

    def populate(db: Database) -> None:
        populate_product_preview(
            db,
            catalog,
            plan,
            build=BUILD,
            authored_revision=REVISION,
            languages=LANGUAGES,
        )

    with pytest.raises(KeyError, match="art"):
        rebuild_database(compile_build(), destination, populate)
    assert destination.read_bytes() == b"old destination"


def test_rebuild_success_uses_the_caller_transaction(
    product_root: Path, inputs: Inputs, tmp_path: Path
) -> None:
    plan = plan_preview(product_root, evidence(inputs), regions=("jp",))
    catalog = load_products(product_root, registry=plan.snapshot)
    destination = tmp_path / "build.sqlite"

    def populate(db: Database) -> None:
        populate_product_preview(
            db,
            catalog,
            plan,
            build=BUILD,
            authored_revision=REVISION,
            languages=LANGUAGES,
        )
        assert len(db.rows("printing")) == 2

    rebuild_database(compile_build(), destination, populate)
    assert destination.read_bytes().startswith(b"SQLite format 3")

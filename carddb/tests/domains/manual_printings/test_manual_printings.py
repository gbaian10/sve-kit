"""Synthetic unlisted printings: allocation lane, authored rules and build staging."""

import sqlite3
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import canonical
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.digital.name_policies.owners import publication_owners
from sve_carddb.domains.manual_printings import (
    ManualSpec,
    load_manual_printings,
    plan_manual_files,
    populate_manual_printings,
    reference_class,
)
from sve_carddb.domains.manual_printings.records import AREA
from sve_carddb.domains.products import load_products
from sve_carddb.domains.registry.records import PrintingData
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import Index, read_yaml, write_files
from sve_carddb.workflows.manual_printings_commands import app

from ...support.product_fixtures import envelope, family, install, write_yaml
from ...support.registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose synthetic pytest fixture
from ..registry.test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose dependency of the synthetic registry fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.domains.manual_printings import ManualSnapshot

    from ...support.database_template import DatabaseTemplate
    from ...support.shared_case_fixtures import TextCaseTemplate

REVISION = "a" * 40
IMAGE: dict[str, JsonValue] = {
    "sha256": "sha256:" + "3" * 64,
    "width": 918,
    "height": 1282,
    "origin": "maintainer_photo",
    "source_url": None,
    "approved": True,
}


def image(**changes: JsonValue) -> dict[str, JsonValue]:
    return {**IMAGE, **changes}


def spec(**changes: JsonValue) -> ManualSpec:
    base: dict[str, JsonValue] = {
        "slug": "synthetic-ep",
        "region": "jp",
        "home_set_id": "EP",
        "card_type": "ep",
        "card_no": None,
        "card_no_state": "none",
        "printed_code": "2024合成-A",
        "name": {"lang": "ja", "text": "合成EP"},
        "release_kind": "event",
        "references": ["https://shadowverse-evolve.com/news/post-synthetic/"],
        "image": IMAGE,
    }
    return ManualSpec.model_validate_json(canonical(base | changes))


def snapshot(root: Path) -> ManualSnapshot:
    registry = load_registry(root)
    return load_manual_printings(
        root,
        next_unlisted_int_id=registry.files.index().unlisted_cursors(),
        registry=registry,
    )


def register(root: Path, *specs: ManualSpec) -> ManualSnapshot:
    registry = load_registry(root)
    plan = plan_manual_files(root, snapshot(root), registry.files.index(), specs)
    write_files(plan.files)
    return snapshot(root)


def test_append_allocates_unlisted_lane_and_keeps_official_cursors(
    registry_root: Path,
) -> None:
    before = load_registry(registry_root).files.index()
    assert snapshot(registry_root).records == {}
    result = register(
        registry_root,
        spec(),
        spec(
            slug="synthetic-en-sep",
            region="en",
            card_type="sep",
            name={"lang": "en", "text": "Synthetic SEP"},
            printed_code=None,
            release_kind="basic",
            references=[],
            image=image(width=1282, height=918),
        ),
    )
    printings = result.printings()
    assert [(item.region, item.int_id) for item in printings] == [
        ("en", 95000),
        ("jp", 55000),
    ]
    after = load_registry(registry_root).files.index()
    assert after.next_int_id == before.next_int_id
    assert after.next_unlisted_int_id == {"en": 95001, "jp": 55001}
    assert [shard.path for shard in result.shards] == [AREA + "/EP/001.yaml"]
    jp = next(item for item in printings if item.region == "jp")
    assert jp.id.startswith("p:")
    assert jp.card_id.startswith("c:")
    assert jp.face_id != jp.card_id
    # A second append continues the lane in a new shard and rejects a reused key.
    second = register(registry_root, spec(slug="synthetic-ep-2", image=None))
    assert {item.int_id for item in second.printings()} == {55000, 55001, 95000}
    assert len(second.shards) == 2
    with pytest.raises(ValueError, match="slugs must be new"):
        register(registry_root, spec())


def test_cli_appends_and_checks_a_reviewed_spec(registry_root: Path) -> None:
    install(registry_root, "products/family/EP/001.yaml", envelope([family("EP")]))
    path = registry_root / "spec.json"
    path.write_bytes(canonical([spec(image=None).model_dump(mode="json")]))
    runner = CliRunner()
    result = runner.invoke(
        app, ["append", "--authored", str(registry_root), "--spec", str(path)]
    )
    assert result.exit_code == 0, result.output
    assert snapshot(registry_root).printings()[0].int_id == 55000
    checked = runner.invoke(app, ["check", "--authored", str(registry_root)])
    assert checked.exit_code == 0, checked.output
    assert '"printings":1' in checked.output


@pytest.mark.parametrize("state", ["missing", "proposed"])
def test_cli_rejects_unconfirmed_family_before_writing(
    registry_root: Path, state: str
) -> None:
    (registry_root / "products").mkdir()
    if state == "proposed":
        record = family("EP")
        record["state"] = "proposed"
        install(registry_root, "products/family/EP/001.yaml", envelope([record]))
    before = (registry_root / "ids" / "index.yaml").read_bytes()
    path = registry_root / "spec.json"
    path.write_bytes(canonical([spec(image=None).model_dump(mode="json")]))
    runner = CliRunner()
    result = runner.invoke(
        app, ["append", "--authored", str(registry_root), "--spec", str(path)]
    )
    assert result.exit_code != 0
    assert isinstance(result.exception, ValueError)
    assert "confirmed product family" in str(result.exception)
    assert (registry_root / "ids" / "index.yaml").read_bytes() == before
    assert not (registry_root / AREA).exists()
    register(registry_root, spec(image=None))
    checked = runner.invoke(app, ["check", "--authored", str(registry_root)])
    assert checked.exit_code != 0
    assert isinstance(checked.exception, ValueError)
    assert "confirmed product family" in str(checked.exception)


@pytest.mark.parametrize(("region", "last"), [("jp", 59999), ("en", 99999)])
def test_batch_exhaustion_preserves_both_allocation_lanes(
    registry_root: Path, region: str, last: int
) -> None:
    registry = load_registry(registry_root)
    before = registry.files.index()
    cursors = before.unlisted_cursors() | {region: last}
    index = Index.model_validate(
        before.model_dump() | {"next_unlisted_int_id": cursors}
    )
    specs = tuple(
        spec(
            slug=slug,
            region=region,
            name={"lang": "ja" if region == "jp" else "en", "text": "Synthetic EP"},
            image=None,
        )
        for slug in ("synthetic-last", "synthetic-overflow")
    )
    with pytest.raises(ValueError, match="range exhausted"):
        plan_manual_files(registry_root, snapshot(registry_root), index, specs)
    assert index.next_unlisted_int_id == cursors
    assert index.next_int_id == before.next_int_id
    assert load_registry(registry_root).files.index() == before
    assert not (registry_root / AREA).exists()


@pytest.mark.parametrize(
    ("changes", "message"),
    [
        ({"card_no": "PR-999"}, "card_no is empty exactly"),
        ({"home_set_id": "PR"}, "must belong to the EP family"),
        ({"card_no_state": "official"}, "card_no is empty exactly"),
        ({"release_kind": "basic"}, "Basic EP/SEP have no references"),
        ({"references": []}, "need at least one"),
        (
            {"references": ["https://example.invalid/shop/ep"]},
            "not an official site",
        ),
        (
            {"references": ["https://x.com/someone_else/status/1"]},
            "official X account",
        ),
        ({"references": ["http://shadowverse-evolve.com/news/"]}, "plain https"),
        ({"name": {"lang": "en", "text": "Synthetic"}}, "language must match"),
        ({"card_type": "sep"}, "orientation"),
        ({"image": image(width=918, height=918)}, "orientation"),
        ({"image": image(approved=False)}, "approved"),
        (
            {"image": image(origin="official_image")},
            "requires its source URL",
        ),
        (
            {"image": image(source_url="https://shadowverse-evolve.com/x.png")},
            "maintainer photo has no source URL",
        ),
        ({"printed_code": " 2024"}, "surrounding whitespace"),
        ({"slug": "Synthetic"}, "pattern"),
    ],
)
def test_spec_rules_reject_inconsistent_records(
    registry_root: Path, changes: dict[str, JsonValue], message: str
) -> None:
    with pytest.raises(ValueError, match=message):
        register(registry_root, spec(**changes))


@pytest.mark.parametrize(
    ("url", "expected"),
    [
        ("https://shadowverse-evolve.com/news/post-234/", "official_site"),
        ("https://en.bushiroad.com/bcs2526/prizes/", "official_site"),
        ("https://x.com/ShadowverseRAGE/status/1856881662711533604", "official_x"),
        ("https://www.youtube.com/watch?v=HfunwwLkw2I", "official_video"),
        ("https://youtu.be/HfunwwLkw2I", "official_video"),
    ],
)
def test_reference_class_accepts_official_sources(url: str, expected: str) -> None:
    assert reference_class(url) == expected


@pytest.mark.parametrize(
    "url",
    [
        "https://shop.shadowverse-evolve.com/",
        "https://www.youtube.com/channel/UCxxxx",
        "https://user:pw@shadowverse-evolve.com/",
        "https://shadowverse-evolve.com/#frag",
        "https://x.com/shadowverse_ev",
        "https://shadowverse-evolve.com:/news/",
        " https://shadowverse-evolve.com/news/",
        "https://shadowverse-evolve.com/ne\nws/",
    ],
)
def test_reference_class_rejects_other_sources(url: str) -> None:
    with pytest.raises(ValueError, match="Reference"):
        reference_class(url)


def _edit_shard(root: Path, edit: str, value: JsonValue) -> None:
    path = root / AREA / "EP" / "001.yaml"
    raw = read_yaml(path)
    assert isinstance(raw, dict)
    records = raw["records"]
    assert isinstance(records, list)
    record = records[0]
    assert isinstance(record, dict)
    data = record["data"]
    assert isinstance(data, dict)
    if edit == "owner":
        record[edit] = value
    else:
        data[edit] = value
    write_yaml(path, raw)


@pytest.mark.parametrize(
    ("edit", "value", "message"),
    [
        ("int_id", 20005, "out-of-range"),
        ("int_id", 55001, "high-water"),
        ("id", "p:" + "0" * 32, "data:value_error"),
        ("home_set_id", "BP02", "home_set_id:value_error"),
        ("owner", "PR", "records.0:value_error"),
        ("extra", 1, "extra_forbidden"),
    ],
)
def test_loader_rejects_tampered_shards(
    registry_root: Path, edit: str, value: JsonValue, message: str
) -> None:
    register(registry_root, spec())
    _edit_shard(registry_root, edit, value)
    with pytest.raises(ValueError, match=message):
        snapshot(registry_root)


def test_loader_rejects_cursor_and_registry_collisions(registry_root: Path) -> None:
    register(registry_root, spec())
    registry = load_registry(registry_root)
    index = registry.files.index()
    with pytest.raises(ValueError, match="high-water"):
        load_manual_printings(
            registry_root,
            next_unlisted_int_id={"en": 95000, "jp": 55000},
            registry=registry,
        )
    official = next(
        record.data.id
        for record in registry.records.values()
        if isinstance(record.data, PrintingData)
    )
    data = snapshot(registry_root).printings()[0]
    _edit_shard(registry_root, "id", official)
    # The slug recipe already rejects a foreign ID before the registry comparison.
    with pytest.raises(ValueError, match="value_error"):
        snapshot(registry_root)
    _edit_shard(registry_root, "id", data.id)
    # Moving the registry cursor backwards is caught by the index model itself.
    with pytest.raises(ValueError, match="cursors disagree"):
        Index(next_unlisted_int_id={"en": 95000, "jp": 54999})
    assert index.next_unlisted_int_id == {"en": 95000, "jp": 55001}


def test_shard_bytes_are_stable_block_yaml(registry_root: Path) -> None:
    register(registry_root, spec(image=None))
    path = registry_root / AREA / "EP" / "001.yaml"
    text = path.read_text(encoding="utf-8")
    assert text.startswith("format: 1\nkind: manual_printings_shard\nrecords:\n")
    # ruamel writes explicit nulls as bare keys; both stay present rather than omitted.
    assert "\n      image:\n" in text
    assert "\n      card_no:\n" in text
    before = path.read_bytes()
    register(registry_root, spec(slug="synthetic-next", image=None))
    assert path.read_bytes() == before


def test_oversized_append_is_rejected_before_any_write(registry_root: Path) -> None:
    before = (registry_root / "ids" / "index.yaml").read_bytes()
    with pytest.raises(ValueError, match="smaller than one MiB"):
        register(registry_root, spec(name={"lang": "ja", "text": "x" * 1048576}))
    assert (registry_root / "ids" / "index.yaml").read_bytes() == before
    assert not (registry_root / AREA).exists()


@pytest.mark.parametrize("card_type", ["ep", "sep"])
def test_populate_stages_textless_provisional_card(
    registry_root: Path, t0_database_template: DatabaseTemplate, card_type: str
) -> None:
    result = register(
        registry_root,
        spec(
            card_type=card_type,
            image=None,
            references=[
                "https://shadowverse-evolve.com/news/post-synthetic/",
                "https://x.com/shadowverse_ev/status/1715598409107939492",
            ],
        ),
    )
    context = BuildContext.from_inputs(REVISION, {"synthetic": True})
    with t0_database_template.copy() as db:
        with db.transaction():
            db.insert(
                "product_family",
                {
                    "id": "EP",
                    "code": "ep",
                    "public_code": "EP",
                    "kind": "promo",
                    "name_unit_id": "text",
                },
            )
            populate_manual_printings(
                db, result, authored_revision=REVISION, build=context
            )
        data = result.printings()[0]
        printing = next(
            row.values for row in db.rows("printing") if row.values["id"] == data.id
        )
        assert not printing["card_no"]
        assert printing["card_no_state"] == "none"
        assert printing["catalog_state"] == "unlisted"
        assert printing["manual_type_code"] == card_type
        assert printing["decklog_available"] is False
        assert printing["printed_code"] == "2024合成-A"
        assert printing["rarity_raw"] == "-"
        name = next(
            row.values["text"]
            for row in db.rows("text_unit")
            if row.values["id"] == printing["manual_name_unit_id"]
        )
        assert name == "合成EP"
        assert {
            row.values["url"]
            for row in db.rows("printing_reference")
            if row.values["printing_id"] == data.id
        } == set(data.references)
        assert any(
            row.values["int_id"] == 55000 and row.values["printing_id"] == data.id
            for row in db.rows("card_int_id")
        )
        card = next(
            row.values for row in db.rows("card") if row.values["id"] == data.card_id
        )
        assert card["identity_state"] == "provisional"
        # The manual name never becomes official text, a current or a rules name.
        assert not any(
            row.values["face_id"] == data.face_id for row in db.rows("face_revision")
        )
        assert not any(
            row.values["face_id"] == data.face_id for row in db.rows("face_rules_name")
        )
        with pytest.raises(sqlite3.IntegrityError, match="CHECK"), db.transaction():
            db.update(
                "printing",
                {"id": "printing"},
                {"card_no": "", "card_no_state": "none"},
            )
        with pytest.raises(sqlite3.IntegrityError, match="CHECK"), db.transaction():
            db.update(
                "printing",
                {"id": "printing"},
                {"manual_name_unit_id": printing["manual_name_unit_id"]},
            )
        with pytest.raises(sqlite3.IntegrityError, match="CHECK"), db.transaction():
            db.update("printing", {"id": "printing"}, {"manual_type_code": card_type})


def test_populate_requires_registered_family(
    registry_root: Path, t0_database_template: DatabaseTemplate
) -> None:
    result = register(registry_root, spec())
    context = BuildContext.from_inputs(REVISION, {"synthetic": True})
    with (
        t0_database_template.copy() as db,
        pytest.raises(ValueError, match="Missing product_family parent"),
        db.transaction(),
    ):
        populate_manual_printings(db, result, authored_revision=REVISION, build=context)


def test_index_without_unlisted_cursors_is_rejected(registry_root: Path) -> None:
    path = registry_root / "ids" / "index.yaml"
    index = Index.model_validate(read_yaml(path))
    raw = index.model_dump(mode="json")
    del raw["next_unlisted_int_id"]
    write_yaml(path, raw)
    with pytest.raises(ValueError, match="envelope"):
        load_registry(registry_root)


def test_historical_v1_index_reads_without_an_unlisted_lane() -> None:
    raw: dict[str, JsonValue] = {
        "format": 2,
        "kind": "registry_index",
        "allocation_policy": "region-ranges-2026-09-28-v1",
        "next_int_id": {"en": 67421, "jp": 27675},
    }
    index = Index.model_validate(raw)
    # The identity-transition basis hash covers this dump; the lane must not appear.
    assert index.model_dump(mode="json") == raw
    with pytest.raises(ValueError, match="current allocation policy"):
        index.unlisted_cursors()
    with pytest.raises(ValueError, match="current allocation policy"):
        Index.model_validate(raw | {"next_unlisted_int_id": {"en": 95000, "jp": 55000}})
    with pytest.raises(ValueError, match="current allocation policy"):
        Index.model_validate(
            {
                k: v
                for k, v in Index().model_dump(mode="json").items()
                if k != "next_unlisted_int_id"
            }
            | {"next_unlisted_int_id": None}
        )


def test_unlisted_faces_are_not_name_owners(
    default_text_case: TextCaseTemplate, tmp_path: Path
) -> None:
    case = default_text_case.copy(tmp_path / "synthetic")
    install(case.root, "products/family/EP/001.yaml", envelope([family("EP")]))
    case.catalog = load_products(case.root, registry=case.identity.snapshot)
    manual = register(case.root, spec(image=None))
    context = BuildContext.from_inputs(REVISION, {"synthetic": True})
    with create_database(compile_build(("en", "related", "correction"))) as db:
        with db.transaction():
            case.stage(db)
            case.compose().populate(
                db, build=case.context(), vocabulary=case.vocabulary, published=()
            )
            populate_manual_printings(
                db, manual, authored_revision=REVISION, build=context
            )
        owners = publication_owners(db, case.plan)
        manual_ids = {data.id for data in manual.printings()}
        assert owners
        assert not any(owner.identifier in manual_ids for owner, _ in owners)

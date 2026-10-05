"""Frozen page face maps, image DB projection and inseparable bundle publication."""

import hashlib
from dataclasses import dataclass, replace
from datetime import date
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import BuildContext, InputRecord
from sve_carddb.extract.compare_jp import legacy_projection
from sve_carddb.extract.official_jp import extract_card
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.image_assets import (
    build_regional_assets,
    plan_regional_images,
    populate_assets,
)
from sve_carddb.image_variants import DEFAULT_RECIPE
from sve_carddb.manifest import Kind
from sve_carddb.registry.build import build
from sve_carddb.registry.inputs import Mapping
from sve_carddb.registry.preview import FrozenJP, plan_preview, populate_preview
from sve_carddb.registry.records import PrintingData
from sve_carddb.registry.review import InitDecisions, Inputs
from sve_carddb.registry.storage import plan_files, write_files
from sve_carddb.source_archive import ArchiveStore, Scope, seal_batch
from sve_carddb.sources.official_jp import card_url

from .build_db_fixtures import rows
from .test_image_assets import frozen as frozen  # ruff: ignore[useless-import-alias] -- reuse only synthetic archive fixture
from .test_image_assets import roots
from .test_image_variants import png
from .test_registry_preview_archive import RAW
from .test_source_archive import _put, _resource

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import Source
    from sve_carddb.image_assets import ImageReference
    from sve_carddb.image_crops import ImageCrops
    from sve_carddb.registry.preview import PreviewPlan
    from sve_carddb.source_archive import Descriptor

REVISION = "a" * 40


@dataclass
class Staged:
    cards: FrozenSources
    plan: PreviewPlan
    context: BuildContext

    def parents(self, db: Database) -> InputRecord:
        values = rows()
        for name in ("decision", "language", "text_unit"):
            db.insert(name, values[name])
        required = {
            record.data.home_set_id
            for record in self.plan.included("printing")
            if isinstance(record.data, PrintingData)
        }
        for family in required:
            db.insert("product_family", values["product_family"] | {"id": family})
        return populate_preview(
            db, self.plan, authored_revision=REVISION, build=self.context
        )


@pytest.fixture
def staged(tmp_path: Path, frozen: FrozenSources) -> Staged:
    return make_staged(tmp_path, frozen)


def make_staged(
    tmp_path: Path,
    frozen: FrozenSources,
    front_src: bytes = b"/synthetic/0.png",
) -> Staged:
    data_root = frozen.root.parent / "data"
    store = ArchiveStore(
        data_root,
        data_root / "manifest/manifest.sqlite",
        data_root / "manifest/.lock",
        frozen.root,
        frozen.store_id,
    )
    front = RAW.replace(b"/synthetic/exact-image.png", front_src)
    back = front.replace(b"Synthetic card", b"Synthetic back").replace(
        front_src, b"/synthetic/1.png"
    )
    inner = back[
        back.index(b'<div class="cardlist-Detail_Box_Inner">') : back.index(b"\n<!--")
    ].removesuffix(b"</div>")
    double = front.replace(b"</div>\n<!--", inner + b"</div>\n<!--")
    cards = {"TEST-001": front, "TEST-002": double}
    for number, raw in cards.items():
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", raw, Kind.CARD),
            raw,
        )
    batch = seal_batch(store, scope=(Scope(provider="jp", kind="card"),))
    frozen_cards = FrozenSources(store.root, store.store_id, batch.batch_id)
    inputs = Inputs(
        jp={
            number: legacy_projection(extract_card(raw, number=number))
            for number, raw in cards.items()
        },
        en={},
        mapping=Mapping(targets={}, original_art=set(), reskins={}),
        decisions=InitDecisions(),
        as_of=date(2026, 9, 29),
        jp_hash="sha256:" + "0" * 64,
    )
    authored = tmp_path / "authored"
    write_files(plan_files(authored, build(inputs, {})))
    provider = FrozenJP(
        store.root,
        store.store_id,
        batch.batch_id,
        parser_version="identity-synthetic-v1",
    )
    plan = plan_preview(authored, provider, regions=("jp",))
    assert len(plan.included("printing")) == 2
    context = BuildContext.from_inputs(
        REVISION,
        {"synthetic.lock": b"synthetic"},
        {"image_recipe": DEFAULT_RECIPE.version},
    )
    return Staged(frozen_cards, plan, context)


def refs_for(staged: Staged) -> tuple[ImageReference, ...]:
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
        return plan_regional_images(db, staged.plan, staged.cards, region="jp")


@pytest.mark.parametrize(
    ("raw_src", "resolved"),
    [
        ("/synthetic/0 .png", "/synthetic/0%20.png"),
        ("/synthetic/0%20.png", "/synthetic/0%20.png"),
        ("/synthetic/Ⓢ .png", "/synthetic/%E2%93%88%20.png"),
        (
            "//SHADOWVERSE-EVOLVE.COM:443/synthetic/0 .png?b=2&a=1#image",
            "/synthetic/0%20.png?a=1&b=2",
        ),
    ],
)
def test_original_src_and_crawler_url_encoding_bind_the_same_archived_image(
    empty_crops: ImageCrops,
    tmp_path: Path,
    frozen: FrozenSources,
    raw_src: str,
    resolved: str,
) -> None:
    staged = make_staged(tmp_path, frozen, raw_src.encode())
    data_root = frozen.root.parent / "data"
    store = ArchiveStore(
        data_root,
        data_root / "manifest/manifest.sqlite",
        data_root / "manifest/.lock",
        frozen.root,
        frozen.store_id,
    )
    source_url = "https://shadowverse-evolve.com" + resolved
    data = png(80, 112)
    _put(store, _resource(source_url, "raw/encoded.png", data), data)
    batch = seal_batch(store, scope=(Scope(provider="jp", kind="image"),))
    images = FrozenSources(store.root, store.store_id, batch.batch_id)
    output = roots(tmp_path)
    encoded = build_regional_assets(images, output, region="jp", crops=empty_crops)
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
        refs = plan_regional_images(db, staged.plan, staged.cards, region="jp")
        selected = tuple(ref for ref in refs if ref.source_src_raw == raw_src)
        assert len(selected) == 2
        assert {ref.source_url for ref in selected} == {source_url}
        with db.transaction():
            populate_assets(db, encoded, selected, output.preview)
        assert len(db.rows("printing_image")) == 2
        assert len(db.rows("image_variant")) == 5
        asset = db.rows("image_asset")[0].values
        assert asset["source_src_raw"] == raw_src
        assert asset["source_url"] == source_url
        assert asset["availability"] == "available"
        assert asset["content_hash"] == "sha256:" + hashlib.sha256(data).hexdigest()


def test_original_src_double_faces_and_five_variants_are_projected(
    empty_crops: ImageCrops, tmp_path: Path, frozen: FrozenSources, staged: Staged
) -> None:
    output = roots(tmp_path)
    encoded = build_regional_assets(frozen, output, region="jp", crops=empty_crops)
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
        refs = plan_regional_images(db, staged.plan, staged.cards, region="jp")
        assert len(refs) == 3
        assert {ref.source_src_raw for ref in refs} == {
            "/synthetic/0.png",
            "/synthetic/1.png",
        }
        with db.transaction():
            uses = populate_assets(db, encoded, refs, output.preview)
        assert len(db.rows("printing_image")) == 3
        assert len(db.rows("image_asset")) == 2
        assert len(db.rows("image_variant")) == 10
        assert all(
            row.values["publication_state"] == "approved"
            for row in db.rows("image_asset")
        )
        assert {row.values["source_src_raw"] for row in db.rows("image_asset")} == {
            "/synthetic/0.png",
            "/synthetic/1.png",
        }
        assert all(row.values["is_original"] is False for row in db.rows("image_size"))
        assert {use.usage for use in uses} == {"jp_image_link", "jp_image_variant"}
        assert all(
            row.values["parser_version"] is None
            for row in db.rows("source_record")
            if row.values["kind"] != "authored"
        )
    assert encoded.report(refs)["mapped_printing_faces"] == 3


def test_adopted_source_map_is_used_instead_of_logical_face_order(
    staged: Staged,
) -> None:
    original = next(
        record
        for record in staged.plan.included("printing")
        if isinstance(record.data, PrintingData)
        and len(record.data.source_face_map) == 2
    )
    data = original.data
    assert isinstance(data, PrintingData)
    mapping = tuple(
        item.model_copy(update={"source_index": 1 - item.source_index})
        for item in data.source_face_map
    )
    revised = replace(
        original, data=data.model_copy(update={"source_face_map": mapping})
    )
    records = dict(staged.plan.snapshot.records)
    records[original.record_key] = revised
    plan = replace(staged.plan, snapshot=replace(staged.plan.snapshot, records=records))
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
        refs = plan_regional_images(db, plan, staged.cards, region="jp")
    by_face = {
        ref.face_id: ref.source_src_raw for ref in refs if ref.printing_id == data.id
    }
    assert by_face[data.source_face_map[0].face_id] == "/synthetic/1.png"
    assert by_face[data.source_face_map[1].face_id] == "/synthetic/0.png"


@pytest.mark.parametrize(
    "field", ["source_url", "source_src_raw", "card_no", "printing_id", "face_id"]
)
def test_each_binding_provenance_constraint_rolls_back(
    empty_crops: ImageCrops,
    tmp_path: Path,
    frozen: FrozenSources,
    staged: Staged,
    field: str,
) -> None:
    output = roots(tmp_path)
    encoded = build_regional_assets(frozen, output, region="jp", crops=empty_crops)
    refs = refs_for(staged)
    ref = refs[0]
    bad = {
        "source_url": replace(ref, source_url="wrong"),
        "source_src_raw": replace(ref, source_src_raw="wrong"),
        "card_no": replace(ref, card_no="wrong"),
        "printing_id": replace(ref, printing_id="wrong"),
        "face_id": replace(ref, face_id="wrong"),
    }[field]
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
        with pytest.raises(ValueError, match="binding"), db.transaction():
            populate_assets(db, encoded, (bad, *refs[1:]), output.preview)
        assert not db.rows("image_asset")
        assert not db.rows("image_variant")
        assert not db.rows("printing_image")


def test_absent_source_has_metadata_without_fake_path(
    empty_crops: ImageCrops, tmp_path: Path, frozen: FrozenSources, staged: Staged
) -> None:
    output = roots(tmp_path)
    encoded = build_regional_assets(frozen, output, region="jp", crops=empty_crops)
    refs = refs_for(staged)
    ref = replace(
        refs[0],
        source_src_raw="/synthetic/missing.png",
        source_url="https://shadowverse-evolve.com/synthetic/missing.png",
    )
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
            populate_assets(db, encoded, (ref,), output.preview)
        asset = db.rows("image_asset")[0].values
        assert asset["availability"] == "unfetched"
        assert asset["publication_state"] == "pending"
        assert asset["content_hash"] is None
        assert not db.rows("image_variant")
    assert encoded.report((ref,))["missing"] == [
        {"card_no": ref.card_no, "page_sha256": ref.page.sha256}
    ]


def test_shared_url_with_different_original_spelling_keeps_both(
    empty_crops: ImageCrops, tmp_path: Path, frozen: FrozenSources, staged: Staged
) -> None:
    output = roots(tmp_path)
    encoded = build_regional_assets(frozen, output, region="jp", crops=empty_crops)
    refs = refs_for(staged)
    ref = next(ref for ref in refs if ref.source_src_raw == "/synthetic/0.png")
    other = next(
        other
        for other in refs
        if other.source_url == ref.source_url and other.printing_id != ref.printing_id
    )
    other = replace(other, source_src_raw=other.source_url)
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
            populate_assets(db, encoded, (ref, other), output.preview)
        assert len(db.rows("image_asset")) == 2
        assert len(db.rows("image_variant")) == 10
        assert len({row.values["path"] for row in db.rows("image_variant")}) == 2


@pytest.mark.parametrize(
    "case", ["provider", "kind", "url", "evidence-metadata", "db-metadata", "printings"]
)
def test_each_page_source_gate_has_an_independent_counterexample(
    staged: Staged, case: str, monkeypatch: pytest.MonkeyPatch
) -> None:
    plan = staged.plan
    selected = next(iter(plan.included("printing")))
    data = selected.data
    assert isinstance(data, PrintingData)
    source_id = plan.evidence["jp", data.card_no].source.id
    original = staged.cards.read

    def altered(
        version: str, *, parser_version: str
    ) -> tuple[Source, bytes, Descriptor]:
        source, raw, descriptor = original(version, parser_version=parser_version)
        if version == source_id and case in {"provider", "kind", "url"}:
            descriptor = descriptor.model_copy(
                update={case: {"provider": "en", "kind": "image", "url": "wrong"}[case]}
            )
        return source, raw, descriptor

    monkeypatch.setattr(staged.cards, "read", altered)
    if case == "evidence-metadata":
        evidence = plan.evidence["jp", data.card_no]
        plan = replace(
            plan,
            evidence={
                **plan.evidence,
                ("jp", data.card_no): replace(
                    evidence,
                    source=evidence.source.model_copy(update={"etag": "wrong"}),
                ),
            },
        )
    elif case == "printings":
        plan = replace(
            plan,
            projections=tuple(
                replace(item, disposition="excluded")
                if item.record_key == selected.record_key
                else item
                for item in plan.projections
            ),
        )
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
        if case == "db-metadata":
            with db.transaction():
                db.update("source_record", {"id": source_id}, {"etag": "wrong"})
        expected = (
            "JP image descriptor differs from the card page"
            if case in {"provider", "kind", "url"}
            else "Image identity plan does not match the build printings"
            if case == "printings"
            else "JP image page provenance differs from the identity input"
        )
        with pytest.raises(ValueError, match="^" + expected + "$"):
            plan_regional_images(db, plan, staged.cards, region="jp")


@pytest.mark.parametrize(
    "case",
    ["source-index", "missing-face", "db-source", "page-source", "printing-number"],
)
def test_identity_plan_and_source_face_map_are_rechecked(
    staged: Staged, case: str
) -> None:
    original = next(
        record
        for record in staged.plan.included("printing")
        if isinstance(record.data, PrintingData)
        and len(record.data.source_face_map) == 2
    )
    data = original.data
    assert isinstance(data, PrintingData)
    plan = staged.plan
    with create_database(compile_build(("images",))) as db:
        with db.transaction():
            staged.parents(db)
        if case in {"source-index", "missing-face"}:
            mappings = data.source_face_map
            if case == "source-index":
                mappings = (
                    mappings[0].model_copy(update={"source_index": 99}),
                    *mappings[1:],
                )
            else:
                mappings = mappings[:1]
            records = dict(plan.snapshot.records)
            records[original.record_key] = replace(
                original, data=data.model_copy(update={"source_face_map": mappings})
            )
            plan = replace(plan, snapshot=replace(plan.snapshot, records=records))
        else:
            with db.transaction():
                if case == "printing-number":
                    db.update("printing", {"id": data.id}, {"card_no": "WRONG-001"})
                else:
                    source = next(
                        row.values["id"]
                        for row in db.rows("source_record")
                        if row.values["kind"] == "authored"
                    )
                    if case == "db-source":
                        db.update("printing", {"id": data.id}, {"source_id": source})
                    else:
                        db.update(
                            "printing_face",
                            {
                                "printing_id": data.id,
                                "face_id": data.source_face_map[0].face_id,
                            },
                            {"source_id": source},
                        )
        with pytest.raises(ValueError, match=r"source face map|provenance|identity"):
            plan_regional_images(db, plan, staged.cards, region="jp")

"""Non-publication identities survive only the two supported review-only changes."""

import json
import shutil
import sys
from copy import deepcopy
from typing import TYPE_CHECKING

import pytest

from sve_carddb.core.json import canonical, digest, object_value
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.storage import write as write_catalog
from sve_carddb.domains.registry import english_inventory
from sve_carddb.domains.registry.preview import plan_preview
from sve_carddb.domains.registry.records import (
    AllocationData,
    MappingReviewData,
    PrintingData,
)
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.transitions.renewal import (
    load_identity_registry,
    reference,
)
from sve_carddb.domains.registry.transitions.writer import plan, write
from sve_carddb.export.image_index import ImageBinding, ImageIndex
from sve_carddb.export.media_urls import image_path
from sve_carddb.images.variants import DEFAULT_RECIPE, SIZES
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import ArchiveStore, Scope, seal_batch
from sve_carddb.parse.pages.official_jp import card_url
from sve_carddb.workflows import authoring, offline
from sve_carddb.workflows.art_matching import art_inputs

from ...ingest.test_source_archive import _put, _resource, _store
from ...support.adoption_fixtures import REPO, envelope, record
from ...support.adoption_fixtures import write as write_document
from ...support.identity_transition_fixtures import merge_record as merge_record  # ruff: ignore[useless-import-alias] -- structurally valid unsupported repair fixture
from ...support.product_fixtures import envelope as product_envelope
from ...support.product_fixtures import family, install
from .test_art_extension import art_case as art_case  # ruff: ignore[useless-import-alias] -- shared small sealed registry fixture
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- dependency of case
from .test_registry_preview_en_sources import frozen_registry as frozen_registry  # ruff: ignore[useless-import-alias] -- dependency of case
from .test_review_renewal import case as case  # ruff: ignore[useless-import-alias] -- dependency of art_case

if TYPE_CHECKING:
    from pathlib import Path
    from typing import Any

    from pydantic import JsonValue

    from .test_art_extension import ArtCase


def test_both_renewals_preserve_jp_identity_and_never_expose_approvals(
    art_case: ArtCase,
) -> None:
    root = art_case.base.authored
    before = load_identity_registry(root)
    projected = plan_preview(
        root, art_case.base.provider, regions=("jp",), snapshot=before
    )
    write(root, plan(art_case.recipe, art_case.base.decisions))
    write(root, plan(art_case.recipe, art_case.decisions))
    after = load_identity_registry(root)
    assert before.records == after.records
    assert not after.transitions
    assert not after.transition_uses
    assert all(
        record.kind not in {"art", "region_mapping_review"}
        for record in after.records.values()
    )
    assert (
        plan_preview(root, art_case.base.provider, regions=("jp",), snapshot=after)
        == projected
    )
    with pytest.raises(ValueError, match="effective projection support"):
        load_registry(root)


@pytest.mark.parametrize(
    "change",
    [
        "deactivate",
        "repair",
        "route",
        "revert",
        "card",
        "printing",
        "face",
        "art_identity",
        "art_remove",
        "unknown",
        "before",
        "dependency",
    ],
)
def test_unsupported_or_damaged_changes_are_rejected(
    art_case: ArtCase, change: str, merge_record: dict[str, Any]
) -> None:
    root = art_case.base.authored
    decision = (
        art_case.base.decisions
        if change not in {"art_remove", "art_identity"}
        else art_case.decisions
    )
    planned = plan(art_case.recipe, decision)
    assert planned.shard is not None
    value = deepcopy(
        planned.shard.envelope().model_dump(mode="json", exclude_computed_fields=True)
    )
    record = value["records"][0]
    update = record["updates"][0]
    if change == "deactivate":
        update["after"] = None
    elif change in {"card", "printing", "face"}:
        original = next(
            r.entry() for r in load_registry(root).records.values() if r.kind == change
        )
        update["target_key"] = original.record_key
        update["before"] = reference(original, None).model_dump(mode="json")
        update["after"] = original.model_dump(mode="json", exclude_computed_fields=True)
        field, changed = {
            "card": ("identity_state", "retired"),
            "printing": ("variant_key", "alternate"),
            "face": ("side", "back"),
        }[change]
        update["after"]["data"][field] = changed
    elif change == "repair":
        record["repairs"] = merge_record["repairs"]
    elif change == "route":
        record["routes"] = [
            {
                "printing_id": next(
                    u["after"]["data"]["id"]
                    for u in merge_record["updates"]
                    if u["after"]["kind"] == "printing"
                ),
                "before": {
                    "canonical": {"namespace": "official", "route_key": "BP02-071"},
                    "aliases": [],
                },
                "after": {
                    "canonical": {"namespace": "official", "route_key": "PR-001"},
                    "aliases": [],
                },
            }
        ]
    elif change == "revert":
        record["action"] = "revert"
        record["reverts"] = {
            "record_key": "synthetic",
            "record_hash": "sha256:" + "0" * 64,
        }
    elif change == "art_identity":
        update["after"]["data"]["classification"] = "alternate"
    elif change == "art_remove":
        update["after"]["data"]["uses"] = update["after"]["data"]["uses"][1:]
    elif change == "unknown":
        update["after"]["kind"] = "unknown"
    elif change == "before":
        update["before"]["record_hash"] = "sha256:" + "0" * 64
    else:
        context = record["review_context"]["context"]
        config = json.loads(context["configuration"])
        config["dependencies"][0]["sha256"] = "sha256:" + "0" * 64
        context["configuration"] = canonical(config).decode()
    target = root / planned.shard.path
    target.parent.mkdir(parents=True)
    target.write_bytes(canonical(value))
    with pytest.raises(
        ValueError, match=r"(?i)identity|renewal|transition|only|art|revert"
    ):
        load_identity_registry(root)


def test_public_author_inspection_is_unchanged_without_archive(  # ruff: ignore[too-many-locals] -- two complete synthetic cards and vocabulary exercise real preparation without mocks
    art_case: ArtCase, tmp_path: Path
) -> None:
    root = art_case.base.authored
    repo = art_case.recipe.repo
    shutil.copytree(REPO / "dsl", repo / "dsl")
    (root / "translations/glossary").mkdir(parents=True)
    (root / "products/identities").mkdir(parents=True)
    write_document(
        root,
        "translations/parameter-rules/current.yaml",
        {
            "format": 2,
            "kind": "template_parameter_rules",
            "rules": [],
        },
    )
    write_document(
        root,
        "catalog/adoptions/languages/001.yaml",
        envelope(
            [
                record(
                    "language_adoption",
                    {"code": "ja"},
                    {"display_name": "Japanese", "fallback_order": ["en"]},
                ),
                record(
                    "language_adoption",
                    {"code": "en"},
                    {"display_name": "English", "fallback_order": ["ja"]},
                ),
            ]
        ),
    )
    for name in ("BP02", "PR", "GF01"):
        install(
            root, f"products/family/{name}/001.yaml", product_envelope([family(name)])
        )
    store = _store(tmp_path / "public-source")
    old_sources = FrozenSources(
        art_case.recipe.archive, store.store_id, art_case.base.provider.jp.batch_id
    )
    for number in ("BP02-071", "PR-001"):
        version = next(
            item.source_version_id
            for item in old_sources.current_versions
            if item.url == card_url(number)
        )
        _, raw, _ = old_sources.read(version, parser_version="synthetic-public")
        complete = raw.replace(
            b'<div class="speech"></div>',
            (
                '<div class="speech"></div><div class="illustrator"><span class="heading">Synthetic artist</span><span class="name">'
                + number
                + "</span></div>"
            ).encode(),
        )
        _put(
            store,
            _resource(card_url(number), f"raw/{number}.html", complete, Kind.CARD),
            complete,
        )
    batch = seal_batch(store, scope=[Scope(provider="jp", kind="card")])
    frozen = FrozenSources(store.root, store.store_id, batch.batch_id)
    catalog = generate(
        [("jp", frozen)],
        {("jp", number): number.split("-")[0] for number in ("BP02-071", "PR-001")},
        producer_revision=art_case.recipe.program_revision,
    )
    write_catalog(repo / "card-data", catalog)
    type_ref: dict[str, JsonValue] = {
        "batch_id": batch.batch_id,
        "source_version_id": next(
            item.source_version_id
            for item in frozen.current_versions
            if item.url == card_url("BP02-071")
        ),
        "parser": "official-jp-exact-v1",
        "locator": "/faces/0/card_type",
        "text_hash": digest("フォロワー".encode()),
    }
    term = record(
        "vocabulary_adoption",
        {"kind": "type", "code": "follower"},
        {
            "label": {"kind": "authored", "lang": "ja", "text": "Synthetic follower"},
            "raw_mappings": [
                {
                    "region": "jp",
                    "lang": "ja",
                    "raw": "フォロワー",
                    "source_ref": type_ref,
                    "special_kinds": [],
                }
            ],
            "active": True,
        },
    )
    class_ref: dict[str, JsonValue] = {
        **type_ref,
        "locator": "/faces/0/card_class",
        "text_hash": digest("エルフ".encode()),
    }
    class_term = record(
        "vocabulary_adoption",
        {"kind": "class", "code": "forestcraft"},
        {
            "label": {"kind": "authored", "lang": "ja", "text": "Synthetic class"},
            "raw_mappings": [
                {
                    "region": "jp",
                    "lang": "ja",
                    "raw": "エルフ",
                    "source_ref": class_ref,
                    "special_kinds": [],
                }
            ],
            "active": True,
        },
    )
    write_document(
        root, "catalog/adoptions/vocabulary/001.yaml", envelope([class_term, term])
    )
    inputs = authoring.Inputs(
        profile="public",
        repo=repo,
        jp_batch_id=batch.batch_id,
        revision=art_case.recipe.program_revision,
    )
    with authoring.prepare(inputs, cards=frozenset({"BP02-071"})) as workspace:
        before = authoring.inspect(workspace, "BP02-071")
    write(root, plan(art_case.recipe, art_case.base.decisions))
    write(root, plan(art_case.recipe, art_case.decisions))
    store.root.rename(tmp_path / "unmounted-archive")
    art_case.recipe.archive.rename(tmp_path / "unmounted-review-archive")
    with authoring.prepare(inputs, cards=frozenset({"BP02-071"})) as workspace:
        assert authoring.inspect(workspace, "BP02-071") == before
    assert before["card_id"]


def test_art_candidate_binding_survives_both_renewals(
    art_case: ArtCase, tmp_path: Path
) -> None:
    root = art_case.base.authored
    registry = load_registry(root)
    printing = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, PrintingData) and r.data.region == "jp"
    )
    allocation = next(
        r.data
        for r in registry.records.values()
        if isinstance(r.data, AllocationData) and r.data.printing_id == printing.id
    )
    face = printing.source_face_map[0].face_id
    binding = ImageBinding(
        printing_id=printing.id, face_id=face, int_id=allocation.int_id, ordinal=0
    )
    bitmap = tmp_path / "images" / image_path(allocation.int_id, 0, "art_s")
    bitmap.parent.mkdir(parents=True)
    bitmap.write_bytes(b"synthetic bitmap bytes")
    variants: list[JsonValue] = [
        {"size_key": size.key, "width": size.max_width, "height": size.max_height}
        for size in sorted(SIZES, key=lambda s: s.key)
    ]
    index = ImageIndex(
        recipe=DEFAULT_RECIPE.version,
        store_id="synthetic",
        pins=(),
        evidence=(),
        bindings=(binding,),
        tables={
            "image_asset": [{"id": "synthetic"}],
            "printing_image": [
                {
                    "printing_id": printing.id,
                    "face_id": face,
                    "image_id": "synthetic",
                    "availability": "available",
                    "publication_state": "approved",
                    "variants": variants,
                    "card_version": 1,
                    "art_version": 1,
                }
            ],
            "image_variant": [
                {
                    "image_id": "synthetic",
                    **object_value(v),
                    "bytes": bitmap.stat().st_size,
                }
                for v in variants
            ],
        },
    )
    before = art_inputs(index, root, tmp_path / "images")
    assert len(before[0]) == 1
    write(root, plan(art_case.recipe, art_case.base.decisions))
    write(root, plan(art_case.recipe, art_case.decisions))
    art_case.recipe.archive.rename(tmp_path / "unmounted-review-archive")
    assert art_inputs(index, root, tmp_path / "images") == before


def test_full_registry_for_database_authoring_requires_review_evidence(
    art_case: ArtCase,
) -> None:
    recipe = art_case.recipe
    write(art_case.base.authored, plan(recipe, art_case.base.decisions))
    write(art_case.base.authored, plan(recipe, art_case.decisions))
    inputs = offline.Inputs(
        repo=recipe.repo,
        archive=recipe.archive,
        store_id=recipe.store_id,
        sources=tuple(
            offline.RegionalInput(
                region=s.region,
                card_batch=s.card_batch,
                image_batch=s.image_batch or "sha256:" + "0" * 64,
                parser_version=s.parser_version,
            )
            for s in recipe.sources
        ),
        revision=recipe.program_revision,
        as_of="2026-10-02",
        published_at="2026-10-02T00:00:00Z",
        data_version="preview-20261002T010203Z-0001",
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic",
        normalizer_version="synthetic",
    )
    with pytest.raises(ValueError, match="permanent registry review inputs"):
        offline.registry_snapshot(inputs)
    inputs = inputs.model_copy(
        update={"registry_review_inputs": recipe.registry_review_inputs}
    )
    snapshot = offline.registry_snapshot(inputs)
    review = next(
        r.data
        for r in snapshot.records.values()
        if isinstance(r.data, MappingReviewData)
    )
    assert review.coverage_hash == recipe.registry_review_inputs.coverage_hash
    assert snapshot.transition_uses
    (
        recipe.registry_review_inputs.root
        / recipe.registry_review_inputs.batch_id
        / "jp.jsonl"
    ).write_bytes(b"damaged")
    with pytest.raises(ValueError, match=r"(?i)hash|inventory|coverage|bytes"):
        offline.registry_snapshot(inputs)


@pytest.mark.parametrize("damage", [None, "recipe", "evidence", "output"])
def test_en_inventory_explicitly_replays_renewal(
    art_case: ArtCase,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    damage: str | None,
) -> None:
    recipe = art_case.recipe
    write(art_case.base.authored, plan(recipe, art_case.base.decisions))
    write(art_case.base.authored, plan(recipe, art_case.decisions))
    store = ArchiveStore(
        tmp_path / "data",
        tmp_path / "data/manifest/manifest.sqlite",
        tmp_path / "data/manifest/.lock",
        recipe.archive,
        recipe.store_id,
    )
    recipe = recipe.model_copy(
        update={
            "sources": tuple(
                s.model_copy(
                    update={
                        "card_batch": seal_batch(
                            store, scope=[Scope(provider=s.region, kind="card")]
                        ).batch_id
                    }
                )
                for s in recipe.sources
            )
        }
    )
    path, output = tmp_path / "renewal.json", tmp_path / "inventory.json"
    if damage == "output":
        output = (
            recipe.registry_review_inputs.root
            / recipe.registry_review_inputs.batch_id
            / "jp.jsonl"
        )
    preserved = output.read_bytes() if output.exists() else None
    saved = (
        recipe.model_copy(update={"program_revision": "0" * 40})
        if damage == "recipe"
        else recipe
    )
    path.write_bytes(canonical(saved.model_dump(mode="json")))
    if damage == "evidence":
        (
            recipe.registry_review_inputs.root
            / recipe.registry_review_inputs.batch_id
            / "jp.jsonl"
        ).write_bytes(b"damaged")
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "english-inventory",
            "--archive",
            str(recipe.archive),
            "--store-id",
            recipe.store_id,
            "--en-batch",
            recipe.sources[0].card_batch,
            "--jp-batch",
            recipe.sources[1].card_batch,
            "--authored",
            str(art_case.base.authored),
            "--program-revision",
            recipe.program_revision,
            "--renewal-inputs",
            str(path),
            "--output",
            str(output),
        ],
    )
    if damage:
        with pytest.raises(
            ValueError,
            match=r"(?i)recipe|hash|inventory|coverage|bytes|permanent review inputs",
        ):
            english_inventory.main()
        if damage == "output":
            assert output.read_bytes() == preserved
        else:
            assert not output.exists()
    else:
        english_inventory.main()
        report = json.loads(output.read_bytes())
        reviewed = [
            row["historical_review"]
            for row in report["candidates"]
            if "historical_review" in row
        ]
        assert reviewed
        assert all(
            r["coverage_hash"] == recipe.registry_review_inputs.coverage_hash
            for r in reviewed
        )

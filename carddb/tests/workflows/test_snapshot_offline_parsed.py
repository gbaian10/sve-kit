"""Native parsed builds retain all source uses after removing the SVE archive."""

import shutil
from dataclasses import replace
from datetime import date
from typing import TYPE_CHECKING

from pydantic import JsonValue

from sve_carddb.core.json import digest
from sve_carddb.core.provenance import InputRecord
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.parsed_cards.generate import generate
from sve_carddb.domains.parsed_cards.storage import write as write_parsed
from sve_carddb.domains.registry.build import build as build_identity
from sve_carddb.domains.registry.inputs import Card, Mapping
from sve_carddb.domains.registry.review import InitDecisions
from sve_carddb.domains.registry.review import Inputs as IdentityInputs
from sve_carddb.domains.registry.storage import plan_files, write_files
from sve_carddb.ingest.archive.frozen_sources import FrozenSources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import Scope, seal_batch
from sve_carddb.parse.pages import extract_en, extract_jp, official_en, official_jp
from sve_carddb.workflows import offline

from ..domains.text_observations.test_effect_presence import page
from ..ingest.test_source_archive import _put, _resource, _store
from ..support.adoption_fixtures import REPO, commit, envelope, make_case
from ..support.catalog_vocabulary_fixtures import vocabulary_record
from ..support.product_fixtures import envelope as product_envelope
from ..support.product_fixtures import family, install
from ..support.registry_observation_fixtures import parsed_card
from ..support.translation_fixtures import write as write_translations

if TYPE_CHECKING:
    from pathlib import Path


def test_default_parsed_build_without_sve_archive(tmp_path: Path) -> None:  # ruff: ignore[too-many-locals] -- complete native build uses real regional inputs without consumer mocks
    root = tmp_path / "repository"
    case = make_case(root)
    write_translations(case.root, {})
    shutil.copytree(
        REPO / "carddb/src",
        root / "carddb/src",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_version.py"),
    )
    for name in ("carddb/uv.lock", "carddb/pyproject.toml"):
        (root / name).write_bytes((REPO / name).read_bytes())
    for area in ("rules/rulings", "products/identities"):
        (case.root / area).mkdir(parents=True)
    documents: dict[str, dict[str, JsonValue]] = {
        "catalog/leader-name-frames.yaml": {
            "format": 1,
            "kind": "printing_leader_name_frames",
            "records": [],
        },
        "catalog/reprints.yaml": {
            "format": 1,
            "kind": "printing_reprints",
            "records": [],
        },
        "catalog/rarity-badges.yaml": {
            "format": 1,
            "kind": "printing_rarity_badges",
            "records": [],
        },
        "rules/keyword-registry.yaml": {
            "format": 1,
            "kind": "keyword_registry",
            "decision": {
                "id": "synthetic",
                "state": "confirmed",
                "note": "Synthetic mapping.",
            },
            "records": [],
        },
    }
    for name, document in documents.items():
        install(case.root, name, document)
    archive = tmp_path / "absent-archive"
    store = _store(tmp_path / "card-sources")
    batches: dict[str, str] = {}
    cards: dict[str, Card] = {}
    mappings: list[dict[str, JsonValue]] = []
    for region in (SourceRegion.EN, SourceRegion.JP):
        adapter = official_jp if region == SourceRegion.JP else official_en
        number = "SYN-01" if region == SourceRegion.JP else "SYN-02"
        raw = page(
            "jp" if region == SourceRegion.JP else "en",
            '<div class="detail">Synthetic native card effect</div>',
        ).replace(b"SYN-01", number.encode())
        resource = replace(
            _resource(
                adapter.card_url(number), f"raw/{region.value}.html", raw, Kind.CARD
            ),
            region=region,
        )
        _put(store, resource, raw)
        card = (
            parsed_card(extract_jp.extract_card(raw, number=number))
            if region == SourceRegion.JP
            else parsed_card(extract_en.extract_card(raw, number=number))
        )
        cards[region.value] = card
        sealed = seal_batch(store, scope=(Scope(provider=region.value, kind="card"),))
        batches[region.value] = sealed.batch_id
        mappings.append(
            {
                "region": region.value,
                "lang": "ja" if region == SourceRegion.JP else "en",
                "raw": "Synthetic type",
                "source_ref": {
                    "batch_id": sealed.batch_id,
                    "source_version_id": sealed.inventory.current[0].source_version_id,
                    "parser": f"official-{region.value}-exact-v1",
                    "locator": "/faces/0/card_type"
                    if region == SourceRegion.JP
                    else "/faces/0/info/Card Type",
                    "text_hash": digest(b"Synthetic type"),
                },
                "special_kinds": [],
            }
        )
    install(
        case.root,
        "catalog/adoptions/vocabulary/001.yaml",
        envelope([vocabulary_record("type", "follower", mappings)]),
    )
    identities = build_identity(
        IdentityInputs(
            jp={"SYN-01": cards["jp"]},
            en={"SYN-02": cards["en"]},
            mapping=Mapping(targets={"SYN-02": None}, original_art=set(), reskins={}),
            decisions=InitDecisions(),
            as_of=date(2026, 10, 2),
            jp_hash=digest(b"synthetic JP coverage"),
        ),
        {},
    )
    write_files(plan_files(root / "authored", identities))
    homes = {
        str(entry.data["home_set_id"]) for entry in identities if entry.kind == "card"
    }
    for home in homes:
        install(
            root / "authored",
            "products/family/" + home + "/001.yaml",
            product_envelope([family(home, code="synthetic")]),
        )
    recipe = offline.Inputs(
        repo=root,
        archive=archive,
        store_id="test-store",
        sources=tuple(
            offline.RegionalInput(
                region=region,
                card_batch=batches[region],
                image_batch=batches[region],
                parser_version=f"official-{region}-exact-v1",
            )
            for region in ("en", "jp")
        ),
        revision=commit(root),
        as_of="2026-10-01",
        data_version="preview-20261002T010203Z-0001",
        published_at="2026-10-02T01:02:03Z",
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic-v1",
        normalizer_version="nfkc-casefold-v1",
    )
    catalog = generate(
        [
            (pin.region, FrozenSources(store.root, store.store_id, pin.card_batch))
            for pin in recipe.sources
        ],
        {("en", "SYN-02"): "SYN", ("jp", "SYN-01"): "SYN"},
        producer_revision=recipe.revision,
    )
    write_parsed(root / "card-data", catalog)
    archived = recipe.model_copy(
        update={"source_mode": "archive", "archive": store.root}
    )
    expected = offline.build(archived)
    store.root.rename(tmp_path / "unmounted-archive")
    actual = offline.build(recipe, bundle_dir=tmp_path / "bundle")
    assert {row["region"] for row in actual.projection.tables["printing"]} == {
        "en",
        "jp",
    }
    assert actual.projection.tables == expected.projection.tables
    assert (
        InputRecord.model_validate_json(actual.input_content).uses
        == InputRecord.model_validate_json(expected.input_content).uses
    )

"""Native current offline composition verifies actual synthetic frozen sources."""

import shutil
from dataclasses import replace
from datetime import date
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.cli import app
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.core.provenance import InputRecord
from sve_carddb.core.regions import SourceRegion
from sve_carddb.domains.registry.build import build as build_identity
from sve_carddb.domains.registry.inputs import Card, Mapping
from sve_carddb.domains.registry.review import InitDecisions
from sve_carddb.domains.registry.review import Inputs as IdentityInputs
from sve_carddb.domains.registry.storage import Index, plan_files, write_files
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import Scope, seal_batch
from sve_carddb.parse.pages import extract_en, extract_jp, official_en, official_jp
from sve_carddb.parse.pages.official_jp import card_url
from sve_carddb.workflows import offline

from .adoption_fixtures import REPO, commit, write
from .catalog_vocabulary_fixtures import make_vocabulary_case
from .product_fixtures import envelope as product_envelope
from .product_fixtures import family, install
from .registry_observation_fixtures import parsed_card
from .test_effect_presence import page
from .test_source_archive import _put, _resource, _store
from .translation_fixtures import choice, envelope, term
from .translation_fixtures import write as write_translations

CODE_PATH = "carddb/src/sve_carddb/domains/translations/sources.py"
RUNTIME = (CODE_PATH,)


if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

    from .catalog_vocabulary_fixtures import VocabularyCase


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> VocabularyCase:
    case = make_vocabulary_case(tmp_path_factory.mktemp("offline-source-replay"))
    shutil.copytree(
        REPO / "carddb/src",
        case.case.repository / "carddb/src",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_version.py"),
    )
    write(
        case.case.root,
        "ids/index.yaml",
        Index().model_dump(mode="json"),
    )
    for area in ("products", "product-identities"):
        (case.case.root / area).mkdir(parents=True, exist_ok=True)
    store = _store(case.case.repository.parent / "translation-sources")
    shutil.copytree(case.archive, store.root, dirs_exist_ok=True)
    payloads = {
        card_url("SYN-01"): (
            page("jp", '<div class="detail">Synthetic frozen term</div>'),
            SourceRegion.JP,
            Kind.CARD,
        ),
        "https://shadowverse-wb.com/web/CardList/cardList?lang=ja&offset=0": (
            canonical(
                {
                    "data_headers": {"result_code": 1},
                    "data": {
                        "card_details": {
                            "10000002": {
                                "common": {"skill_text": "Synthetic frozen term"}
                            }
                        },
                    },
                }
            ),
            SourceRegion.SVWB,
            Kind.API,
        ),
        "https://shadowverse-wb.com/web/CardList/cardList?lang=cht&offset=0": (
            canonical(
                {
                    "data_headers": {"result_code": 1},
                    "data": {
                        "card_details": {
                            "10000001": {
                                "common": {"skill_text": "Synthetic chosen label"}
                            }
                        }
                    },
                }
            ),
            SourceRegion.SVWB,
            Kind.API,
        ),
    }
    for number, (url, (raw, region, kind)) in enumerate(payloads.items()):
        resource = replace(
            _resource(url, f"raw/translation-{number}", raw, kind),
            region=region,
            content_type="application/json" if kind == Kind.API else "text/html",
        )
        _put(store, resource, raw)
    batch = seal_batch(store)
    references: dict[str, dict[str, JsonValue]] = {}
    for entry in batch.inventory.current:
        if entry.url == card_url("SYN-01"):
            locator, text, parser = (
                "/faces/0/text",
                "Synthetic frozen term",
                "translation-jp-v1",
            )
            name = "term"
        elif "lang=ja" in entry.url:
            locator, text, parser = (
                "/data/card_details/10000002/common/skill_text",
                "Synthetic frozen term",
                "translation-svwb-v1",
            )
            name = "evidence"
        else:
            locator, text, parser = (
                "/data/card_details/10000001/common/skill_text",
                "Synthetic chosen label",
                "translation-svwb-v1",
            )
            name = "choice"
        references[name] = {
            "batch_id": batch.batch_id,
            "source_version_id": entry.source_version_id,
            "parser": parser,
            "locator": locator,
            "text_hash": digest(text.encode()),
        }
    definition = term("rule.synthetic")
    object_value(definition["data"])["source_ref"] = references["term"]
    selected = choice("rule.synthetic")
    chosen = object_value(selected["data"])
    chosen["value"] = {
        "kind": "source",
        "source_ref": references["choice"],
        "span": None,
    }
    chosen["concept_evidence"] = [
        {
            "kind": "effect_term",
            "jp_ref": references["evidence"],
            "jp_span": {"start": 0, "end": len("Synthetic frozen term")},
            "target_ref": references["choice"],
            "target_span": {"start": 0, "end": len("Synthetic chosen label")},
            "concept_note": "Synthetic exact concept relation",
        }
    ]
    write_translations(
        case.case.root,
        {
            "translations/glossary/concepts/001.yaml": envelope([definition]),
            "translations/glossary/choices/001.yaml": envelope([selected]),
        },
    )
    return replace(case, archive=store.root)


def test_native_export_offline_current_catalog_without_adapters(  # ruff: ignore[too-many-locals] -- full native CLI binds real synthetic identities, current catalog and frozen regional sources
    baseline: VocabularyCase, tmp_path: Path, mocker: MockerFixture
) -> None:
    prepare = mocker.spy(offline, "_prepare_catalog")
    populate = mocker.spy(offline, "populate_text_preview")
    root = tmp_path / "repository"
    archive = tmp_path / "archive"
    shutil.copytree(baseline.case.repository, root)
    shutil.copytree(baseline.archive, archive)
    store = _store(tmp_path / "card-sources")
    batches: dict[str, str] = {}
    cards: dict[str, Card] = {}
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
        batches[region.value] = seal_batch(
            store, scope=(Scope(provider=region.value, kind="card"),)
        ).batch_id
    shutil.copytree(store.root, archive, dirs_exist_ok=True)
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
        as_of="2026-10-02",
        data_version="preview-20261002T010203Z-0001",
        published_at="2026-10-02T01:02:03Z",
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic-v1",
        normalizer_version="nfkc-casefold-v1",
    )
    path = tmp_path / "inputs.json"
    path.write_bytes(canonical(recipe.model_dump(mode="json")))
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export-offline",
            "--format-version",
            "2.0.0",
            "--inputs",
            str(path),
            "--preview-dir",
            str(tmp_path / "preview"),
            "--private-dir",
            str(tmp_path / "private"),
            "--bundle-dir",
            str(tmp_path / "bundle"),
        ],
    )
    assert result.exit_code == 0, repr(result.exception)
    assert prepare.call_count == populate.call_count == 1
    assert {p.name for p in (tmp_path / "bundle").iterdir()} == {
        "build.sqlite",
        "inputs.json",
        "report.json",
    }
    pointer = object_value(
        parse((tmp_path / "preview/snapshots/preview/current.json").read_bytes())
    )
    manifest = object_value(
        parse((tmp_path / "preview" / str(pointer["manifest_path"])).read_bytes())
    )
    assert manifest["format_version"] == "2.0.0"
    assert manifest["regions"] == ["en", "jp"]
    record = InputRecord.model_validate_json(
        (tmp_path / "bundle/inputs.json").read_bytes()
    )
    assert {use.usage for use in record.uses} >= {
        "catalog_exact_text",
        "translation_evidence",
    }
    with open_database(
        compile_build((*MINIMUM_CAPABILITIES, "en", "translation_evidence")),
        tmp_path / "bundle/build.sqlite",
    ) as db:
        assert len(db.rows("card")) == 2
        assert len(db.rows("printing")) == 2
        assert len(db.rows("language")) == 3
        assert len(db.rows("vocabulary")) == 3
        for table in (
            "language",
            "vocabulary",
            "glossary_term",
            "glossary_translation",
        ):
            assert db.rows(table)
            assert all(
                row.values["authored_source_id"] is not None for row in db.rows(table)
            )
        assert {row.values["category"] for row in db.rows("decision")}.isdisjoint(
            {"language_adoption", "vocabulary_adoption", "glossary_translation"}
        )

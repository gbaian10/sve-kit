"""The production offline composition replays actual synthetic source receipts."""

import copy
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue
from typer.testing import CliRunner

from sve_carddb.build_db.database import open_database
from sve_carddb.build_db.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.build_inputs import BuildContext, InputRecord
from sve_carddb.catalog import adoption_importer
from sve_carddb.cli import app
from sve_carddb.manifest import Kind, Region
from sve_carddb.registry.preview import plan_preview
from sve_carddb.registry.preview.evidence import MemoryEvidence
from sve_carddb.registry.storage import Index
from sve_carddb.snapshot import offline
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.source_archive import seal_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.translations.sources import CODE_PATH, RUNTIME, Sources

from .adoption_fixtures import REPO, commit, write
from .catalog_vocabulary_fixtures import make_vocabulary_case
from .test_adoption_sources import SOURCE_RUNTIME
from .test_catalog_replay import adopt_then_update
from .test_effect_presence import page
from .test_snapshot_preview import EmptySources
from .test_source_archive import _put, _resource, _store
from .translation_fixtures import choice, envelope, term
from .translation_fixtures import write as write_translations

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.text_observations import TextPlan

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
    write(
        case.case.root,
        "products/index.yaml",
        {"product_authored_format": 1, "kind": "product_index", "includes": {}},
    )
    write(
        case.case.root,
        "product-identities/index.yaml",
        {
            "product_identity_format": 1,
            "kind": "product_identity_index",
            "includes": {},
        },
    )
    store = _store(case.case.repository.parent / "translation-sources")
    shutil.copytree(case.archive, store.root, dirs_exist_ok=True)
    payloads = {
        card_url("SYN-01"): (
            page("jp", '<div class="detail">Synthetic frozen term</div>'),
            Region.JP,
            Kind.CARD,
        ),
        "https://shadowverse-wb.com/web/CardList/cardList?lang=ja&offset=0": (
            canonical(
                {
                    "data_headers": {"result_code": 1},
                    "data": {
                        "skill_names": {"1": "Synthetic history evidence"},
                        "card_details": {
                            "10000002": {
                                "common": {"skill_text": "Synthetic frozen term"}
                            }
                        },
                    },
                }
            ),
            Region.SVWB,
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
            Region.SVWB,
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
                "/data/skill_names/1",
                "Synthetic history evidence",
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
            "store_id": store.store_id,
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
            "jp_ref": references["evidence"]
            | {
                "locator": "/data/card_details/10000002/common/skill_text",
                "text_hash": digest(b"Synthetic frozen term"),
            },
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
    case = replace(case, archive=store.root)
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))


def _drop_glossary_uses(monkeypatch: pytest.MonkeyPatch) -> None:
    original_populate = adoption_importer.populate_adoptions

    def omit_glossary_uses(
        db: Database,
        inputs: adoption_importer.AdoptionInputs,
        *,
        build: BuildContext,
        stores: dict[str, Path],
        text_plan: TextPlan | None = None,
    ) -> InputRecord:
        result = original_populate(
            db, inputs, build=build, stores=stores, text_plan=text_plan
        )
        return result.model_copy(
            update={
                "uses": tuple(
                    use for use in result.uses if use.usage != "translation_evidence"
                )
            }
        )

    monkeypatch.setattr(adoption_importer, "populate_adoptions", omit_glossary_uses)


@pytest.mark.parametrize(
    "name", [SOURCE_RUNTIME, "carddb/uv.lock", "carddb/pyproject.toml"]
)
@pytest.mark.parametrize("drop_actual_translation_uses", [False, True])
def test_export_offline_replays_source_receipts_after_pinned_update(
    baseline: VocabularyCase,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    name: str,
    drop_actual_translation_uses: bool,
) -> None:
    root = tmp_path / "repository"
    shutil.copytree(baseline.case.repository, root)
    case = replace(baseline.case, repository=root, root=root / "authored")
    case, signed = adopt_then_update(case, name)
    # Empty physical inventory isolates catalog replay; its two frozen JSON sources are real.
    identity = plan_preview(
        case.root, MemoryEvidence({}, frozenset()), regions=("en", "jp")
    )
    monkeypatch.setattr(offline, "plan_preview", lambda *_args, **_kwargs: identity)
    for attribute in (
        "FrozenEN",
        "FrozenJP",
        "FrozenTexts",
        "FrozenProducts",
        "FrozenCardExtras",
        "RegionalImages",
    ):
        monkeypatch.setattr(
            offline, attribute, lambda *_args, **_kwargs: EmptySources()
        )
    batch = str(baseline.references["jp", "type", 0]["batch_id"])
    inputs = offline.Inputs(
        repo=root,
        archive=baseline.archive,
        store_id="test-store",
        sources=tuple(
            offline.RegionalInput(
                region=region,
                card_batch=batch,
                image_batch=batch,
                parser_version=f"official-{region}-exact-v1",
            )
            for region in ("en", "jp")
        ),
        revision=case.revision,
        as_of="2026-10-02",
        data_version="preview-20261002T010203Z-0001",
        published_at="2026-10-02T01:02:03Z",
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic-v1",
        normalizer_version="nfkc-casefold-v1",
    )
    recipe = tmp_path / "inputs.json"
    recipe.write_bytes(canonical(inputs.model_dump(mode="json")))
    if drop_actual_translation_uses:
        _drop_glossary_uses(monkeypatch)
    result = CliRunner().invoke(
        app,
        [
            "snapshot",
            "export-offline",
            "--inputs",
            str(recipe),
            "--preview-dir",
            str(tmp_path / "preview"),
            "--cdn-dir",
            str(tmp_path / "formal"),
            "--bundle-dir",
            str(tmp_path / "bundle"),
        ],
    )
    if drop_actual_translation_uses:
        assert result.exit_code == 1
        assert isinstance(result.exception, ValueError)
        assert str(result.exception) == "Build input use closure or context mismatch"
        assert not (tmp_path / "preview/snapshots/preview/current.json").exists()
        assert not (tmp_path / "bundle").exists()
        return
    assert result.exit_code == 0, repr(result.exception)
    record = InputRecord.model_validate_json(
        (tmp_path / "bundle/inputs.json").read_bytes()
    )
    source_uses = [use for use in record.uses if use.usage == "catalog_exact_text"]
    assert len({use.source.id for use in source_uses}) == 2
    glossary_uses = [use for use in record.uses if use.usage == "translation_evidence"]
    assert len(glossary_uses) == 4
    assert len({use.source.id for use in glossary_uses}) == 3
    config = object_value(parse(record.context.configuration.encode()))
    with open_database(
        compile_build((*MINIMUM_CAPABILITIES, "en", "translation_evidence")),
        tmp_path / "bundle/build.sqlite",
    ) as db:
        assert len(db.rows("glossary_term")) == 1
        assert (
            db.rows("glossary_term")[0].values["source_ja"] == "Synthetic frozen term"
        )
        assert len(db.rows("glossary_translation")) == 1
        assert (
            db.rows("glossary_translation")[0].values["text"]
            == "Synthetic chosen label"
        )
    assert "translation_authored" in config
    assert isinstance(config["catalog_source_recipes"], dict)
    translations = object_value(config["translation_recipes"])
    assert set(translations) == {
        "translation-en-v1",
        "translation-jp-v1",
        "translation-sv1-v1",
        "translation-svwb-v1",
    }
    for parser, recipe_value in translations.items():
        pin = object_value(recipe_value)
        assert pin["program_revision"] == case.revision
        assert pin["code_path"] == CODE_PATH
        assert pin["code_hash"] == digest((root / CODE_PATH).read_bytes())
        provider = parser.removeprefix("translation-").removesuffix("-v1")
        assert pin["config"] == {"provider": provider}
        assert pin["config_hash"] == digest(canonical({"provider": provider}))
    assert "carddb/pyproject.toml" in {pin.name for pin in record.context.dependencies}
    assert (tmp_path / "preview/snapshots/preview/current.json").is_file()
    assert not (tmp_path / "formal").exists()
    assert signed == {
        p.relative_to(case.root).as_posix(): p.read_bytes()
        for p in (case.root / "catalog-adoptions").rglob("*.yaml")
    }


@pytest.mark.parametrize("recipes", [None, [], "invalid", 1])
def test_catalog_recipe_configuration_has_an_explicit_refusal(
    baseline: VocabularyCase, recipes: JsonValue
) -> None:
    from sve_carddb.catalog.adoption_sources import AdoptionSources, PinnedRepository  # ruff: ignore[import-outside-top-level] -- isolate the resolver boundary

    build = baseline.build()
    config = copy.deepcopy(object_value(parse(build.configuration.encode())))
    config["catalog_source_recipes"] = recipes
    build = build.model_copy(update={"configuration": canonical(config).decode()})
    with pytest.raises(ValueError, match=r"^Catalog source recipes must be an object$"):
        AdoptionSources(
            {"test-store": baseline.archive}, PinnedRepository(baseline.case.repository)
        ).recipe("exact-json-v1", build)


@pytest.mark.parametrize(
    "path",
    [
        "carddb/src/sve_carddb/extract/official_jp.py",
        "carddb/src/sve_carddb/extract/official_en.py",
        "carddb/src/sve_carddb/snapshot/values.py",
    ],
)
def test_offline_recipe_rejects_each_missing_current_parser(
    baseline: VocabularyCase, path: str
) -> None:
    dependencies = {
        name: (baseline.case.repository / name).read_bytes()
        for name in (
            "carddb/src/sve_carddb/extract/official_jp.py",
            "carddb/src/sve_carddb/extract/official_en.py",
            "carddb/src/sve_carddb/snapshot/values.py",
        )
        if name != path
    }
    with pytest.raises(
        ValueError, match=r"^Offline catalog parser dependency is absent$"
    ):
        offline._catalog_source_recipes(baseline.case.revision, dependencies)


def test_catalog_recipe_configuration_cannot_be_absent(
    baseline: VocabularyCase,
) -> None:
    from sve_carddb.catalog.adoption_sources import AdoptionSources, PinnedRepository  # ruff: ignore[import-outside-top-level] -- isolate the resolver boundary

    build = baseline.build()
    config = object_value(parse(build.configuration.encode()))
    del config["catalog_source_recipes"]
    build = build.model_copy(update={"configuration": canonical(config).decode()})
    with pytest.raises(ValueError, match=r"^Catalog source recipes must be an object$"):
        AdoptionSources(
            {"test-store": baseline.archive}, PinnedRepository(baseline.case.repository)
        ).recipe("exact-json-v1", build)


@pytest.mark.parametrize("recipes", [None, [], "invalid", 1, "absent"])
def test_translation_recipes_have_a_domain_refusal(
    baseline: VocabularyCase, recipes: JsonValue
) -> None:
    config: dict[str, JsonValue] = {}
    if recipes != "absent":
        config["translation_recipes"] = recipes
    build = BuildContext.from_inputs(
        baseline.case.revision,
        {name: (baseline.case.repository / name).read_bytes() for name in RUNTIME},
        config,
    )
    sources = Sources({"test-store": baseline.archive}, baseline.case.repository, build)
    with pytest.raises(
        ValueError, match=r"^Translation source recipes must be an object$"
    ):
        sources.projection("test-store", "unused", "unused", "translation-jp-v1")


def test_offline_translation_recipe_requires_pinned_code() -> None:
    with pytest.raises(
        ValueError, match=r"^Offline translation parser dependency is absent$"
    ):
        offline._translation_source_recipes("a" * 40, {})

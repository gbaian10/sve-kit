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
from sve_carddb.build_inputs import InputRecord
from sve_carddb.cli import app
from sve_carddb.registry.preview import plan_preview
from sve_carddb.registry.preview.evidence import MemoryEvidence
from sve_carddb.registry.storage import Index
from sve_carddb.snapshot import offline
from sve_carddb.snapshot.values import canonical, object_value, parse

from .adoption_fixtures import REPO, commit, write
from .catalog_vocabulary_fixtures import make_vocabulary_case
from .test_adoption_sources import SOURCE_RUNTIME
from .test_catalog_replay import adopt_then_update
from .test_glossary_adoption import authored
from .test_snapshot_preview import EmptySources
from .translation_fixtures import choice, envelope
from .translation_fixtures import write as write_translations

if TYPE_CHECKING:
    from pathlib import Path

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
    write_translations(
        case.case.root,
        {
            "translations/glossary/concepts/001.yaml": envelope(
                [authored("rule.synthetic")]
            ),
            "translations/glossary/choices/001.yaml": envelope(
                [choice("rule.synthetic")]
            ),
        },
    )
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))


@pytest.mark.parametrize(
    "name", [SOURCE_RUNTIME, "carddb/uv.lock", "carddb/pyproject.toml"]
)
def test_export_offline_replays_source_receipts_after_pinned_update(
    baseline: VocabularyCase,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    name: str,
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
    assert result.exit_code == 0, repr(result.exception)
    record = InputRecord.model_validate_json(
        (tmp_path / "bundle/inputs.json").read_bytes()
    )
    source_uses = [use for use in record.uses if use.usage == "catalog_exact_text"]
    assert len({use.source.id for use in source_uses}) == 2
    config = object_value(parse(record.context.configuration.encode()))
    with open_database(
        compile_build((*MINIMUM_CAPABILITIES, "en", "translation_evidence")),
        tmp_path / "bundle/build.sqlite",
    ) as db:
        assert len(db.rows("glossary_term")) == 1
        assert len(db.rows("glossary_translation")) == 1
    assert "translation_authored" in config
    assert isinstance(config["catalog_source_recipes"], dict)
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

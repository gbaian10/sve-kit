"""Offline entry composes real policy/raw owners and verifies both bundle reconstructions."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db.current import compile_current_build
from sve_carddb.build_db.database import open_database
from sve_carddb.build_db.t1 import MINIMUM_CAPABILITIES
from sve_carddb.catalog import adoption_importer
from sve_carddb.catalog.models import Catalog
from sve_carddb.catalog.projection import CatalogProjection
from sve_carddb.products import Language
from sve_carddb.snapshot import offline
from sve_carddb.snapshot.offline_names import Composer, composer
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.text_observations import Binding, Vocabulary
from sve_carddb.translations.importer import Inputs as TranslationInputs
from sve_carddb.translations.importer import populate_glossary

from .adoption_fixtures import REPO, commit, git
from .digital_name_policy_fixtures import make_policy_fixture
from .product_fixtures import envelope, family, install
from .test_snapshot_preview import EmptySources
from .translation_fixtures import write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import Database
    from sve_carddb.build_inputs import BuildContext, InputRecord
    from sve_carddb.digital_links.importer import Result as LinkResult
    from sve_carddb.digital_name_policies.application import Result
    from sve_carddb.text_observations import TextPlan
    from sve_carddb.translations.current_names import Names


@pytest.fixture(scope="module")
def recipe(tmp_path_factory: pytest.TempPathFactory) -> offline.Inputs:
    root = tmp_path_factory.mktemp("name-offline")
    fixture = make_policy_fixture(root)
    shutil.copytree(
        REPO / "carddb/src",
        root / "carddb/src",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "_version.py"),
    )
    for name in ("carddb/uv.lock", "carddb/pyproject.toml"):
        (root / name).write_bytes((REPO / name).read_bytes())
    write(root / "authored", {})
    home = fixture.digital.printing.home_set_id
    install(
        root / "authored",
        "products/family/" + home + "/001.yaml",
        envelope([family(home, code="synthetic")]),
    )
    identities = root / "authored/product-identities/index.yaml"
    identities.parent.mkdir(parents=True)
    identities.write_bytes(
        canonical(
            {
                "product_identity_format": 1,
                "kind": "product_identity_index",
                "includes": {},
            }
        )
    )
    revision = commit(root)
    ref = fixture.owner().name_ref
    assert ref is not None
    batch = ref.batch_id
    return offline.Inputs(
        repo=root,
        archive=fixture.digital.store,
        store_id="test-store",
        sources=tuple(
            offline.RegionalInput(
                region=region,
                card_batch=batch,
                image_batch=batch,
                parser_version="translation-" + region + "-v1",
            )
            for region in ("en", "jp")
        ),
        revision=revision,
        as_of="2026-10-03",
        data_version="preview-20261003T000000Z-0001",
        published_at="2026-10-03T00:00:00Z",
        feedback_url="https://example.invalid/feedback",
        grammar_version="synthetic-v1",
        normalizer_version="nfkc-casefold-v1",
        name_policy="approved-frozen-v1",
    )


def catalogue(monkeypatch: pytest.MonkeyPatch) -> None:
    """Ancillary vocabulary is synthetic; policy, link, glossary and physical replay are real."""

    for attribute in ("FrozenProducts", "FrozenCardExtras"):
        monkeypatch.setattr(
            offline, attribute, lambda *_args, **_kwargs: EmptySources()
        )

    class CatalogInputs:
        def __init__(
            self,
            root: Path,
            repository: Path,
            authored_revision: str,
            *_args: object,
            **_kwargs: object,
        ) -> None:
            self.root, self.repository, self.authored_revision = (
                root,
                repository,
                authored_revision,
            )

        def translation_inputs(self) -> TranslationInputs:
            return TranslationInputs(self.root, self.repository, self.authored_revision)

        def configuration(self) -> dict[str, JsonValue]:
            return self.translation_inputs().configuration()

        def load(self) -> tuple[()]:
            return ()

    def populate(
        db: Database,
        inputs: CatalogInputs,
        *,
        build: BuildContext,
        stores: dict[str, Path],
        **_kwargs: object,
    ) -> InputRecord:
        return populate_glossary(
            db, inputs.translation_inputs(), build=build, stores=stores
        )

    vocabulary = Vocabulary(
        bindings=(
            Binding(region="jp", kind="type", raw="フォロワー", code="follower"),
            Binding(region="jp", kind="class", raw="エルフ", code="elf"),
        )
    )
    projection = CatalogProjection(
        Catalog(
            languages=tuple(
                Language(code=code, display_name=code, fallback_order=())
                for code in ("en", "ja", "zh-Hant")
            ),
            terms=(),
            aliases=(),
            symbols=(),
            normalizer_version="nfkc-casefold-v1",
        ),
        vocabulary,
    )
    monkeypatch.setattr(adoption_importer, "AdoptionInputs", CatalogInputs)
    monkeypatch.setattr(
        adoption_importer, "derive_catalog", lambda *_args, **_kwargs: projection
    )
    monkeypatch.setattr(adoption_importer, "populate_adoptions", populate)


def test_offline_name_policy_reconstructs_identical_bundle(
    recipe: offline.Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    catalogue(monkeypatch)
    first = offline.build(recipe, bundle_dir=tmp_path / "first")
    second = offline.build(recipe, bundle_dir=tmp_path / "second")
    assert first == second
    files = {
        p.relative_to(tmp_path / "first"): p.read_bytes()
        for p in (tmp_path / "first").rglob("*")
        if p.is_file()
    }
    assert files == {
        p.relative_to(tmp_path / "second"): p.read_bytes()
        for p in (tmp_path / "second").rglob("*")
        if p.is_file()
    }
    assert object_value(first.report["name_application"])["covered_owners"] == 1
    translation = first.projection.tables["translation"][0]
    assert translation["origin"] == "official"
    assert translation["authority"] == "digital_official"
    assert translation["low_confidence"] is False
    assert "status" not in translation
    assert "decision_id" not in translation
    assert (
        object_value(
            array(first.projection.tables["face_revision"][0]["translations"])[0]
        )["translation_id"]
        == first.projection.tables["translation"][0]["id"]
    )
    assert not first.projection.tables["digital_link"]
    assert first.projection.config["digital_endpoints"] == []
    schema = compile_current_build(
        (*MINIMUM_CAPABILITIES, "en", "translation_evidence", "translation_names")
    )
    with open_database(schema, tmp_path / "first/build.sqlite") as db:
        assert db.rows("digital_link")
        assert not any(
            r.values["category"] == "digital_name_policy" for r in db.rows("decision")
        )
    assert digest(first.input_content) == first.report["input_sha256"]


def test_offline_detects_missing_name_application_source_use(
    recipe: offline.Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    catalogue(monkeypatch)
    original = Composer.populate

    def omitted(  # ruff: ignore[too-many-arguments] -- mirror the independently checked producer signature
        self: Composer,
        db: Database,
        texts: TextPlan,
        *,
        context: BuildContext,
        stores: dict[str, Path],
        replay: Names,
        links: LinkResult | None,
    ) -> Result:
        result = original(
            self, db, texts, context=context, stores=stores, replay=replay, links=links
        )
        return replace(
            result,
            record=result.record.model_copy(
                update={
                    "uses": tuple(
                        u
                        for u in result.record.uses
                        if u.usage != "digital_policy_owner_name"
                    )
                }
            ),
        )

    monkeypatch.setattr(Composer, "populate", omitted)
    with pytest.raises(
        ValueError, match=r"^Build input use closure or context mismatch$"
    ):
        offline.build(recipe, bundle_dir=tmp_path / "bundle")
    assert not (tmp_path / "bundle").exists()


def test_name_composition_requires_available_immutable_tree(
    recipe: offline.Inputs,
) -> None:
    with pytest.raises(
        ValueError, match=r"^Name composition digital-link entry tree is unavailable$"
    ):
        composer(recipe.model_copy(update={"revision": "0" * 40}))


def test_name_composition_rejects_symlinked_authored_entry(
    recipe: offline.Inputs, tmp_path: Path
) -> None:
    root = tmp_path / "checkout"
    git(tmp_path, "clone", "--local", "--no-hardlinks", str(recipe.repo), str(root))
    path = root / "authored/digital-links"
    target = root / "private-link-entry"
    if path.exists():
        path.rename(target)
    else:
        target.mkdir()
    path.symlink_to(target, target_is_directory=True)
    with pytest.raises(
        ValueError,
        match=r"^Name composition digital-link entry differs from immutable tree$",
    ):
        composer(recipe.model_copy(update={"repo": root}))

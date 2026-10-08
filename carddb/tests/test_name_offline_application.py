"""Offline entry composes real policy/raw owners and verifies both bundle reconstructions."""

import shutil
from types import SimpleNamespace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.core.json import array, digest, object_value
from sve_carddb.domains.catalog import adoption_importer
from sve_carddb.domains.catalog.models import Catalog
from sve_carddb.domains.catalog.projection import CatalogProjection
from sve_carddb.domains.products import Language
from sve_carddb.domains.text_observations import Binding, Vocabulary
from sve_carddb.domains.translations.glossary.importer import populate_glossary
from sve_carddb.domains.translations.inputs import Inputs as TranslationInputs
from sve_carddb.workflows import offline
from sve_carddb.workflows.offline_names import composer

from .adoption_fixtures import REPO, commit, git
from .digital_name_policy_fixtures import make_policy_fixture
from .product_fixtures import envelope, family, install
from .test_snapshot_preview import EmptySources
from .translation_fixtures import write

if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

    from sve_carddb.build import Database
    from sve_carddb.core.provenance import BuildContext, InputRecord


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
    (root / "authored/product-identities").mkdir(parents=True)
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
        offline,
        "_prepare_catalog",
        lambda *_args, **_kwargs: SimpleNamespace(projection=projection),
    )
    monkeypatch.setattr(offline, "_populate_adoptions", populate)


def test_offline_name_policy_reconstructs_identical_bundle(
    recipe: offline.Inputs,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    mocker: MockerFixture,
) -> None:
    catalogue(monkeypatch)
    prepared = mocker.spy(offline, "prepare_names")
    first = offline.build(recipe, bundle_dir=tmp_path / "first")
    second = offline.build(recipe, bundle_dir=tmp_path / "second")
    assert first == second
    assert prepared.call_count == 2
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
    schema = compile_build(
        (*MINIMUM_CAPABILITIES, "en", "translation_evidence", "translation_names")
    )
    with open_database(schema, tmp_path / "first/build.sqlite") as db:
        assert db.rows("digital_link")
        assert not any(
            r.values["category"] == "digital_name_policy" for r in db.rows("decision")
        )
    assert digest(first.input_content) == first.report["input_sha256"]


def test_name_composition_reads_link_entry_from_disk(
    recipe: offline.Inputs, tmp_path: Path
) -> None:
    root = tmp_path / "checkout"
    git(tmp_path, "clone", "--local", "--no-hardlinks", str(recipe.repo), str(root))
    checkout = recipe.model_copy(update={"repo": root})
    present = composer(checkout)
    assert present is not None
    assert present.links is not None
    shutil.rmtree(root / "authored/digital-links")
    absent = composer(checkout)
    assert absent is not None
    assert absent.links is None


def test_name_composition_rejects_symlinked_authored_entry(
    recipe: offline.Inputs, tmp_path: Path
) -> None:
    root = tmp_path / "checkout"
    git(tmp_path, "clone", "--local", "--no-hardlinks", str(recipe.repo), str(root))
    path = root / "authored/digital-links"
    target = root / "private-link-entry"
    path.rename(target)
    path.symlink_to(target, target_is_directory=True)
    checkout = recipe.model_copy(update={"repo": root})
    names = composer(checkout)
    assert names is not None
    with pytest.raises(ValueError, match=r"^Symlink digital-link input$"):
        names.configuration(checkout)

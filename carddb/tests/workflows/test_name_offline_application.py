"""Offline entry composes real policy/raw owners and verifies both bundle reconstructions."""

import shutil
from types import SimpleNamespace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build.database import open_database
from sve_carddb.build.t1 import MINIMUM_CAPABILITIES, compile_build
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.domains.catalog import adoption_importer
from sve_carddb.domains.catalog.models import Catalog
from sve_carddb.domains.catalog.projection import CatalogProjection
from sve_carddb.domains.digital.links.loader import load_links
from sve_carddb.domains.products import Language
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.domains.text_observations import Binding, Vocabulary
from sve_carddb.domains.translations.glossary.importer import populate_glossary
from sve_carddb.domains.translations.inputs import Inputs as TranslationInputs
from sve_carddb.workflows import offline
from sve_carddb.workflows.offline_names import composer

from ..export.test_snapshot_preview import EmptySources
from ..support.adoption_fixtures import REPO, commit, git
from ..support.digital_link_fixtures import write as write_links
from ..support.digital_name_policy_fixtures import make_policy_fixture
from ..support.product_fixtures import envelope, family, install
from ..support.translation_fixtures import name_term, write

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
    install(
        root / "authored",
        "catalog/leader-name-frames.yaml",
        {"format": 1, "kind": "printing_leader_name_frames", "records": []},
    )
    install(
        root / "authored",
        "catalog/reprints.yaml",
        {"format": 1, "kind": "printing_reprints", "records": []},
    )
    install(
        root / "authored",
        "catalog/rarity-badges.yaml",
        {"format": 1, "kind": "printing_rarity_badges", "records": []},
    )
    (root / "authored/rules/rulings").mkdir(parents=True)
    install(
        root / "authored",
        "rules/keyword-registry.yaml",
        {
            "format": 1,
            "kind": "keyword_registry",
            "decision": {
                "id": "synthetic-keywords",
                "state": "confirmed",
                "note": "Synthetic mapping.",
            },
            "records": [],
        },
    )
    home = fixture.digital.printing.home_set_id
    install(
        root / "authored",
        "products/family/" + home + "/001.yaml",
        envelope([family(home, code="synthetic")]),
    )
    (root / "authored/products/identities").mkdir(parents=True)
    ref = fixture.owner().name_ref
    assert ref is not None
    term = name_term("name.test")
    object_value(term["data"]).update(
        source_ref=ref.model_dump(mode="json"),
        authored_source_ja=None,
        missing_source_reason=None,
    )
    write(
        root / "authored",
        {
            "translations/glossary/concepts/001.yaml": {
                "format": 3,
                "kind": "translation_shard",
                "records": [term],
            }
        },
    )
    revision = commit(root)
    batch = ref.batch_id
    return offline.Inputs(
        source_mode="archive",
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
    tables = first.projection.tables
    assert tables["face_revision"][0]["name_concept_id"] == "term:name.test"
    assert tables["annotation_concept"][0]["card_ids"] == [tables["card"][0]["id"]]
    assert first.report["card_name_target_gaps"] == []
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
    assert len(first.projection.tables["digital_link"]) == 1
    assert first.projection.tables["digital_link_coverage"] == []
    assert first.projection.tables["digital_art"] == []
    assert first.projection.tables["voice"] == []
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
    shutil.rmtree(root / "authored/digital/links")
    absent = composer(checkout)
    assert absent is not None
    assert absent.links is None


def test_offline_reports_fragment_even_when_it_matches_a_complete_owner_name(
    recipe: offline.Inputs, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    catalogue(monkeypatch)
    root = tmp_path / "checkout"
    git(tmp_path, "clone", "--local", "--no-hardlinks", str(recipe.repo), str(root))
    for directory in ("products/identities", "rules/rulings"):
        (root / "authored" / directory).mkdir(parents=True, exist_ok=True)
    shard = root / "authored/translations/glossary/concepts/001.yaml"
    document = object_value(read_yaml(shard))
    term = object_value(object_value(array(document["records"])[0])["data"])
    term.update(
        source_ref=None,
        authored_source_ja="Synthetic card",
        missing_source_reason="Synthetic name fragment",
    )
    shard.write_bytes(canonical(document))
    built = offline.build(
        recipe.model_copy(update={"repo": root, "revision": commit(root)})
    )
    assert (
        built.projection.tables["face_revision"][0]["name_concept_id"]
        == "term:name.test"
    )
    assert built.projection.tables["annotation_concept"][0]["card_ids"] == []
    assert built.report["card_name_target_gaps"] == [
        {"term_id": "term:name.test", "reason": "name_fragment"}
    ]


def test_name_composition_rejects_symlinked_authored_entry(
    recipe: offline.Inputs, tmp_path: Path
) -> None:
    root = tmp_path / "checkout"
    git(tmp_path, "clone", "--local", "--no-hardlinks", str(recipe.repo), str(root))
    path = root / "authored/digital/links"
    target = root / "private-link-entry"
    path.rename(target)
    path.symlink_to(target, target_is_directory=True)
    checkout = recipe.model_copy(update={"repo": root})
    names = composer(checkout)
    assert names is not None
    with pytest.raises(ValueError, match=r"^Symlink digital-link input$"):
        names.configuration(checkout)


@pytest.mark.parametrize("name_policy", [None, "approved-frozen-v1"])
def test_model_browse_intake_is_independent_of_name_policy(
    recipe: offline.Inputs,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    name_policy: str | None,
) -> None:
    catalogue(monkeypatch)
    chosen = recipe.model_copy(update={"name_policy": name_policy})
    original = offline.build(chosen)
    root = tmp_path / "checkout"
    git(tmp_path, "clone", "--local", "--no-hardlinks", str(recipe.repo), str(root))
    for directory in ("products/identities", "rules/rulings"):
        (root / "authored" / directory).mkdir(parents=True, exist_ok=True)
    records = list[JsonValue](
        record.model_copy(update={"review_level": "model_reviewed"}).model_dump(
            mode="json"
        )
        for record in load_links(root / "authored").records()
    )
    write_links(
        root / "authored",
        {
            "digital/links/synthetic/001.yaml": {
                "format": 2,
                "kind": "digital_link_shard",
                "records": records,
            }
        },
    )
    changed = chosen.model_copy(update={"repo": root, "revision": commit(root)})
    composed = composer(changed)
    assert composed is not None
    assert composed.links is not None
    assert (composed.inputs is None) is (name_policy is None)
    built = offline.build(changed)
    assert len(built.projection.tables["digital_link"]) == 1
    assert (
        built.projection.tables["digital_link"][0]["review_level"] == "model_reviewed"
    )
    for table, rows in original.projection.tables.items():
        if table != "digital_link":
            assert built.projection.tables[table] == rows
    assert built.projection.tables["digital_link_coverage"] == []
    assert object_value(built.report["digital_links"])["stale"] == []
    if name_policy is None:
        assert "name_application" not in built.report


def test_no_composition_without_either_capability(
    recipe: offline.Inputs, tmp_path: Path
) -> None:
    root = tmp_path / "checkout"
    git(tmp_path, "clone", "--local", "--no-hardlinks", str(recipe.repo), str(root))
    shutil.rmtree(root / "authored/digital/links")
    assert (
        composer(recipe.model_copy(update={"repo": root, "name_policy": None})) is None
    )

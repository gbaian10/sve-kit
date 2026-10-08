"""Real sealed synthetic sources exercise independent raw/parser/locator guards."""

import copy
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import CompiledSchema, create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, canonical, digest, object_value
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.catalog.adoption_models import ReviewContext, SourceRef
from sve_carddb.domains.catalog.adoption_sources import pointer
from sve_carddb.domains.registry.snapshot import load_registry
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.domains.translations.sources import Sources
from sve_carddb.ingest.archive.manifest import Kind
from sve_carddb.ingest.archive.source_archive import (
    ArchiveError,
    seal_batch,
    verify_batch,
)
from sve_carddb.parse.pages.official_jp import card_url
from sve_carddb.workflows.offline import _populate_adoptions, _prepare_catalog

from .adoption_fixtures import (
    CODE,
    REPO,
    Case,
    commit,
    envelope,
    index,
    make_case,
    write,
)
from .catalog_adoption_fixtures import populate_case
from .registry_snapshot_fixtures import registry_root as registry_root  # ruff: ignore[useless-import-alias] -- expose synthetic pytest fixture
from .test_registry import inputs as inputs  # ruff: ignore[useless-import-alias] -- expose registry fixture dependency
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

JSON_CODE = "carddb/src/sve_carddb/core/json.py"
SOURCE_RUNTIME = "carddb/src/sve_carddb/domains/catalog/adoption_sources.py"


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> tuple[Case, Path, str]:  # ruff: ignore[too-many-locals] -- module-scoped two-field sealed source and immutable runtime pins
    root = tmp_path_factory.mktemp("adoption-source")
    store = _store(root / "source")
    raw = canonical(
        {
            "faces": [
                {"name": "Synthetic source name", "card_type": "Synthetic source type"}
            ]
        }
    )
    resource = replace(
        _resource(card_url("SYNTHETIC-001"), "raw/source.json", raw, Kind.CARD),
        content_type="application/json",
    )
    _put(store, resource, raw)
    sealed = seal_batch(store)
    version = sealed.inventory.current[0].source_version_id
    case = make_case(root / "repository")
    paths = (CODE, JSON_CODE, SOURCE_RUNTIME, "carddb/uv.lock", "carddb/pyproject.toml")
    for name in paths:
        target = case.repository / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / name).read_bytes())
    revision = commit(case.repository)
    config: dict[str, JsonValue] = {}
    recipe: dict[str, JsonValue] = {"version": "exact-json-v1", "config": config}
    context = BuildContext.from_inputs(
        revision,
        {"catalog_source_recipes": {"exact-json-v1": recipe}},
    )
    review: dict[str, JsonValue] = {
        "context": context.model_dump(mode="json", round_trip=True),
        "source_batches": [{"batch_id": sealed.batch_id}],
    }

    def ref(locator: str, text: str) -> dict[str, JsonValue]:
        return {
            "batch_id": sealed.batch_id,
            "source_version_id": version,
            "parser": "exact-json-v1",
            "locator": locator,
            "text_hash": digest(text.encode()),
        }

    for path in (case.root / "catalog-adoptions").rglob("*.yaml"):
        if path.name == "index.yaml":
            continue
        shard = object_value(read_yaml(path))
        members = array(shard["records"])
        for item in members:
            member = object_value(item)
            data = object_value(member["data"])
            if member["kind"] == "vocabulary_adoption":
                label_ref = ref("/faces/0/name", "Synthetic source name")
                type_ref = ref("/faces/0/card_type", "Synthetic source type")
                value = object_value(data["value"])
                value["label"] = {"kind": "source", "source_ref": label_ref}
                value["raw_mappings"] = [
                    {
                        "region": "jp",
                        "lang": "ja",
                        "raw": "Synthetic source type",
                        "source_ref": type_ref,
                        "special_kinds": [],
                    }
                ]
        write(case.root, path.relative_to(case.root).as_posix(), envelope(members))
    index(case.root)
    return (
        replace(case, revision=commit(case.repository), review=review),
        store.root,
        version,
    )


@pytest.fixture
def case(tmp_path: Path, baseline: tuple[Case, Path, str]) -> tuple[Case, Path, str]:
    base, archive, version = baseline
    repository = tmp_path / "repository"
    copied = tmp_path / "archive"
    shutil.copytree(base.repository, repository)
    shutil.copytree(archive, copied)
    return (
        replace(
            base,
            repository=repository,
            root=repository / "authored",
            review=copy.deepcopy(base.review),
        ),
        copied,
        version,
    )


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build()


def test_exact_frozen_fields_derive_evidence_from_values(
    case: tuple[Case, Path, str], schema: CompiledSchema
) -> None:
    inputs, archive, version = case
    with create_database(schema) as db:
        result = populate_case(db, inputs, {"test-store": archive})
        assert len(result.uses) == 2
        assert {u.source.id for u in result.uses} == {version}
        assert not db.rows("decision")
        assert any(
            r.values["text"] == "Synthetic source name" for r in db.rows("text_unit")
        )


def test_identity_sources_are_stage_local(
    case: tuple[Case, Path, str], registry_root: Path
) -> None:
    inputs, archive, version = case
    registry = load_registry(registry_root)
    sources = Sources(
        {"test-store": archive}, inputs.repository, inputs.build(), registry
    )
    review = ReviewContext.model_validate_json(canonical(inputs.review))
    ref = SourceRef(
        batch_id=str(
            object_value(array(inputs.review["source_batches"])[0])["batch_id"]
        ),
        source_version_id=version,
        parser="exact-json-v1",
        locator="/faces/0/name",
        text_hash=digest(b"Synthetic source name"),
    )
    sources.identities.text(ref, review)
    first = sources.stage(inputs.build())
    second = first.stage(inputs.build())
    for stage in (first, second):
        assert stage.identities is not sources.identities
        assert stage.identities.registry() is registry
        assert stage.identity_indexes is sources.identity_indexes
        assert stage.cache is sources.cache
        assert not stage.identities.uses
        assert stage.identities.text(ref, review)[0].text == "Synthetic source name"
        assert len(stage.identities.uses) == 1
        stage.identities.text(ref, review)
        assert len(stage.identities.uses) == 1
    assert first.identities is not second.identities
    assert len(sources.identities.uses) == 1
    assert len(first.identities.uses) == 1
    assert len(second.identities.uses) == 1


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("locator", "locator is absent"),
        ("text_hash", "exact text hash mismatch"),
        ("raw", "hash"),
        ("missing_raw", "Missing|missing|absent|No such file"),
        ("wrong_kind", "source field does not match kind"),
        ("half_raw", "exact raw/region/language mismatch"),
        ("trait", "^Vocabulary mapping source field does not match kind$"),
        (
            "disabled_recipe",
            "^Source-field recipe is not enabled for this vocabulary kind$",
        ),
    ],
)
def test_source_single_guard_rejection(
    case: tuple[Case, Path, str], schema: CompiledSchema, mutation: str, message: str
) -> None:
    inputs, archive, _ = case
    name = "catalog-adoptions/vocabulary/shared/001.yaml"
    shard = object_value(read_yaml(inputs.root / name))
    member = object_value(array(shard["records"])[0])
    data = object_value(member["data"])
    value = object_value(data["value"])
    mapping = object_value(array(value["raw_mappings"])[0])
    reference = object_value(mapping["source_ref"])
    review = copy.deepcopy(inputs.review)
    if mutation in {"raw", "missing_raw"}:
        batch = str(object_value(array(inputs.review["source_batches"])[0])["batch_id"])
        blob = archive / verify_batch(archive, "test-store", batch).entries[0].blob.path
        if mutation == "raw":
            blob.write_bytes(b"Changed synthetic source")
        else:
            blob.unlink()
    elif mutation in {"wrong_kind", "trait", "disabled_recipe"}:
        object_value(data["subject"])["kind"] = (
            "class"
            if mutation == "wrong_kind"
            else "trait"
            if mutation == "trait"
            else "frame"
        )
    elif mutation == "half_raw":
        mapping["raw"] = "Synthetic source"
    else:
        reference["locator" if mutation == "locator" else "text_hash"] = (
            "/missing" if mutation == "locator" else "sha256:" + "e" * 64
        )
    write(inputs.root, name, envelope([member]))
    index(inputs.root)
    revised = replace(inputs, revision=commit(inputs.repository), review=review)
    build = revised.build()
    with create_database(schema) as db:
        with pytest.raises((ValueError, OSError, ArchiveError), match=message):
            with db.transaction():
                _populate_adoptions(
                    db,
                    revised.inputs(),
                    build=build,
                    stores={"test-store": archive},
                    prepared=_prepare_catalog(
                        revised.inputs(), build, {"test-store": archive}
                    ),
                    sources=Sources({"test-store": archive}, revised.repository, build),
                )
        assert not db.rows("source_record")


@pytest.mark.parametrize("locator", ["/a~", "/a~2", "/items/01", "/items/-1"])
def test_json_pointer_has_no_implicit_escapes_or_index_folding(locator: str) -> None:
    with pytest.raises(ValueError, match=r"Pointer|locator"):
        pointer({"items": ["Synthetic"]}, locator)
    assert pointer({"a/b": {"~": "Synthetic"}}, "/a~1b/~0") == "Synthetic"

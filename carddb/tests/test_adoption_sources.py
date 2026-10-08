"""Real sealed synthetic sources exercise independent raw/parser/locator guards."""

import copy
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import CompiledSchema, create_database
from sve_carddb.build_db.current import compile_current_build
from sve_carddb.build_inputs import BuildContext
from sve_carddb.catalog.adoption_sources import pointer
from sve_carddb.manifest import Kind
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.offline import _populate_adoptions, _prepare_catalog
from sve_carddb.snapshot.values import array, canonical, digest, object_value
from sve_carddb.source_archive import ArchiveError, seal_batch, verify_batch
from sve_carddb.sources.official_jp import card_url
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import (
    CODE,
    REPO,
    Case,
    commit,
    envelope,
    fields,
    index,
    make_case,
    write,
)
from .current_catalog_fixtures import current_case, populate_case
from .test_source_archive import _put, _resource, _store

if TYPE_CHECKING:
    from pathlib import Path

JSON_CODE = "carddb/src/sve_carddb/snapshot/values.py"
SOURCE_RUNTIME = "carddb/src/sve_carddb/catalog/adoption_sources.py"


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
        "context": context.model_dump(mode="json"),
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
            data["review_context_hash"] = digest(canonical(review))
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
                member["evidence"] = sorted(
                    [
                        {"source_ref": label_ref, "role": "synthetic label review"},
                        {"source_ref": type_ref, "role": "synthetic mapping review"},
                    ],
                    key=canonical,
                )
        write(
            case.root, path.relative_to(case.root).as_posix(), envelope(members, review)
        )
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
    return compile_current_build()


def test_exact_frozen_field_source_and_f1(
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


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("locator", "locator is absent"),
        ("text_hash", "exact text hash mismatch"),
        ("raw", "hash"),
        ("missing_raw", "Missing|missing|absent|No such file"),
        ("wrong_kind", "source field does not match kind"),
        ("half_raw", "exact raw/region/language mismatch"),
        ("evidence_missing", "mapping lacks approved source evidence"),
        ("trait", "^Vocabulary mapping source field does not match kind$"),
        (
            "disabled_recipe",
            "^Source-field recipe is not enabled for this vocabulary kind$",
        ),
    ],
)
def test_source_single_guard_rejection(  # ruff: ignore[too-many-locals] -- independent mutations of one tiny sealed-source baseline
    case: tuple[Case, Path, str], schema: CompiledSchema, mutation: str, message: str
) -> None:
    inputs, archive, _ = case
    name = "catalog-adoptions/vocabulary/shared/001.yaml"
    shard = object_value(read_yaml(inputs.root / name))
    member, data, _ = fields(shard)
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
        member["record_key"] = canonical(
            ["vocabulary_adoption", data["subject"], 1]
        ).decode()
    elif mutation == "half_raw":
        mapping["raw"] = "Synthetic source"
    elif mutation == "evidence_missing":
        member["evidence"] = [
            e
            for e in array(member["evidence"])
            if object_value(e)["source_ref"] != reference
        ]
    else:
        reference["locator" if mutation == "locator" else "text_hash"] = (
            "/missing" if mutation == "locator" else "sha256:" + "e" * 64
        )
        for evidence in array(member["evidence"]):
            old = object_value(object_value(evidence)["source_ref"])
            if old["locator"] == "/faces/0/card_type":
                old.update(reference)
        member["evidence"] = sorted(array(member["evidence"]), key=canonical)
    write(inputs.root, name, envelope([member], review))
    index(inputs.root)
    revised = replace(inputs, revision=commit(inputs.repository), review=review)
    revised = current_case(revised)
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

"""Historical receipts survive dependency updates while both contexts remain verified."""

import copy
import shutil
from dataclasses import replace
from pathlib import Path
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build_db import CompiledSchema, create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.catalog.adoption_importer import (
    derive_catalog,
    import_adopted_text,
    import_adoptions,
)
from sve_carddb.catalog.adoption_models import ReviewContext
from sve_carddb.catalog.adoption_sources import AdoptionSources, PinnedRepository
from sve_carddb.registry.preview.importer import populate_identity_rows
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.text_observations import text_configuration

from .adoption_display_fixtures import make_display_case
from .adoption_fixtures import (
    Case,
    commit,
    envelope,
    index,
    source_configuration,
    write,
)
from .test_adoption_sources import JSON_CODE, SOURCE_RUNTIME
from .test_adoption_sources import baseline as baseline  # ruff: ignore[useless-import-alias] -- reuse one tiny sealed source per module
from .test_catalog_text_composition import adopt_compound

if TYPE_CHECKING:
    from .adoption_display_fixtures import DisplayCase


@pytest.fixture(scope="module")
def text_baseline(tmp_path_factory: pytest.TempPathFactory) -> DisplayCase:
    return adopt_compound(
        make_display_case(
            tmp_path_factory.mktemp("catalog-replay-text") / "case",
            variants=False,
            raw_type="Synthetic type / Evolved",
        )
    )


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_t0()


@pytest.fixture
def case(tmp_path: Path, baseline: tuple[Case, Path, str]) -> tuple[Case, Path]:
    original, archive, _ = baseline
    repository = tmp_path / "repository"
    shutil.copytree(original.repository, repository)
    return replace(
        original, repository=repository, root=repository / "authored"
    ), archive


def replace_reviews(case: Case, review: dict[str, JsonValue]) -> Case:
    for file in (case.root / "catalog-adoptions").rglob("*.yaml"):
        if file.name == "index.yaml":
            continue
        members = array(object_value(read_yaml(file))["records"])
        for member in members:
            object_value(object_value(member)["data"])["review_context_hash"] = digest(
                canonical(review)
            )
        write(
            case.root, file.relative_to(case.root).as_posix(), envelope(members, review)
        )
    index(case.root)
    return replace(case, revision=commit(case.repository))


def adopt_then_update(case: Case, name: str) -> tuple[Case, dict[str, bytes]]:
    path = case.repository / name
    current = path.read_bytes()
    path.write_bytes(current + b"\n# Previous synthetic runtime revision.\n")
    reviewed_revision = commit(case.repository)
    review = copy.deepcopy(case.review)
    context = object_value(review["context"])
    context["program_revision"] = reviewed_revision
    for item in array(context["dependencies"]):
        pin = object_value(item)
        pin["sha256"] = digest((case.repository / str(pin["name"])).read_bytes())
    configuration = object_value(parse(str(context["configuration"]).encode()))
    for item in object_value(configuration["catalog_source_recipes"]).values():
        recipe = object_value(item)
        recipe["program_revision"] = reviewed_revision
        recipe["code_hash"] = digest(
            (case.repository / str(recipe["code_path"])).read_bytes()
        )
    context["configuration"] = canonical(configuration).decode()
    case = replace_reviews(case, review)
    signed = {
        p.relative_to(case.root).as_posix(): p.read_bytes()
        for p in (case.root / "catalog-adoptions").rglob("*.yaml")
    }
    path.write_bytes(current)
    return replace(case, revision=commit(case.repository), review=review), signed


@pytest.mark.parametrize(
    "name", [SOURCE_RUNTIME, "carddb/uv.lock", "carddb/pyproject.toml", JSON_CODE]
)
def test_adopted_catalog_survives_pinned_file_update_without_resigning(
    case: tuple[Case, Path], schema: CompiledSchema, name: str
) -> None:
    inputs, archive = case
    inputs, signed = adopt_then_update(inputs, name)
    review = ReviewContext.model_validate_json(canonical(inputs.review))
    assert next(
        p.sha256 for p in review.context.dependencies if p.name == name
    ) != digest((inputs.repository / name).read_bytes())
    build = inputs.build()
    assert build.program_revision != review.context.program_revision
    assert next(p.sha256 for p in build.dependencies if p.name == name) == digest(
        (inputs.repository / name).read_bytes()
    )
    with create_database(schema) as db:
        before = {t.name: db.rows(t.name) for t in schema.tables}
        projection = derive_catalog(
            db, inputs.inputs(), build=build, stores={"test-store": archive}
        )
        assert before == {t.name: db.rows(t.name) for t in schema.tables}
        assert len(projection.catalog.terms) == 1
        assert projection.catalog.terms[0].active
        assert (
            projection.vocabulary.lookup("jp", "type", "Synthetic source type").code
            == "follower"
        )
        result = import_adoptions(
            db, inputs.inputs(), build=build, stores={"test-store": archive}
        )
        assert result.context == build
        assert len(result.uses) == 2
    assert signed == {
        p.relative_to(inputs.root).as_posix(): p.read_bytes()
        for p in (inputs.root / "catalog-adoptions").rglob("*.yaml")
    }


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("dependency_hash", "^Review dependency hash mismatch$"),
        (
            "missing_pin",
            "^Source parser runtime/dependency closure cannot be replayed$",
        ),
        ("revision", "^Pinned immutable dependency unavailable$"),
        ("parser_hash", "^Recipe program/config hash mismatch$"),
    ],
)
def test_bad_historical_pins_are_rejected_after_dependency_update(
    case: tuple[Case, Path], schema: CompiledSchema, change: str, message: str
) -> None:
    inputs, archive = case
    inputs, _ = adopt_then_update(inputs, SOURCE_RUNTIME)
    review = copy.deepcopy(inputs.review)
    context = object_value(review["context"])
    if change == "dependency_hash":
        for value in array(context["dependencies"]):
            pin = object_value(value)
            if pin["name"] == SOURCE_RUNTIME:
                pin["sha256"] = "sha256:" + "0" * 64
    elif change == "missing_pin":
        context["dependencies"] = [
            p
            for p in array(context["dependencies"])
            if object_value(p)["name"] != "carddb/uv.lock"
        ]
    elif change == "revision":
        context["program_revision"] = "0" * 40
    else:
        config = object_value(parse(str(context["configuration"]).encode()))
        object_value(object_value(config["catalog_source_recipes"])["exact-json-v1"])[
            "code_hash"
        ] = "sha256:" + "0" * 64
        context["configuration"] = canonical(config).decode()
    # Re-sign only synthetic envelopes to reach the historical guard, not outer membership.
    revised = replace_reviews(inputs, review)
    with create_database(schema) as db:
        with pytest.raises(ValueError, match=message):
            import_adoptions(
                db,
                revised.inputs(),
                build=revised.build(),
                stores={"test-store": archive},
            )
        assert not db.rows("source_record")


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("dependency_hash", "^Review dependency hash mismatch$"),
        (
            "missing_pin",
            "^Source parser runtime/dependency closure cannot be replayed$",
        ),
    ],
)
def test_historical_recipe_itself_checks_immutable_runtime_closure(
    case: tuple[Case, Path], change: str, message: str
) -> None:
    inputs, archive = case
    inputs, _ = adopt_then_update(inputs, SOURCE_RUNTIME)
    context = ReviewContext.model_validate_json(canonical(inputs.review)).context
    if change == "dependency_hash":
        context = context.model_copy(
            update={
                "dependencies": tuple(
                    p.model_copy(update={"sha256": "sha256:" + "0" * 64})
                    if p.name == SOURCE_RUNTIME
                    else p
                    for p in context.dependencies
                )
            }
        )
    else:
        context = context.model_copy(
            update={
                "dependencies": tuple(
                    p for p in context.dependencies if p.name != "carddb/uv.lock"
                )
            }
        )
    sources = AdoptionSources(
        {"test-store": archive}, PinnedRepository(inputs.repository), historical=True
    )
    with pytest.raises(ValueError, match=message):
        sources.recipe("exact-json-v1", context)


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("disk", "^Source parser runtime/dependency closure cannot be replayed$"),
        ("parser_disk", "^Historical recipe implementation cannot be replayed$"),
        ("dependency_hash", "^Review dependency hash mismatch$"),
        (
            "missing_pin",
            "^Source parser runtime/dependency closure cannot be replayed$",
        ),
    ],
)
def test_current_build_cannot_borrow_historical_runtime_validation(
    case: tuple[Case, Path], schema: CompiledSchema, change: str, message: str
) -> None:
    inputs, archive = case
    if change in {"disk", "parser_disk"}:
        file = inputs.repository / (
            JSON_CODE if change == "parser_disk" else SOURCE_RUNTIME
        )
        file.write_bytes(file.read_bytes() + b"\n# Unloaded current runtime.\n")
        inputs = replace(inputs, revision=commit(inputs.repository))
    build = inputs.build()
    if change not in {"disk", "parser_disk"}:
        pins = tuple(
            p.model_copy(update={"sha256": "sha256:" + "0" * 64})
            if change == "dependency_hash" and p.name == SOURCE_RUNTIME
            else p
            for p in build.dependencies
            if change != "missing_pin" or p.name != "carddb/uv.lock"
        )
        build = build.model_copy(update={"dependencies": pins})
    with create_database(schema) as db:
        with pytest.raises(ValueError, match=message):
            derive_catalog(
                db, inputs.inputs(), build=build, stores={"test-store": archive}
            )
        assert not db.rows("source_record")


@pytest.mark.parametrize("condition", ["missing", "symlink"])
def test_current_runtime_file_must_be_regular(
    case: tuple[Case, Path], monkeypatch: pytest.MonkeyPatch, condition: str
) -> None:
    inputs, archive = case
    target = Path(__file__).resolve().parents[2] / SOURCE_RUNTIME
    if condition == "missing":
        original = Path.is_file
        monkeypatch.setattr(
            Path, "is_file", lambda p: False if p == target else original(p)
        )
    else:
        original_link = Path.is_symlink
        monkeypatch.setattr(
            Path, "is_symlink", lambda p: True if p == target else original_link(p)
        )
    sources = AdoptionSources(
        {"test-store": archive}, PinnedRepository(inputs.repository)
    )
    with pytest.raises(
        ValueError,
        match=r"^Source parser runtime/dependency closure cannot be replayed$",
    ):
        sources.recipe("exact-json-v1", inputs.build())


@pytest.mark.parametrize(
    "name",
    [SOURCE_RUNTIME, "carddb/uv.lock", "carddb/src/sve_carddb/extract/official_jp.py"],
)
def test_current_frozen_identity_and_freshness_survive_historical_program_update(
    tmp_path: Path, text_baseline: DisplayCase, schema: CompiledSchema, name: str
) -> None:
    repository = tmp_path / "repository"
    shutil.copytree(text_baseline.case.repository, repository)
    copied = replace(
        text_baseline.case, repository=repository, root=repository / "authored"
    )
    copied, signed = adopt_then_update(copied, name)
    case = replace(text_baseline, case=copied)
    build = case.context()
    with create_database(schema) as db:
        case.parents(db)
        with db.transaction():
            populate_identity_rows(
                db,
                case.plan.publication_identity(),
                authored_revision=copied.revision,
                build=build,
            )
        before = {t.name: db.rows(t.name) for t in schema.tables}
        projection = derive_catalog(
            db,
            case.inputs(),
            build=build,
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
        assert before == {t.name: db.rows(t.name) for t in schema.tables}
        binding = projection.vocabulary.lookup(
            "jp", "type", case.plan.observations[0].content.type_raw
        )
        assert binding.code == "follower"
        assert binding.special_kinds == ("evolve",)
    config = object_value(parse(build.configuration.encode())) | text_configuration(
        case.plan, projection.vocabulary, ()
    )
    build = build.model_copy(update={"configuration": canonical(config).decode()})
    with create_database(schema) as db:
        case.parents(db)
        result = import_adopted_text(
            db,
            case.inputs(),
            build=build,
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
        assert result.context == build
        assert len(db.rows("printing_face_observation")) == len(case.plan.observations)
        assert db.rows("face_current")
    assert signed == {
        p.relative_to(copied.root).as_posix(): p.read_bytes()
        for p in (copied.root / "catalog-adoptions").rglob("*.yaml")
    }


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("code_path", "^Unsupported source parser recipe$"),
        ("config", "^Unsupported source recipe configuration$"),
    ],
)
def test_current_recipe_requires_supported_projection(
    case: tuple[Case, Path], schema: CompiledSchema, change: str, message: str
) -> None:
    inputs, archive = case
    build = inputs.build()
    config = object_value(parse(build.configuration.encode()))
    pin = object_value(object_value(config["catalog_source_recipes"])["exact-json-v1"])
    if change == "code_path":
        pin["code_path"] = "carddb/src/sve_carddb/routes/codec.py"
        pin["code_hash"] = digest(
            (inputs.repository / str(pin["code_path"])).read_bytes()
        )
    else:
        pin["config"] = {"unsupported": True}
        pin["config_hash"] = digest(canonical(pin["config"]))
    build = build.model_copy(update={"configuration": canonical(config).decode()})
    with create_database(schema) as db:
        with pytest.raises(ValueError, match=message):
            derive_catalog(
                db, inputs.inputs(), build=build, stores={"test-store": archive}
            )


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("locator", "^Adoption source locator is absent$"),
        ("text_hash", "^Source locator/exact text hash mismatch$"),
    ],
)
def test_historical_projection_still_requires_exact_frozen_field(
    case: tuple[Case, Path], schema: CompiledSchema, change: str, message: str
) -> None:
    inputs, archive = case
    inputs, _ = adopt_then_update(inputs, "carddb/uv.lock")
    name = "catalog-adoptions/vocabulary/shared/001.yaml"
    shard = object_value(read_yaml(inputs.root / name))
    members = array(shard["records"])
    member = object_value(members[0])
    value = object_value(object_value(member["data"])["value"])
    reference = object_value(
        object_value(array(value["raw_mappings"])[0])["source_ref"]
    )
    old = copy.deepcopy(reference)
    reference[change] = "/missing" if change == "locator" else "sha256:" + "0" * 64
    for item in array(member["evidence"]):
        evidence = object_value(item)
        if evidence["source_ref"] == old:
            evidence["source_ref"] = copy.deepcopy(reference)
    member["evidence"] = sorted(array(member["evidence"]), key=canonical)
    write(inputs.root, name, envelope(members, inputs.review))
    index(inputs.root)
    inputs = replace(inputs, revision=commit(inputs.repository))
    with create_database(schema) as db:
        with pytest.raises(ValueError, match=message):
            import_adoptions(
                db,
                inputs.inputs(),
                build=inputs.build(),
                stores={"test-store": archive},
            )
        assert not db.rows("source_record")


def test_current_identity_checks_disk_even_without_source_evidence_adoptions(
    tmp_path: Path, text_baseline: DisplayCase, schema: CompiledSchema
) -> None:
    repository = tmp_path / "repository"
    shutil.copytree(text_baseline.case.repository, repository)
    copied = replace(
        text_baseline.case, repository=repository, root=repository / "authored"
    )
    # No receipt evidence can pre-validate the current identity parser on this path.
    shutil.rmtree(copied.root / "catalog-adoptions")
    index(copied.root)
    parser = repository / "carddb/src/sve_carddb/extract/official_jp.py"
    parser.write_bytes(parser.read_bytes() + b"\n# Unloaded current identity parser.\n")
    copied = replace(copied, revision=commit(repository))
    case = replace(text_baseline, case=copied)
    inputs = copied.inputs()
    config = source_configuration(copied, inputs.configuration())
    config["text_observations"] = case.plan.configuration()
    build = case.context().model_copy(
        update={"configuration": canonical(config).decode()}
    )
    with create_database(schema) as db:
        with pytest.raises(
            ValueError, match=r"^Historical recipe implementation cannot be replayed$"
        ):
            derive_catalog(
                db,
                inputs,
                build=build,
                stores={"test-store": case.archive},
                text_plan=case.plan,
            )

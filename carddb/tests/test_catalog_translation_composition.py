"""Complete translation entry composition and rollback of both provenance graphs."""

import copy
import re
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import create_database
from sve_carddb.build_db.t1 import compile_build
from sve_carddb.build_inputs import BuildContext, SourceUse
from sve_carddb.catalog.adoption_importer import import_adoptions
from sve_carddb.frozen_sources import FrozenSources
from sve_carddb.manifest import Kind, Region
from sve_carddb.snapshot.values import canonical, digest, object_value, parse
from sve_carddb.source_archive import seal_batch
from sve_carddb.translations.sources import CODE_PATH
from sve_carddb.translations.sources import RUNTIME as TRANSLATION_RUNTIME

from .adoption_fixtures import REPO, commit, source_configuration
from .catalog_vocabulary_fixtures import RUNTIME, VocabularyCase, make_vocabulary_case
from .test_glossary_adoption import authored
from .test_source_archive import _put, _resource, _store
from .translation_fixtures import choice, envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.build_db import CompiledSchema
    from sve_carddb.catalog.adoption_importer import AdoptionInputs


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_build(("t0", "translation_evidence"))


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> VocabularyCase:
    case = make_vocabulary_case(tmp_path_factory.mktemp("composed-translations"))
    for name in TRANSLATION_RUNTIME:
        target = case.case.repository / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes((REPO / name).read_bytes())
    definitions = [authored(f"rule.synthetic_{number}") for number in range(264)]
    choices = [choice(f"rule.synthetic_{number}") for number in range(264)]
    write(
        case.case.root,
        {
            "translations/glossary/concepts/001.yaml": envelope(definitions),
            "translations/glossary/choices/001.yaml": envelope(choices),
        },
    )
    return replace(case, case=replace(case.case, revision=commit(case.case.repository)))


@pytest.fixture
def case(tmp_path: Path, baseline: VocabularyCase) -> VocabularyCase:
    repository = tmp_path / "repository"
    shutil.copytree(baseline.case.repository, repository)
    return replace(
        baseline,
        case=replace(
            baseline.case, repository=repository, root=repository / "authored"
        ),
    )


def context(case: VocabularyCase, inputs: AdoptionInputs) -> BuildContext:
    return BuildContext.from_inputs(
        case.case.revision,
        {
            name: (case.case.repository / name).read_bytes()
            for name in {*RUNTIME, *TRANSLATION_RUNTIME}
        },
        source_configuration(case.case, inputs.configuration()),
    )


def test_composes_all_264_synthetic_choices_and_keeps_review_policies_separate(
    case: VocabularyCase, schema: CompiledSchema
) -> None:
    inputs = replace(case.case.inputs(), include_translations=True)
    with create_database(schema) as db:
        result = import_adoptions(
            db, inputs, build=context(case, inputs), stores={"test-store": case.archive}
        )
        assert len(db.rows("glossary_term")) == 264
        assert len(db.rows("glossary_translation")) == 264
        assert len(db.rows("vocabulary")) == 3
        assert {"Synthetic human", "gbaian10"} <= {
            row.values["reviewed_by"] for row in db.rows("decision")
        }
        assert "translation_authored" in result.context.configuration
        assert any(
            row.values["authored_path"] == "authored/translations/index.yaml"
            for row in db.rows("source_record")
        )


def test_frozen_glossary_source_use_is_in_the_combined_input_record(
    case: VocabularyCase, schema: CompiledSchema, tmp_path: Path
) -> None:
    store = _store(tmp_path / "glossary-store")
    shutil.copytree(case.archive, store.root, dirs_exist_ok=True)
    raw = canonical(
        {
            "data_headers": {"result_code": 1},
            "data": {"skill_names": {"1": "Synthetic frozen term"}},
        }
    )
    resource = replace(
        _resource(
            "https://shadowverse-wb.com/web/CardList/cardList?lang=ja&offset=0",
            "raw/glossary.json",
            raw,
            Kind.API,
        ),
        region=Region.SVWB,
        content_type="application/json",
    )
    _put(store, resource, raw)
    batch = seal_batch(store)
    frozen = FrozenSources(store.root, store.store_id, batch.batch_id)
    source, _, _ = frozen.read(
        batch.inventory.current[0].source_version_id,
        parser_version="translation-svwb-v1",
    )
    locator = "/data/skill_names/1"
    definition = term("rule.frozen")
    object_value(definition["data"])["source_ref"] = {
        "store_id": store.store_id,
        "batch_id": batch.batch_id,
        "source_version_id": source.id,
        "parser": "translation-svwb-v1",
        "locator": locator,
        "text_hash": digest(b"Synthetic frozen term"),
    }
    write(
        case.case.root,
        {
            "translations/glossary/concepts/001.yaml": envelope([definition]),
            "translations/glossary/choices/001.yaml": envelope(
                [choice("rule.frozen", value="Synthetic chosen label")]
            ),
        },
    )
    case = replace(case, case=replace(case.case, revision=commit(case.case.repository)))
    inputs = replace(case.case.inputs(), include_translations=True)
    build = context(case, inputs)
    configuration = object_value(parse(build.configuration.encode()))
    configuration["translation_recipes"] = {
        "translation-svwb-v1": {
            "version": "translation-svwb-v1",
            "program_revision": case.case.revision,
            "code_path": CODE_PATH,
            "code_hash": digest((case.case.repository / CODE_PATH).read_bytes()),
            "config": {"provider": "svwb"},
            "config_hash": digest(canonical({"provider": "svwb"})),
        }
    }
    build = build.model_copy(
        update={"configuration": canonical(configuration).decode()}
    )
    with create_database(schema) as db:
        result = import_adoptions(
            db, inputs, build=build, stores={"test-store": store.root}
        )
        expected = SourceUse(
            source=source, usage="translation_evidence", locator=locator
        )
        assert [use for use in result.uses if use.usage == "translation_evidence"] == [
            expected
        ]
        assert (
            db.rows("glossary_term")[0].values["source_ja"] == "Synthetic frozen term"
        )
        assert any(row.values["id"] == source.id for row in db.rows("source_record"))
        result.verify(db, build, result.uses, complete=False)


@pytest.mark.parametrize("symlink", [False, True])
def test_present_translation_entry_requires_explicit_composition(
    case: VocabularyCase, schema: CompiledSchema, symlink: bool
) -> None:
    if symlink:
        path = case.case.root / "translations"
        shutil.rmtree(path)
        path.symlink_to("missing-translations")
    with create_database(schema) as db:
        with pytest.raises(
            ValueError,
            match=r"^Translation entry must be explicitly enabled for catalog composition$",
        ):
            import_adoptions(
                db,
                case.case.inputs(),
                build=case.build(),
                stores={"test-store": case.archive},
            )
        assert not db.rows("source_record")


def test_late_choice_error_rolls_back_catalog_and_all_glossary_rows(
    case: VocabularyCase, schema: CompiledSchema
) -> None:
    from sve_carddb.registry.storage import read_yaml  # ruff: ignore[import-outside-top-level] -- only malformed receipt construction needs detached YAML
    from sve_carddb.snapshot.values import array  # ruff: ignore[import-outside-top-level] -- mutate the final signed choice without bypassing its loader

    concept_path = "translations/glossary/concepts/001.yaml"
    choice_path = "translations/glossary/choices/001.yaml"
    definitions = [
        object_value(value)
        for value in array(
            object_value(read_yaml(case.case.root / concept_path))["records"]
        )
    ]
    choices = [
        object_value(value)
        for value in array(
            object_value(read_yaml(case.case.root / choice_path))["records"]
        )
    ]
    object_value(choices[-1]["data"])["origin"] = "official_svwb"
    write(
        case.case.root,
        {concept_path: envelope(definitions), choice_path: envelope(choices)},
    )
    case = replace(case, case=replace(case.case, revision=commit(case.case.repository)))
    inputs = replace(case.case.inputs(), include_translations=True)
    with create_database(schema) as db:
        with pytest.raises(
            ValueError, match=r"^Official choice lacks same-concept evidence$"
        ):
            import_adoptions(
                db,
                inputs,
                build=context(case, inputs),
                stores={"test-store": case.archive},
            )
        for table in (
            "source_record",
            "decision",
            "vocabulary",
            "language",
            "glossary_term",
            "glossary_translation",
        ):
            assert not db.rows(table)


def test_unsupported_vocabulary_choice_remains_atomic_and_fail_closed(
    case: VocabularyCase, schema: CompiledSchema
) -> None:
    selected = copy.deepcopy(choice())
    selected.update(
        record_key=canonical(
            ["vocabulary_choice", "type", "follower", "zh-Hant", 1]
        ).decode(),
        kind="vocabulary_choice",
        filing_key="vocabulary",
    )
    data = object_value(selected["data"])
    data.pop("term_id")
    data.update(vocabulary_kind="type", vocabulary_code="follower")
    shutil.rmtree(case.case.root / "translations")
    write(
        case.case.root,
        {"translations/glossary/vocabulary/001.yaml": envelope([selected])},
    )
    case = replace(case, case=replace(case.case, revision=commit(case.case.repository)))
    inputs = replace(case.case.inputs(), include_translations=True)
    message = "Vocabulary label projection belongs to #53; glossary import is atomic"
    with create_database(schema) as db:
        with pytest.raises(TypeError, match="^" + re.escape(message) + "$"):
            import_adoptions(
                db,
                inputs,
                build=context(case, inputs),
                stores={"test-store": case.archive},
            )
        assert not db.rows("vocabulary")
        assert not db.rows("source_record")

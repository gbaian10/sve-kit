"""Current values still need valid frozen sources and atomic authored provenance."""

import shutil
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import array, canonical, object_value, parse
from sve_carddb.core.provenance import BuildContext, SourceUse
from sve_carddb.domains.translations.glossary.importer import (
    import_glossary,
    populate_glossary,
)
from sve_carddb.domains.translations.inputs import Inputs, load_glossary

from .adoption_fixtures import commit
from .build_db_fixtures import seed
from .database_fixtures import DatabaseTemplate
from .test_translation_importer import frozen as _frozen_fixture

synthetic_frozen = _frozen_fixture

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.core.provenance import InputRecord

    from .test_translation_importer import Fixture


@pytest.fixture(scope="module")
def importer_template() -> DatabaseTemplate:
    schema = compile_build(("translation_evidence",))
    with create_database(schema) as db:
        seed(db)
        return DatabaseTemplate(schema, db._connection.serialize())


@pytest.mark.parametrize("bad_source", [False, True])
def test_current_import_rechecks_source_without_creating_decision(
    synthetic_frozen: Fixture,
    importer_template: DatabaseTemplate,
    tmp_path: Path,
    bad_source: bool,
) -> None:
    frozen = synthetic_frozen
    shutil.copytree(frozen.root, tmp_path / "repository")
    root = tmp_path / "repository"
    old = load_glossary(root / "authored")
    for path, _, _ in old.shards:
        rows = [
            r.model_dump(mode="json", round_trip=True)
            for r in old.current_records()
            if (r.kind == "glossary_term") == ("concepts" in path)
        ]
        if "choices" in path:
            rows[0]["low_confidence"] = True
            if bad_source:
                ref = object_value(
                    object_value(
                        array(object_value(rows[0]["data"])["concept_evidence"])[0]
                    )["target_ref"]
                )
                ref["text_hash"] = "sha256:" + "0" * 64
        payload: dict[str, JsonValue] = {
            "format": 2,
            "kind": "translation_shard",
            "records": list[JsonValue](rows),
        }
        (root / "authored" / path).write_bytes(canonical(payload))
    revision = commit(root)
    inputs = Inputs(root / "authored", root, revision)
    config = object_value(parse(frozen.build.configuration.encode()))
    config.update(inputs.configuration())
    build = BuildContext.from_inputs(frozen.program, config)
    with importer_template.copy() as db:
        if bad_source:
            with pytest.raises(
                ValueError, match=r"^Evidence must locate exact hash-verified text$"
            ):
                import_glossary(
                    db, inputs, build=build, stores={"test-store": frozen.store}
                )
            assert not db.rows("glossary_term")
        else:
            before = db.rows("decision")
            result = import_glossary(
                db, inputs, build=build, stores={"test-store": frozen.store}
            )
            assert db.rows("decision") == before
            row = db.rows("glossary_translation")[0].values
            assert row["text"] == "合成乙"
            assert "decision_id" not in row
            assert row["authored_source_id"] is not None
            assert row["origin"] == "official"
            assert row["low_confidence"] is True
            assert len(result.uses) == 2
            _assert_stage_uses(importer_template, inputs, build, frozen, result)


def _assert_stage_uses(
    template: DatabaseTemplate,
    inputs: Inputs,
    build: BuildContext,
    frozen: Fixture,
    result: InputRecord,
) -> None:
    sources = frozen.sources(build=build)
    _, _, source = sources.text(frozen.refs[0])
    sources.uses.append(
        SourceUse(source=source, usage="prior_stage", locator="synthetic-prior-stage")
    )
    previous = tuple(sources.uses)
    with template.copy() as db, db.transaction():
        isolated = populate_glossary(
            db,
            inputs,
            build=build,
            stores={"test-store": frozen.store},
            sources=sources,
        )
    assert isolated.uses == result.uses
    assert tuple(sources.uses) == previous

"""Current name rules keep own-source eligibility without review receipts."""

import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.build import create_database
from sve_carddb.build.t1 import compile_build
from sve_carddb.core.json import canonical, digest, object_value, parse
from sve_carddb.core.provenance import BuildContext
from sve_carddb.domains.digital.name_policies.application import Inputs, populate
from sve_carddb.domains.digital.name_policies.projection import materialize, prepare
from sve_carddb.domains.registry.storage import read_yaml
from sve_carddb.domains.translations.glossary.records import (
    AssignmentData,
    AssignmentRecord,
    ChoiceRecord,
    TermRecord,
)
from sve_carddb.domains.translations.inputs import Inputs as TranslationInputs
from sve_carddb.domains.translations.models import PrintingOwner, RevisionOwner
from sve_carddb.domains.translations.names.resolve import prepare as prepare_names

from .adoption_fixtures import commit
from .database_fixtures import DatabaseTemplate
from .digital_name_policy_fixtures import NAMES
from .name_application_fixtures import ApplicationCase, application_case, printed_owner
from .translation_fixtures import choice, name_term, write

RUNTIME: tuple[str, ...] = ()


if TYPE_CHECKING:
    from pathlib import Path


@pytest.fixture(scope="module")
def base_case(tmp_path_factory: pytest.TempPathFactory) -> ApplicationCase:
    return application_case(tmp_path_factory.mktemp("current-names"))


def current_case(  # ruff: ignore[too-many-locals] -- one synthetic fixture composes policy, glossary, publication and schema inputs
    case: ApplicationCase,
    root: Path,
    *,
    excluded: bool = False,
    ambiguous: bool = False,
    choice_text: str | None = None,
    override: bool = False,
) -> ApplicationCase:
    shutil.copytree(case.fixture.root, root)
    base = root / "authored/digital-name-policies" / NAMES
    document = object_value(read_yaml(base / "current.yaml"))
    content = object_value(document["content"])
    content["excluded_names"] = (
        [
            {
                "source_lang": "ja",
                "source_name_hash": digest(b"Synthetic card"),
                "reason": "Synthetic exclusion",
            }
        ]
        if excluded
        else []
    )
    if override:
        with case.database.copy() as db:
            owner = str(db.rows("face_revision")[0].values["id"])
        content["name_overrides"] = [
            {
                "owner": {"kind": "face_revision", "revision_id": owner},
                "source_hash": digest(b"Synthetic card"),
                "term_id": "term:name.test",
            }
        ]
    policy: dict[str, JsonValue] = {
        "digital_name_policy_format": 2,
        "kind": "digital_name_policy",
        "policy_id": NAMES,
        "purpose": "names",
        "content": content,
        "origin": "official",
        "low_confidence": False,
        "note": "",
    }
    for path in base.iterdir():
        path.unlink()
    (base / "current.yaml").write_bytes(canonical(policy))
    index_path = root / "authored/digital-name-policies/index.yaml"
    index = object_value(read_yaml(index_path))
    index["digital_name_policy_index_format"] = 2
    object_value(index["policies"])[NAMES] = {
        "path": "digital-name-policies/" + NAMES + "/current.yaml",
        "hash": digest(canonical(policy)),
    }
    index_path.write_bytes(canonical(index))
    values: list[dict[str, JsonValue]] = []
    for identifier in ["name.test", "name.other"] if ambiguous else ["name.test"]:
        values.append(  # ruff: ignore[manual-list-comprehension] -- synthetic source construction keeps each concept identifier explicit
            TermRecord.model_validate_json(canonical(name_term(identifier))).model_dump(
                round_trip=True, mode="json"
            )
        )
    if choice_text is not None:
        selected = ChoiceRecord.model_validate_json(
            canonical(choice("name.test", value=choice_text))
        )
        values.append(
            selected.model_copy(
                update={"origin": "machine", "low_confidence": True}
            ).model_dump(mode="json", round_trip=True)
        )
    ordered = values
    write(
        root / "authored",
        {
            "translations/glossary/concepts/001.yaml": {
                "translation_authored_format": 2,
                "kind": "translation_shard",
                "records": list[JsonValue](ordered),
            }
        },
    )
    revision = commit(root)
    inputs = Inputs(root / "authored", root, revision)
    translations = TranslationInputs(root / "authored", root, revision)
    snapshot = translations.load()
    config = (
        object_value(parse(case.context.configuration.encode()))
        | inputs.configuration()
        | translations.configuration()
    )
    object_value(config["catalog_registry"])["authored_revision"] = revision
    context = BuildContext.from_inputs(revision, config)
    schema = compile_build(("t0", "translation_names"))
    with create_database(schema) as db, case.database.copy() as old:
        with db.transaction():
            for table in schema.tables:
                if old.has_table(table.name):
                    for row in old.rows(table.name):
                        db.insert(table.name, dict(row.values))
        database = DatabaseTemplate(schema, db._connection.serialize())
    originals = tuple(
        (r.data.id, str(r.data.authored_source_ja))
        for r in snapshot.current_records()
        if isinstance(r, TermRecord)
    )
    return replace(
        case,
        inputs=inputs,
        context=context,
        replay=replace(case.replay, snapshot=snapshot, originals=originals),
        database=database,
        fixture=replace(
            case.fixture,
            digital=replace(
                case.fixture.digital,
                root=root,
                store=root / case.fixture.digital.store.relative_to(case.fixture.root),
            ),
        ),
    )


@pytest.mark.parametrize("excluded", [False, True])
def test_current_policy_no_audit_and_ambiguous_name_is_independent(
    base_case: ApplicationCase,
    tmp_path: Path,
    excluded: bool,
) -> None:
    case = current_case(base_case, tmp_path / "repo", excluded=excluded, ambiguous=True)
    with case.database.copy() as db, db.transaction():
        decisions = db.rows("decision")
        result = populate(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        assert db.rows("decision") == decisions
        assert result.report["warning_owners"] == 1
        assert result.report["covered_owners"] == int(not excluded)
        if not excluded:
            row = db.rows("translation")[0].values
            assert row["text"] == "合成測試名"
            assert row["origin"] == "official"
            assert row["low_confidence"] is False
            assert "decision_id" not in row
            assert "status" not in row
            assert len(result.bindings) == 1
        else:
            assert not db.rows("translation")


def test_current_render_quality_changes_id_but_note_does_not(
    base_case: ApplicationCase,
    tmp_path: Path,
) -> None:
    case = current_case(base_case, tmp_path / "repo")
    with case.database.copy() as db, db.transaction():
        plan = prepare(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        owner = plan.owners[0]
        assert owner.candidate is not None
        context = "test-context"
        db.insert(
            "translation_context",
            {
                "id": context,
                "source_unit_id": owner.source.unit_id,
                "semantic_variant": "default",
            },
        )
        first = materialize(db, owner.source, context, owner.candidate)
        assert materialize(db, owner.source, context, owner.candidate) == first
        low = materialize(
            db, owner.source, context, replace(owner.candidate, low_confidence=True)
        )
        assert low != first


@pytest.mark.parametrize("override", [False, True])
def test_current_explicit_override_and_low_machine_name(
    base_case: ApplicationCase,
    tmp_path: Path,
    override: bool,
) -> None:
    case = current_case(
        base_case, tmp_path / "repo", choice_text="合成低信心名稱", override=override
    )
    with case.database.copy() as db, db.transaction():
        result = populate(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        row = db.rows("translation")[0].values
        if override:
            assert row["text"] == "合成低信心名稱"
            assert row["origin"] == "machine"
            assert row["low_confidence"] is True
        else:
            assert row["text"] == "合成測試名"
            assert row["origin"] == "official"
            assert row["low_confidence"] is False
        assert len(result.bindings) == 1


def test_current_variant_cannot_select_two_concepts(
    base_case: ApplicationCase,
    tmp_path: Path,
) -> None:
    case = current_case(base_case, tmp_path / "repo", ambiguous=True)
    with case.database.copy() as db, db.transaction():
        printed = printed_owner(db, case)
        owners: tuple[RevisionOwner | PrintingOwner, ...] = (
            RevisionOwner(
                kind="face_revision",
                revision_id=str(db.rows("face_revision")[0].values["id"]),
            ),
            PrintingOwner(
                kind="printing_face",
                printing_id=printed.identifier,
                face_id=str(printed.face_id),
            ),
        )
        assignments = []
        for owner, concept in zip(owners, ("name.test", "name.other"), strict=True):
            record = AssignmentRecord(
                kind="context_assignment",
                origin="project",
                low_confidence=False,
                note="",
                data=AssignmentData(
                    owner=owner,
                    field="name",
                    ordinal=None,
                    source_hash=digest(b"Synthetic card"),
                    variant="distinct",
                    concept_key=concept,
                    reason="Synthetic homonym selection",
                ),
            )
            assignments.append(record.model_dump(mode="json", round_trip=True))
        ordered_assignments = assignments
        content = canonical(
            {
                "translation_authored_format": 2,
                "kind": "translation_shard",
                "records": list[JsonValue](ordered_assignments),
            }
        )
        snapshot = replace(
            case.replay.snapshot,
            shards=(
                *case.replay.snapshot.shards,
                ("translations/overrides/names/001.yaml", content, content),
            ),
        )
        inputs = TranslationInputs(
            case.inputs.root, case.inputs.repository, case.inputs.authored_revision
        )
        with pytest.raises(
            ValueError, match=r"^Name semantic variant selects conflicting concepts$"
        ):
            prepare_names(
                snapshot, dict(case.replay.originals), inputs, case.sources(), db
            )

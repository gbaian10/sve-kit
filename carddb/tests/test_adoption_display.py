"""Independently sealed sources validate the entire display adoption transaction."""

import copy
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import CompiledSchema, create_database
from sve_carddb.build_db.t0 import compile_t0
from sve_carddb.catalog.adoption_importer import import_adoption_build
from sve_carddb.catalog.adoption_loader import load_adoptions
from sve_carddb.catalog.adoption_models import RouteRecord
from sve_carddb.catalog.adoption_validation import route_value
from sve_carddb.registry.storage import read_yaml
from sve_carddb.snapshot.values import array, canonical, digest, object_value

from .adoption_display_fixtures import DisplayCase, current_case, make_display_case
from .adoption_fixtures import commit, envelope, fields, index, successor, write

if TYPE_CHECKING:
    from pathlib import Path

    from sve_carddb.catalog.adoption_loader import Entry


@pytest.fixture(scope="module", params=[False, True], ids=["unique-routes", "variants"])
def baseline(
    tmp_path_factory: pytest.TempPathFactory, request: pytest.FixtureRequest
) -> DisplayCase:
    return make_display_case(
        tmp_path_factory.mktemp("display") / "case", variants=bool(request.param)
    )


@pytest.fixture
def case(tmp_path: Path, baseline: DisplayCase) -> DisplayCase:
    repository = tmp_path / "repository"
    shutil.copytree(baseline.case.repository, repository)
    return replace(
        baseline,
        case=replace(
            baseline.case,
            repository=repository,
            root=repository / "authored",
            review=copy.deepcopy(baseline.case.review),
        ),
    )


@pytest.fixture(scope="module")
def schema() -> CompiledSchema:
    return compile_t0()


def test_atomic_names_route_override_and_default(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    with create_database(schema) as db:
        case.parents(db)
        inputs, defaults = import_adoption_build(
            db,
            case.inputs(),
            build=case.context(),
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
        assert len(db.rows("card_route")) == 2
        assert len(db.rows("rules_name")) == 1
        assert len(db.rows("face_rules_name")) == 1
        assert db.rows("face_rules_name")[0].values["role"] == "treated_as"
        assert defaults[0].method == "override"
        assert defaults[0].printing_id == max(
            o.printing_id for o in case.plan.observations
        )
        assert inputs.uses
        assert {o.card.source.id for o in case.plan.observations} <= {
            u.source.id for u in inputs.uses
        }


@pytest.mark.parametrize(
    ("area", "mutation", "message"),
    [
        ("rules-names", "basis", "name basis hash"),
        ("rules-names", "missing_observation", "printing observation closure"),
        ("rules-names", "rules_missing", "separate rules evidence"),
        ("rules-names", "names_duplicate", "exact unique regional strings"),
        ("rules-names", "observation_face", "observation face mismatch"),
        ("rules-names", "observation_source", "printing/face/source mismatch"),
        ("defaults", "missing_candidate", "candidate closure/hash"),
        ("defaults", "hash", "candidate closure/hash"),
        ("defaults", "target", "Reviewed default target is not displayable"),
        ("routes", "missing_candidate", "candidate closure"),
        ("routes", "hash", "ordering/hash"),
        ("routes", "identity_hash", "identity reference mismatch"),
        ("routes", "identity_state", "Invalid adoption fields"),
        ("routes", "variant", "candidate identity"),
        ("routes", "target", "absent/retired"),
    ],
)
def test_reconstructed_scope_rejects_one_signed_violation(  # ruff: ignore[complex-structure,too-many-branches,too-many-statements] -- each branch injects one independently signed semantic defect
    case: DisplayCase, schema: CompiledSchema, area: str, mutation: str, message: str
) -> None:
    entry: Entry = "catalog-adoptions" if area == "rules-names" else "display-overrides"
    path = f"{entry}/{area}/shared/001.yaml"
    if not (case.case.root / path).exists():
        pytest.skip("Unique official numbers do not require manual route adoption")
    shard = object_value(read_yaml(case.case.root / path))
    member, data, _ = fields(shard)
    value = object_value(data["value"])
    if mutation == "basis":
        value["name_basis_hash"] = "sha256:" + "f" * 64
    elif mutation in {"hash", "missing_candidate"}:
        if mutation == "hash":
            value["candidates_hash"] = "sha256:" + "f" * 64
        else:
            value["candidates"] = array(value["candidates"])[1:]
            value["candidates_hash"] = digest(canonical(value["candidates"]))
    elif mutation == "target":
        value["printing_id"] = "p:synthetic-absent"
        for dep in array(data["dependencies"]):
            if object_value(dep)["table"] == "printing":
                object_value(object_value(dep)["key"])["id"] = value["printing_id"]
        data["dependencies"] = sorted(array(data["dependencies"]), key=canonical)
    elif mutation in {"identity_hash", "identity_state", "variant"}:
        candidate = object_value(array(value["candidates"])[0])
        if mutation == "identity_hash":
            object_value(candidate["identity_ref"])["record_hash"] = (
                "sha256:" + "f" * 64
            )
        else:
            candidate[
                "card_no_state" if mutation == "identity_state" else "variant_key"
            ] = "provisional" if mutation == "identity_state" else "synthetic-other"
        value["candidates_hash"] = digest(canonical(value["candidates"]))
    elif mutation == "missing_observation":
        value["observations"] = array(value["observations"])[1:]
    elif mutation == "rules_missing":
        member["evidence"] = [
            e
            for e in array(member["evidence"])
            if "source_ref" in object_value(e)
            and str(object_value(object_value(e)["source_ref"])["locator"]).endswith(
                "/name"
            )
        ]
    elif mutation == "names_duplicate":
        # Different frozen sources can carry the same exact name; the names array is still a set.
        value["names"] = sorted(
            [
                {"kind": "source", "source_ref": ref}
                for ref in {canonical(ref): ref for ref in case.name_refs}.values()
            ],
            key=canonical,
        )
    elif mutation == "observation_face":
        object_value(array(value["observations"])[0])["face_id"] = "f:synthetic-wrong"
        value["observations"] = sorted(array(value["observations"]), key=canonical)
    else:
        observations = array(value["observations"])
        other = next(
            r
            for r in case.name_refs
            if r != object_value(observations[0])["source_ref"]
        )
        object_value(observations[0])["source_ref"] = other
        value["observations"] = sorted(observations, key=canonical)
    write(case.case.root, path, envelope([member], case.case.review, entry=entry))
    index(case.case.root, entry=entry)
    revised = replace(
        case, case=replace(case.case, revision=commit(case.case.repository))
    )
    with create_database(schema) as db:
        revised.parents(db)
        before = (
            len(db.rows("decision")),
            len(db.rows("language")),
            len(db.rows("product_family")),
        )
        with pytest.raises(ValueError, match=message):
            import_adoption_build(
                db,
                revised.inputs(),
                build=revised.context(),
                stores={"test-store": revised.archive},
                text_plan=revised.plan,
            )
        assert not db.rows("card")
        assert not db.rows("source_record")
        assert before == (
            len(db.rows("decision")),
            len(db.rows("language")),
            len(db.rows("product_family")),
        )


def test_current_input_pin_is_required(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    context = case.context()
    config = object_value(__import__("json").loads(context.configuration))
    config["text_observations"] = {}
    context = context.model_copy(update={"configuration": canonical(config).decode()})
    with create_database(schema) as db:
        case.parents(db)
        with pytest.raises(ValueError, match="pin complete text/identity inputs"):
            import_adoption_build(
                db,
                case.inputs(),
                build=context,
                stores={"test-store": case.archive},
                text_plan=case.plan,
            )
        assert not db.rows("card")


def test_policy_pin_is_required(case: DisplayCase, schema: CompiledSchema) -> None:
    context = case.context()
    config = object_value(__import__("json").loads(context.configuration))
    del config["general_rarity_policy_hash"]
    context = context.model_copy(update={"configuration": canonical(config).decode()})
    with create_database(schema) as db:
        with pytest.raises(ValueError, match="pin approved rarity policy"):
            import_adoption_build(
                db,
                case.inputs(),
                build=context,
                stores={"test-store": case.archive},
                text_plan=case.plan,
            )
        assert not db.rows("decision")


def test_default_and_special_name_withdrawals_use_only_terminal_value(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    successor(case.case.root, "defaults", None, entry="display-overrides")
    successor(case.case.root, "rules-names", None)
    revised = replace(
        case, case=replace(case.case, revision=commit(case.case.repository))
    )
    with create_database(schema) as db:
        revised.parents(db)
        _, defaults = import_adoption_build(
            db,
            revised.inputs(),
            build=revised.context(),
            stores={"test-store": revised.archive},
            text_plan=revised.plan,
        )
        assert not db.rows("default_printing_override")
        assert not db.rows("face_rules_name")
        assert defaults[0].method == "candidate_general"
        assert len(db.rows("card_route")) == 2


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("same_names", None),
        ("same_name_reprint", None),
        ("name", "Stale special name exact observed name basis"),
        ("rules", "Stale special relation exact rules basis"),
        ("default_competitor", "Stale default printing candidate closure"),
    ],
)
def test_current_frozen_changes_revalidate_relevant_content(
    case: DisplayCase,
    tmp_path: Path,
    schema: CompiledSchema,
    change: str,
    message: str | None,
) -> None:
    if (case.case.root / "display-overrides/routes/shared/001.yaml").exists():
        pytest.skip("Version-change fixture uses unique card numbers")
    if change == "same_name_reprint":
        successor(case.case.root, "defaults", None, entry="display-overrides")
    revised = current_case(
        case,
        tmp_path / "current",
        name="Synthetic revised name" if change == "name" else "Synthetic name",
        rule="Synthetic revised relation rule"
        if change == "rules"
        else "Synthetic relation rule",
        extra=change in {"same_name_reprint", "default_competitor"},
    )
    with create_database(schema) as db:
        revised.parents(db)
        if message is not None:
            with pytest.raises(ValueError, match=message):
                import_adoption_build(
                    db,
                    revised.inputs(),
                    build=revised.context(),
                    stores={"test-store": revised.archive},
                    text_plan=revised.plan,
                )
            assert not db.rows("card")
        else:
            result, _ = import_adoption_build(
                db,
                revised.inputs(),
                build=revised.context(),
                stores={"test-store": revised.archive},
                text_plan=revised.plan,
            )
            assert len(db.rows("face_rules_name")) == 1
            old = {str(ref["source_version_id"]) for ref in revised.name_refs}
            new = {o.card.source.id for o in revised.plan.observations}
            assert old.isdisjoint(new)
            assert old | new <= {u.source.id for u in result.uses}


@pytest.mark.parametrize(
    ("field", "replacement", "message"),
    [
        ("raw_hash", "sha256:" + "e" * 64, "image descriptor/raw hash"),
        ("face_id", "f:synthetic-absent", "printing/face association"),
        ("printing_id", "p:synthetic-absent", "printing/face association"),
    ],
)
def test_image_has_independent_hash_and_physical_face_guard(
    case: DisplayCase,
    schema: CompiledSchema,
    field: str,
    replacement: str,
    message: str,
) -> None:
    path = "catalog-adoptions/rules-names/shared/001.yaml"
    shard = object_value(read_yaml(case.case.root / path))
    member, _, _ = fields(shard)
    for item in array(member["evidence"]):
        evidence = object_value(item)
        if "image_ref" in evidence:
            object_value(evidence["image_ref"])[field] = replacement
    member["evidence"] = sorted(array(member["evidence"]), key=canonical)
    write(case.case.root, path, envelope([member], case.case.review))
    index(case.case.root)
    revised = replace(
        case, case=replace(case.case, revision=commit(case.case.repository))
    )
    with create_database(schema) as db:
        revised.parents(db)
        with pytest.raises(ValueError, match=message):
            import_adoption_build(
                db,
                revised.inputs(),
                build=revised.context(),
                stores={"test-store": revised.archive},
                text_plan=revised.plan,
            )
        assert not db.rows("card")


def test_caller_cannot_forge_current_physical_rarity(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    evidence = dict(case.plan.identity.evidence)
    key = next(iter(evidence))
    original = evidence[key]
    evidence[key] = replace(
        original, faces=(replace(original.faces[0], rarity_raw="BR"),)
    )
    identity = replace(case.plan.identity, evidence=evidence)
    revised = replace(
        case,
        plan=replace(
            case.plan,
            identity=identity,
            eligible=replace(case.plan.eligible, evidence=evidence),
        ),
    )
    with create_database(schema) as db:
        revised.parents(db)
        with pytest.raises(ValueError, match="physical evidence cannot be reproduced"):
            import_adoption_build(
                db,
                revised.inputs(),
                build=revised.context(),
                stores={"test-store": revised.archive},
                text_plan=revised.plan,
            )
        assert not db.rows("card")


def test_published_route_changes_and_withdrawals_require_repair(
    case: DisplayCase, schema: CompiledSchema
) -> None:
    path = "display-overrides/routes/shared/001.yaml"
    if not (case.case.root / path).exists():
        pytest.skip("Unique official route has no manual selection")
    with create_database(schema) as db:
        case.parents(db)
        import_adoption_build(
            db,
            case.inputs(),
            build=case.context(),
            stores={"test-store": case.archive},
            text_plan=case.plan,
        )
        previous = db.rows("card_route")
        shard = object_value(read_yaml(case.case.root / path))
        member, _, _ = fields(shard)
        record = RouteRecord.model_validate_json(canonical(member))
        changed = record.model_copy(
            update={"data": record.data.model_copy(update={"value": None})}
        )
        with pytest.raises(
            ValueError, match="Published route withdrawal requires identity repair"
        ):
            route_value(changed, db)
        value = record.data.value
        assert value is not None
        other = next(
            c.printing_id
            for c in value.candidates
            if c.printing_id != value.printing_id
        )
        changed = record.model_copy(
            update={
                "data": record.data.model_copy(
                    update={"value": value.model_copy(update={"printing_id": other})}
                )
            }
        )
        with pytest.raises(
            ValueError, match="Published route target change requires identity repair"
        ):
            route_value(changed, db)
        assert db.rows("card_route") == previous


@pytest.mark.parametrize("withdraw", [False, True])
def test_route_terminal_change_cannot_bypass_permanent_comparison(
    case: DisplayCase, withdraw: bool
) -> None:
    path = "display-overrides/routes/shared/001.yaml"
    if not (case.case.root / path).exists():
        pytest.skip("Unique official route has no manual selection")
    _, data, _ = fields(object_value(read_yaml(case.case.root / path)))
    value = None if withdraw else object_value(copy.deepcopy(data["value"]))
    if value is not None:
        value["printing_id"] = next(
            object_value(c)["printing_id"]
            for c in array(value["candidates"])
            if object_value(c)["printing_id"] != value["printing_id"]
        )
    successor(case.case.root, "routes", value, entry="display-overrides")
    if value is not None:
        name = "display-overrides/routes/shared/002.yaml"
        revised, revised_data, _ = fields(
            object_value(read_yaml(case.case.root / name))
        )
        revised_data["dependencies"] = [
            {"table": "printing", "key": {"id": value["printing_id"]}}
        ]
        write(
            case.case.root,
            name,
            envelope([revised], case.case.review, entry="display-overrides"),
        )
        index(case.case.root, entry="display-overrides")
    with pytest.raises(ValueError, match="permanent-entry comparison"):
        load_adoptions(case.case.root, entry="display-overrides")

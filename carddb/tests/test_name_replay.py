"""Closed override histories, complete raw identity and current name applicability."""


# ruff: file-ignore[pytest-raises-with-multiple-statements] -- graph guards execute at transaction exit and the exact message identifies each guard

import re
import sqlite3
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.translations.importer import import_glossary
from sve_carddb.translations.loader import load_glossary
from sve_carddb.translations.models import AssignmentData, ConceptData
from sve_carddb.translations.name_build import (
    NameOwner,
    default_name_context,
    name_source,
)
from sve_carddb.translations.name_replay import replay_names

from .adoption_fixtures import commit
from .identity_transition_fixtures import merge_record as merge_record  # ruff: ignore[useless-import-alias] -- module-scoped synthetic transition baseline
from .name_build_fixtures import template
from .name_replay_fixtures import (
    Case,
    Mixed,
    copied,
    human,
    make_case,
    make_mixed_case,
    name_term,
)
from .test_glossary_adoption import checked
from .translation_fixtures import envelope, write

if TYPE_CHECKING:
    from pathlib import Path

    from .database_fixtures import DatabaseTemplate


def shards(
    *records: dict[str, JsonValue], terms: list[dict[str, JsonValue]] | None = None
) -> dict[str, dict[str, JsonValue]]:
    result = {
        "translations/glossary/concepts/001.yaml": envelope(terms or [name_term()])
    }
    counts: dict[str, int] = {}
    for record in records:
        filing = str(record["filing_key"])
        counts[filing] = counts.get(filing, 0) + 1
        result[f"translations/overrides/{filing}/{counts[filing]:03}.yaml"] = human(
            [record]
        )
    return result


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("name-replay-template"))


@pytest.fixture(scope="module")
def mixed_baseline(tmp_path_factory: pytest.TempPathFactory) -> Mixed:
    return make_mixed_case(tmp_path_factory.mktemp("name-mixed-template"))


def english_concept(
    case: Case, mixed: Mixed, key: str = "name.synthetic"
) -> dict[str, JsonValue]:
    record = case.concept(key=key)
    data = object_value(record["data"])
    subject = object_value(data["subject"])
    subject.update(
        card_id=mixed.printing.card_id,
        face_id=mixed.printing.source_face_map[0].face_id,
        source_lang="en",
        source_hash=mixed.name_ref.text_hash,
    )
    data["source_ref"] = mixed.name_ref.model_dump(mode="json")
    record["record_key"] = canonical(["card_name_concept", subject, 1]).decode()
    return record


@pytest.fixture(scope="module")
def database() -> DatabaseTemplate:
    return template()


@pytest.fixture
def case(tmp_path: Path, baseline: Case) -> Case:
    return copied(baseline, tmp_path / "case")


def test_full_override_replay_and_atomic_glossary_import(
    case: Case, database: DatabaseTemplate
) -> None:
    replay, inputs, build = case.replay(shards(case.concept(), case.assignment()))
    assert replay.snapshot.review_counts()["human_sampled_rows"] == 3
    with database.copy() as db:
        case.frozen.publish(db)
        with db.transaction():
            db.update(
                "face_revision", {"id": "link-revision"}, {"id": case.revision_id}
            )
            result = replay.resolve(db, NameOwner("face_revision", case.revision_id))
            assert result is not None
            assert (result.term_id, result.variant, result.reason) == (
                "term:name.synthetic",
                "synthetic",
                "selected",
            )
            assert len(result.record_hashes) == 2
        imported = import_glossary(
            db, inputs, build=build, stores={"test-store": case.frozen.store}
        )
        assert {use.usage for use in imported.uses} >= {
            "translation_evidence",
            "name_identity",
            "name_identity_observation",
        }
        assert any(
            row.values["parser_version"] == "name-identity-v1"
            for row in db.rows("source_record")
        )
        assert {row.values["category"] for row in db.rows("decision")} >= {
            "card_name_concept",
            "context_assignment",
        }
        assert not db.rows("translation_use")


def test_offline_replays_name_source_closure_independently(
    case: Case, database: DatabaseTemplate
) -> None:
    from sve_carddb.build_inputs import input_record  # ruff: ignore[import-outside-top-level] -- verify the final build boundary, not only importer output
    from sve_carddb.catalog.adoption_importer import AdoptionInputs  # ruff: ignore[import-outside-top-level] -- reuse the real offline entry
    from sve_carddb.snapshot.offline import _translation_uses  # ruff: ignore[import-outside-top-level] -- independent consumer of immutable records

    _, inputs, build = case.replay(shards(case.concept(), case.assignment()))
    adoptions = AdoptionInputs(
        inputs.root,
        inputs.repository,
        inputs.authored_revision,
        ("catalog-adoptions",),
        include_translations=True,
    )
    expected = _translation_uses(adoptions, build, {"test-store": case.frozen.store})
    with database.copy() as db:
        case.frozen.publish(db)
        imported = import_glossary(
            db, inputs, build=build, stores={"test-store": case.frozen.store}
        )
        assert imported.uses == expected
        imported.verify(db, build, expected)
        incomplete = input_record(
            build, (use for use in imported.uses if use.usage != "name_identity")
        )
        with pytest.raises(
            ValueError, match=r"^Build input use closure or context mismatch$"
        ):
            incomplete.verify(db, build, expected)


@pytest.mark.parametrize("guard", ["complete", "association", "identity_state"])
def test_full_identity_evidence_guards(case: Case, guard: str) -> None:
    from sve_carddb.registry.storage import (  # ruff: ignore[import-outside-top-level] -- reseal a structurally valid synthetic registry
        _area,
        load,
        read_yaml,
        relayout,
        write_files,
    )
    from sve_carddb.translations.models import IdentityBasis  # ruff: ignore[import-outside-top-level] -- immutable basis oracle
    from sve_carddb.translations.name_replay import IdentityEvidence  # ruff: ignore[import-outside-top-level] -- isolate each independently reachable evidence guard

    root = case.frozen.root / "authored"
    _, entries = load(root)
    for entry in entries.values():
        if guard == "identity_state" and entry.kind == "card":
            entry.data["identity_state"] = "provisional"
        elif guard != "identity_state" and entry.kind in {"printing", "art"}:
            object_value(entry.data["observation"])["observation_hash"] = (
                "sha256:" + "0" * 64
            )
    reviews = {
        (_area(entry), entry.owner): ("gbaian10", "2026-10-01")
        for entry in entries.values()
    }
    write_files(relayout(root, list(entries.values()), reviews))
    basis = IdentityBasis(
        authored_revision=commit(case.frozen.root),
        registry_index_hash=digest(canonical(read_yaml(root / "ids/index.yaml"))),
        transition_index_hash=None,
    )
    evidence = IdentityEvidence(case.frozen.sources(), basis.authored_revision)
    message = {
        "complete": "Name identity complete frozen observation closure is absent",
        "association": "Name override physical observation differs from identity basis",
        "identity_state": "Name override requires confirmed physical identity",
    }[guard]
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        if guard == "complete":
            evidence.complete(
                basis, ((case.frozen.jp.store_id, case.frozen.jp.batch_id),)
            )
        else:
            evidence.association(basis, case.frozen.jp)


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("reason", " ", "Name assignment reason must be nonblank"),
        ("field", "effect", "Input should be 'name'"),
        ("ordinal", 0, "Input should be null"),
        ("variant", "not a code", "String should match pattern '^"),
    ],
)
def test_assignment_data_is_name_only(
    baseline: Case, field: str, value: JsonValue, message: str
) -> None:
    data = object_value(baseline.assignment()["data"])
    data[field] = value
    if field == "variant":
        from pydantic import ValidationError  # ruff: ignore[import-outside-top-level] -- verify one constrained-field diagnostic without printing imported values

        with pytest.raises(ValidationError) as error:
            AssignmentData.model_validate_json(canonical(data))
        assert len(error.value.errors()) == 1
        assert error.value.errors()[0]["loc"] == ("variant",)
    else:
        with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
            checked(AssignmentData, data)


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("reviewer", "Name override requires the maintainer human reviewer"),
        ("policy", "Name concept decision policy mismatch"),
        ("term", "Name concept requires an adopted card-name term"),
        ("assignment_key", "Name assignment requires an adopted card-name concept key"),
        ("area", "Translation record is in the wrong authored area"),
    ],
)
def test_complete_loader_refusals(case: Case, change: str, message: str) -> None:
    record = case.assignment() if change == "assignment_key" else case.concept()
    data = object_value(record["data"])
    if change == "term":
        data["term_id"] = "term:name.absent"
    if change == "assignment_key":
        data["concept_key"] = "name.absent"
    inputs = shards(record)
    path = next(path for path in inputs if "/overrides/" in path)
    decision = object_value(array(inputs[path]["decisions"])[0])
    if change == "reviewer":
        decision["reviewed_by"] = "Synthetic AI"
    if change == "policy":
        decision["policy_id"] = "delegated-glossary-v1"
    if change == "area":
        inputs[path.replace("/overrides/", "/glossary/")] = inputs.pop(path)
        # Keep the glossary concept and override filing in independent directories.
        inputs["translations/glossary/names/001.yaml"] = inputs.pop(
            "translations/glossary/concepts/001.yaml"
        )
        object_value(
            array(inputs["translations/glossary/names/001.yaml"]["records"])[0]
        )["filing_key"] = "names"
        inputs["translations/glossary/names/001.yaml"] = envelope(
            [
                object_value(
                    array(inputs["translations/glossary/names/001.yaml"]["records"])[0]
                )
            ]
        )
    write(case.frozen.root / "authored", inputs)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load_glossary(case.frozen.root / "authored")


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("card", "Name override frozen evidence belongs to another card or face"),
        ("face", "Name override frozen evidence belongs to another card or face"),
        ("registry_hash", "Name identity registry index hash mismatch"),
        ("transition", "Name identity transition index pin mismatch"),
        ("field", "Name override frozen source has no unique physical face mapping"),
        ("lang", "Name concept language differs from physical source"),
    ],
)
def test_physical_identity_refusals(case: Case, change: str, message: str) -> None:
    record = case.concept()
    data = object_value(record["data"])
    subject = object_value(data["subject"])
    if change in {"card", "face"}:
        subject[change + "_id"] = "wrong"
        record["record_key"] = canonical(["card_name_concept", subject, 1]).decode()
    elif change in {"registry_hash", "transition"}:
        object_value(data["identity_basis"])[
            "registry_index_hash"
            if change == "registry_hash"
            else "transition_index_hash"
        ] = "sha256:" + "a" * 64
    elif change == "field":
        ref = object_value(data["source_ref"])
        ref.update(locator="/faces/0/text", text_hash=digest(b"Synthetic rule."))
        subject["source_hash"] = ref["text_hash"]
        record["record_key"] = canonical(["card_name_concept", subject, 1]).decode()
    elif change == "lang":
        subject["source_lang"] = "en"
        record["record_key"] = canonical(["card_name_concept", subject, 1]).decode()
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        case.replay(shards(record))


def test_old_basis_is_verified_from_git_not_current_disk(case: Case) -> None:
    path = case.frozen.root / "authored/ids/index.yaml"
    old = path.read_bytes()
    path.write_bytes(b"unusable current index")
    # A concept's explicitly pinned basis is independent from the current working tree.
    replay, _, _ = case.replay(shards(case.concept()))
    assert replay.uses
    path.write_bytes(old)


@pytest.mark.parametrize("state", ["retired", "provisional"])
@pytest.mark.parametrize(
    "owner",
    [
        NameOwner("face_revision", "revision"),
        NameOwner("printing_face", "printing", "face"),
    ],
)
def test_inactive_identity_has_no_name_use(
    database: DatabaseTemplate, owner: NameOwner, state: str
) -> None:
    with database.copy() as db, db.transaction():
        db.update("card", {"id": "card"}, {"identity_state": state})
        assert name_source(db, owner) is None
        assert default_name_context(db, owner) is None
        assert not db.rows("translation_use")


def test_ambiguous_concept_requires_variant_and_exact_owner(
    case: Case, database: DatabaseTemplate
) -> None:
    terms = [name_term(), name_term("name.other")]
    replay, _, _ = case.replay(shards(case.concept(), terms=terms))
    with database.copy() as db:
        case.frozen.publish(db)
        result = replay.resolve(db, NameOwner("face_revision", "link-revision"))
        assert result is not None
        assert result.reason == "ambiguous_name_concept"
    replay, _, _ = case.replay(shards(case.concept(), case.assignment(), terms=terms))
    with database.copy() as db:
        case.frozen.publish(db)
        with db.transaction():
            db.update(
                "face_revision", {"id": "link-revision"}, {"id": case.revision_id}
            )
            result = replay.resolve(db, NameOwner("face_revision", case.revision_id))
            assert result is not None
            assert result.term_id == "term:name.synthetic"
            # Same name, different revision: no inherited semantic assignment.
            other = replay.resolve(db, NameOwner("face_revision", "revision"))
            assert other is not None
            assert other.reason == "missing_name_concept"


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        ("reason", " ", "Name concept reason must be nonblank"),
        ("term_id", None, "Initial name concept cannot be withdrawn"),
        (
            "source_hash",
            "sha256:" + "a" * 64,
            "Name concept subject and frozen name hash disagree",
        ),
    ],
)
def test_concept_data_guards(
    baseline: Case, field: str, value: JsonValue, message: str
) -> None:
    data = object_value(baseline.concept()["data"])
    if field == "source_hash":
        object_value(data["subject"])[field] = value
    else:
        data[field] = value
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        checked(ConceptData, data)


def test_unsupported_override_kind_is_refused_without_echoing_content(
    case: Case,
) -> None:
    record = case.assignment()
    record["kind"] = "translation_override"
    object_value(record["data"])["reason"] = "DO_NOT_ECHO_SYNTHETIC_INPUT"
    write(case.frozen.root / "authored", shards(record))
    with pytest.raises(
        ValueError, match=r"^Invalid translation authored fields at records\.0$"
    ) as error:
        load_glossary(case.frozen.root / "authored")
    assert "DO_NOT_ECHO" not in str(error.value)


@pytest.mark.parametrize("kind", ["context_assignment", "card_name_concept"])
@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("duplicate", "Duplicate immutable translation record"),
        ("gap", "Translation adoption sequence gap or fork"),
        ("predecessor", "Translation predecessor mismatch"),
        ("empty_sample", "Translation decision requires actual checked members"),
        ("partial_confirmed", "Confirmed translation must check every member"),
    ],
)
def test_override_history_and_real_samples(
    case: Case, kind: str, change: str, message: str
) -> None:
    first = case.assignment() if kind == "context_assignment" else case.concept()
    checksum = digest(canonical(first))
    first_shard = human([first])
    previous: dict[str, JsonValue] = {
        "record_key": first["record_key"],
        "record_hash": checksum,
        "decision_id": first_shard["default_decision_id"],
    }
    if change == "predecessor":
        previous["record_hash"] = "sha256:" + "a" * 64
    second = (
        case.assignment(number=3 if change == "gap" else 2, previous=previous)
        if kind == "context_assignment"
        else case.concept(number=3 if change == "gap" else 2, previous=previous)
    )
    inputs = shards(first, first if change == "duplicate" else second)
    path = min(path for path in inputs if "/overrides/" in path)
    if change in {"empty_sample", "partial_confirmed"}:
        if change == "partial_confirmed":
            # Distinct choice keys in one actual checked batch.
            second["data"] = object_value(second["data"]) | {
                "adoption_no": 1,
                "predecessor": None,
            }
            if kind == "context_assignment":
                owner: dict[str, JsonValue] = {
                    "kind": "face_revision",
                    "revision_id": "other",
                }
                object_value(second["data"])["owner"] = owner
                second["record_key"] = canonical(
                    [kind, owner, "name", None, 1]
                ).decode()
            else:
                subject = object_value(object_value(second["data"])["subject"]) | {
                    "face_id": "other"
                }
                object_value(second["data"])["subject"] = subject
                second["record_key"] = canonical([kind, subject, 1]).decode()
            inputs.pop(sorted(path for path in inputs if "/overrides/" in path)[1])
            inputs[path] = human([first, second])
        decision = object_value(array(inputs[path]["decisions"])[0])
        decision["sample_ids"] = (
            [] if change == "empty_sample" else [array(decision["sample_ids"])[0]]
        )
    write(case.frozen.root / "authored", inputs)
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        load_glossary(case.frozen.root / "authored")


def test_withdrawn_concept_returns_to_default_instead_of_inheriting_alias(
    case: Case, database: DatabaseTemplate
) -> None:
    first = case.concept(key="name.alias")
    previous = {
        "record_key": first["record_key"],
        "record_hash": digest(canonical(first)),
        "decision_id": human([first])["default_decision_id"],
    }
    withdrawn = case.concept(key="name.alias", number=2, previous=previous)
    object_value(withdrawn["data"])["term_id"] = None
    terms = [name_term(), name_term("name.alias", "Synthetic alias")]
    initial, _, _ = case.replay(shards(first, terms=terms))
    replay, _, _ = case.replay(shards(first, withdrawn, terms=terms))
    with database.copy() as db:
        case.frozen.publish(db)
        owner = NameOwner("face_revision", "link-revision")
        result = initial.resolve(db, owner)
        assert result is not None
        assert result.term_id == "term:name.alias"
        result = replay.resolve(db, owner)
        assert result is not None
        assert result.term_id == "term:name.synthetic"
        assert not result.record_hashes


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("absent", "Name assignment requires its exact frozen name evidence"),
        (
            "wrong_revision",
            "Name assignment frozen evidence does not reproduce revision owner",
        ),
        (
            "wrong_printing",
            "Name assignment evidence belongs to another printing owner",
        ),
        ("digital_source", "Name override must locate a physical name source"),
    ],
)
def test_assignment_frozen_evidence_and_owner(
    case: Case, change: str, message: str
) -> None:
    record = case.assignment()
    data = object_value(record["data"])
    if change == "absent":
        record["evidence"] = []
    elif change == "wrong_revision":
        object_value(data["owner"])["revision_id"] = "wrong"
    elif change == "wrong_printing":
        data["owner"] = {
            "kind": "printing_face",
            "printing_id": "wrong",
            "face_id": case.frozen.face.id,
        }
    else:
        ref = case.frozen.refs[0]
        data["source_hash"] = ref.text_hash
        record["evidence"] = [
            {"source_ref": ref.model_dump(mode="json"), "role": "name"}
        ]
    record["record_key"] = canonical(
        ["context_assignment", data["owner"], "name", None, 1]
    ).decode()
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        case.replay(shards(record))


def test_replacing_a_bad_historical_record_does_not_hide_its_evidence(
    case: Case,
) -> None:
    first = case.concept()
    object_value(first["data"])["source_ref"] = object_value(
        object_value(first["data"])["source_ref"]
    ) | {"locator": "/faces/0/text", "text_hash": digest(b"Synthetic rule.")}
    object_value(object_value(first["data"])["subject"])["source_hash"] = digest(
        b"Synthetic rule."
    )
    first["record_key"] = canonical(
        ["card_name_concept", object_value(first["data"])["subject"], 1]
    ).decode()
    previous = {
        "record_key": first["record_key"],
        "record_hash": digest(canonical(first)),
        "decision_id": human([first])["default_decision_id"],
    }
    second = case.concept(number=2, previous=previous)
    object_value(second["data"])["subject"] = object_value(first["data"])["subject"]
    object_value(second["data"])["source_ref"] = object_value(first["data"])[
        "source_ref"
    ]
    object_value(second["data"])["term_id"] = None
    second["record_key"] = canonical(
        ["card_name_concept", object_value(second["data"])["subject"], 2]
    ).decode()
    with pytest.raises(
        ValueError,
        match=r"^Name override frozen source has no unique physical face mapping$",
    ):
        case.replay(shards(first, second))


@pytest.mark.parametrize("kind", ["face_revision", "printing_face"])
@pytest.mark.parametrize("state", ["retired", "provisional"])
def test_direct_use_cannot_bypass_identity_gate(
    database: DatabaseTemplate, kind: str, state: str
) -> None:
    from .test_name_build import _use  # ruff: ignore[import-outside-top-level] -- shared valid direct-write row recipe

    owner = (
        NameOwner("face_revision", "revision")
        if kind == "face_revision"
        else NameOwner("printing_face", "printing", "face")
    )
    with database.copy() as db:
        with db.transaction():
            context = default_name_context(db, owner)
            assert context is not None
        with pytest.raises(
            sqlite3.IntegrityError,
            match=r"^Cross-table check failed: name_use_confirmed_identity$",
        ):
            with db.transaction():
                db.update("card", {"id": "card"}, {"identity_state": state})
                db.insert(
                    "translation_use",
                    _use(
                        context,
                        **(
                            {
                                "face_revision_id": None,
                                "printing_id": "printing",
                                "face_id": "face",
                            }
                            if kind == "printing_face"
                            else {}
                        ),
                    ),
                )


@pytest.mark.parametrize("change", ["association", "exact"])
def test_assignment_cannot_choose_an_unrelated_concept(
    case: Case, database: DatabaseTemplate, change: str
) -> None:
    assignment = case.assignment(key="name.other")
    terms = [name_term(), name_term("name.other", "Synthetic other name")]
    records = [case.concept(), assignment] if change == "association" else [assignment]
    replay, _, _ = case.replay(shards(*records, terms=terms))
    message = (
        "Name assignment contradicts its concept association"
        if change == "association"
        else "Name assignment differs from exact adopted name concepts"
    )
    with database.copy() as db:
        case.frozen.publish(db)
        with db.transaction():
            db.update(
                "face_revision", {"id": "link-revision"}, {"id": case.revision_id}
            )
        with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
            replay.resolve(db, NameOwner("face_revision", case.revision_id))


def test_one_variant_cannot_select_two_same_language_concepts(
    case: Case, database: DatabaseTemplate
) -> None:
    first, second = case.assignment(), case.assignment(key="name.other")
    data = object_value(second["data"])
    owner: dict[str, JsonValue] = {
        "kind": "printing_face",
        "printing_id": case.frozen.printing.id,
        "face_id": case.frozen.face.id,
    }
    data["owner"] = owner
    second["record_key"] = canonical(
        ["context_assignment", owner, "name", None, 1]
    ).decode()
    replay, _, _ = case.replay(
        shards(first, second, terms=[name_term(), name_term("name.other")])
    )
    with database.copy() as db:
        case.frozen.publish(db)
        with db.transaction():
            db.update(
                "face_revision", {"id": "link-revision"}, {"id": case.revision_id}
            )
        with pytest.raises(
            ValueError, match=r"^Name semantic variant selects conflicting concepts$"
        ):
            replay.resolve(db, NameOwner("face_revision", case.revision_id))


@pytest.mark.parametrize(
    "owner",
    [
        NameOwner("face_revision", "revision"),
        NameOwner("face_revision", "back-revision"),
        NameOwner("printing_face", "printing", "face"),
    ],
)
def test_default_exact_name_resolution_keeps_every_owner_source(
    case: Case, database: DatabaseTemplate, owner: NameOwner
) -> None:
    terms = [
        name_term("name.current", "Synthetic text"),
        name_term("name.back", "Synthetic back name"),
        name_term("name.printed", "Synthetic old name"),
    ]
    replay, _, _ = case.replay(shards(terms=terms))
    with database.copy() as db:
        result = replay.resolve(db, owner)
        assert result is not None
        expected = {
            "revision": "term:name.current",
            "back-revision": "term:name.back",
            "printing": "term:name.printed",
        }
        assert result.term_id == expected[owner.identifier]
        assert result.variant == "default"
        assert not result.record_hashes
        assert not result.decision_ids
        with db.transaction():
            db.update(
                "printing_face",
                {"printing_id": "printing", "face_id": "face"},
                {"printed_text_state": "unknown", "printed_name_unit_id": None},
            )
        assert (
            replay.resolve(db, NameOwner("printing_face", "printing", "face")) is None
        )
        english = replay.resolve(db, NameOwner("face_revision", "english-revision"))
        assert english is not None
        assert english.reason == "missing_name_concept"


def test_source_change_does_not_carry_an_old_assignment(
    case: Case, database: DatabaseTemplate
) -> None:
    replay, _, _ = case.replay(
        shards(
            case.assignment(),
            terms=[name_term(), name_term("name.other", "Synthetic other name")],
        )
    )
    with database.copy() as db:
        case.frozen.publish(db)
        with db.transaction():
            db.update(
                "face_revision",
                {"id": "link-revision"},
                {"id": case.revision_id, "name_unit_id": "other-name"},
            )
        result = replay.resolve(db, NameOwner("face_revision", case.revision_id))
        assert result is not None
        assert (result.term_id, result.variant) == ("term:name.other", "default")
        assert not result.record_hashes


@pytest.mark.parametrize(
    "change", ["missing", "symlink", "empty_transition", "nonempty_transition"]
)
def test_immutable_identity_tree_guards(
    case: Case, change: str, merge_record: dict[str, object]
) -> None:
    from sve_carddb.registry.storage import read_yaml  # ruff: ignore[import-outside-top-level] -- immutable synthetic tree oracle

    from .identity_transition_fixtures import chain, write_chain  # ruff: ignore[import-outside-top-level] -- reuse closed synthetic transition shapes

    record = case.concept()
    basis = object_value(object_value(record["data"])["identity_basis"])
    if change == "missing":
        basis["authored_revision"] = "0" * 40
        message = "Name identity immutable tree is unavailable"
    elif change == "symlink":
        (case.frozen.root / "authored/registry/synthetic-link").symlink_to(
            "synthetic-target"
        )
        basis["authored_revision"] = commit(case.frozen.root)
        message = "Name identity immutable tree contains a nonregular input"
    else:
        write_chain(
            case.frozen.root / "authored",
            [] if change == "empty_transition" else chain([merge_record]),
        )
        basis["authored_revision"] = commit(case.frozen.root)
        basis["transition_index_hash"] = digest(
            canonical(
                read_yaml(case.frozen.root / "authored/identity-transitions/index.yaml")
            )
        )
        message = "Name identity transitions require complete effective evidence replay"
    if change == "empty_transition":
        replay, _, _ = case.replay(shards(record))
        assert replay.uses
    else:
        with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
            case.replay(shards(record))


@pytest.mark.parametrize("kind", ["assignment", "concept"])
def test_historical_runtime_comment_does_not_invalidate_name_adoption(
    case: Case,
    kind: str,
) -> None:
    from sve_carddb.translations.sources import CODE_PATH, Sources  # ruff: ignore[import-outside-top-level] -- exercise the historical/current distinction explicitly

    record = case.assignment() if kind == "assignment" else case.concept()
    inputs, build = case.stage(shards(record))
    # The immutable program pin continues to verify after disk code changes.
    (case.frozen.root / CODE_PATH).write_bytes(b"# changed runtime only\n")
    sources = Sources(
        {"test-store": case.frozen.store}, case.frozen.root, build, historical=True
    )
    snapshot = inputs.load()
    replay, _ = replay_names(
        snapshot, {"term:name.synthetic": "Synthetic card"}, inputs, sources
    )
    assert replay.uses


def test_english_source_projection_keeps_both_physical_faces() -> None:
    from sve_carddb.sources.official_en import card_url  # ruff: ignore[import-outside-top-level] -- regional URL evidence
    from sve_carddb.translations.sources import project  # ruff: ignore[import-outside-top-level] -- exact physical projection boundary

    from .en_extract_fixtures import page  # ruff: ignore[import-outside-top-level] -- invented two-sided English page

    lang, data = project(page(double=True), card_url("SYNⓈ-01aEN"), "en")
    assert lang == "en"
    assert [
        object_value(face)["name"] for face in array(object_value(data)["faces"])
    ] == [
        "Synthetic front",
        "Synthetic back",
    ]
    with pytest.raises(ValueError, match=r"^EN glossary source URL mismatch$"):
        project(page(), "https://shadowverse-evolve.com/cardlist/?cardno=SYN001", "en")


def test_assignment_accepts_a_pinned_empty_transition_index(case: Case) -> None:
    from .identity_transition_fixtures import write_chain  # ruff: ignore[import-outside-top-level] -- distinguish a real empty index from absence

    write_chain(case.frozen.root / "authored", [])
    replay, _, _ = case.replay(shards(case.assignment()))
    assert replay.assignment_languages


def test_malformed_offline_batches_cannot_enter_identity_replay(case: Case) -> None:
    from sve_carddb.snapshot.values import parse  # ruff: ignore[import-outside-top-level] -- independent recipe input mutation

    config = object_value(parse(case.frozen.build.configuration.encode()))
    config["offline_recipe"] = {"store_id": False, "sources": [{"card_batch": None}]}
    case = replace(case, frozen=replace(case.frozen, build=case.frozen.changed(config)))
    with pytest.raises(
        ValueError, match=r"^Name identity offline source batch is malformed$"
    ):
        case.replay(shards(case.concept()))


@pytest.mark.parametrize("include_en", [False, True])
def test_mixed_registry_replays_jp_and_en_card_descriptors(
    mixed_baseline: Mixed, tmp_path: Path, include_en: bool
) -> None:
    case = copied(mixed_baseline.case, tmp_path / "mixed")
    records = [case.concept()]
    if include_en:
        records.append(english_concept(case, mixed_baseline))
    replay, _, _ = case.replay(shards(*records))
    observations = [
        use for use in replay.uses if use.usage == "name_identity_observation"
    ]
    assert len(observations) == 2
    assert {use.source.url for use in observations} == {
        "https://shadowverse-evolve.com/cardlist/?cardno=SYN-001",
        "https://en.shadowverse-evolve.com/cards/?cardno=SYN-EN001",
    }
    identity_uses = [use for use in replay.uses if use.usage == "name_identity"]
    assert len(identity_uses) == (2 if include_en else 1)


def test_english_exact_string_cannot_borrow_a_japanese_concept(
    case: Case, database: DatabaseTemplate
) -> None:
    replay, _, _ = case.replay(shards(terms=[name_term(text="Synthetic English name")]))
    with database.copy() as db:
        result = replay.resolve(db, NameOwner("face_revision", "english-revision"))
        assert result is not None
        assert result.term_id is None
        assert result.reason == "missing_name_concept"


@pytest.mark.parametrize("include_name", [False, True])
def test_only_card_name_concepts_participate_in_exact_name_resolution(
    case: Case, database: DatabaseTemplate, include_name: bool
) -> None:
    trait = name_term("trait.synthetic", "Synthetic text")
    object_value(trait["data"])["category"] = "trait"
    keyword = name_term("keyword.synthetic", "Synthetic text")
    object_value(keyword["data"])["category"] = "keyword"
    terms = [trait, keyword]
    if include_name:
        terms.append(name_term("name.current", "Synthetic text"))
    replay, _, _ = case.replay(shards(terms=terms))
    with database.copy() as db:
        result = replay.resolve(db, NameOwner("face_revision", "revision"))
        assert result is not None
        assert result.term_id == ("term:name.current" if include_name else None)
        assert result.reason == ("selected" if include_name else "missing_name_concept")


def test_renamed_owner_cannot_reuse_a_concept_association(
    case: Case, database: DatabaseTemplate
) -> None:
    replay, _, _ = case.replay(
        shards(
            case.concept(key="name.alias"),
            terms=[
                name_term("name.alias", "Synthetic alias"),
                name_term("name.other", "Synthetic other name"),
            ],
        )
    )
    with database.copy() as db:
        case.frozen.publish(db)
        with db.transaction():
            db.update(
                "face_revision", {"id": "link-revision"}, {"name_unit_id": "other-name"}
            )
        result = replay.resolve(db, NameOwner("face_revision", "link-revision"))
        assert result is not None
        assert (result.term_id, result.variant, result.reason) == (
            "term:name.other",
            "default",
            "selected",
        )
        assert not result.record_hashes
        assert not result.decision_ids


def test_same_hash_and_variant_in_different_languages_do_not_conflict(
    mixed_baseline: Mixed, tmp_path: Path, database: DatabaseTemplate
) -> None:
    case = copied(mixed_baseline.case, tmp_path / "mixed")
    jp, en = case.assignment(), case.assignment(key="name.other")
    data = object_value(en["data"])
    owner: dict[str, JsonValue] = {
        "kind": "face_revision",
        "revision_id": mixed_baseline.revision_id,
    }
    data["owner"] = owner
    data["source_hash"] = mixed_baseline.name_ref.text_hash
    en["record_key"] = canonical(
        ["context_assignment", owner, "name", None, 1]
    ).decode()
    en["evidence"] = [
        {"source_ref": mixed_baseline.name_ref.model_dump(mode="json"), "role": "name"}
    ]
    replay, _, _ = case.replay(
        shards(
            jp,
            en,
            english_concept(case, mixed_baseline, "name.other"),
            terms=[name_term(), name_term("name.other", "Synthetic other concept")],
        )
    )
    assert dict(replay.assignment_languages) == {
        str(jp["record_key"]): "ja",
        str(en["record_key"]): "en",
    }
    with database.copy() as db:
        case.frozen.publish(db)
        with db.transaction():
            db.update(
                "face_revision", {"id": "link-revision"}, {"id": case.revision_id}
            )
        result = replay.resolve(db, NameOwner("face_revision", case.revision_id))
        assert result is not None
        assert (result.term_id, result.variant) == ("term:name.synthetic", "synthetic")


@pytest.mark.parametrize("kind", ["assignment", "concept"])
def test_each_override_decision_audits_only_its_own_immutable_identity_basis(
    case: Case, database: DatabaseTemplate, kind: str
) -> None:
    from sve_carddb.catalog.adoption_sources import PinnedRepository  # ruff: ignore[import-outside-top-level] -- verify hashes directly against Git blobs
    from sve_carddb.registry.storage import read_yaml  # ruff: ignore[import-outside-top-level] -- this fixture's registry content stays unchanged across revisions

    first = case.assignment() if kind == "assignment" else case.concept()
    previous: dict[str, JsonValue] = {
        "record_key": first["record_key"],
        "record_hash": digest(canonical(first)),
        "decision_id": human([first])["default_decision_id"],
    }
    (case.frozen.root / "synthetic-history.txt").write_text(
        "A separate immutable identity revision.\n"
    )
    revision = commit(case.frozen.root)
    second = (
        case.assignment(number=2, previous=previous)
        if kind == "assignment"
        else case.concept(number=2, previous=previous)
    )
    object_value(object_value(second["data"])["identity_basis"])[
        "authored_revision"
    ] = revision
    _, inputs, build = case.replay(shards(first, second))
    paths = {"authored/ids/index.yaml"} | {
        "authored/" + path
        for path in object_value(
            object_value(read_yaml(inputs.root / "ids/index.yaml"))["includes"]
        )
    }
    repository = PinnedRepository(inputs.repository)
    with database.copy() as db:
        case.frozen.publish(db)
        import_glossary(
            db, inputs, build=build, stores={"test-store": case.frozen.store}
        )
        sources = {row.values["id"]: row.values for row in db.rows("source_record")}
        for record in (first, second):
            decision = human([record])["default_decision_id"]
            own = str(
                object_value(object_value(record["data"])["identity_basis"])[
                    "authored_revision"
                ]
            )
            edges = [
                row.values
                for row in db.rows("decision_source")
                if row.values["decision_id"] == decision
                and str(row.values["role"]).startswith("name_identity:")
            ]
            assert len(edges) == len(paths)
            actual = set()
            for edge in edges:
                source = sources[edge["source_id"]]
                assert source["kind"] == "authored"
                assert source["parser_version"] == "name-identity-v1"
                assert edge["locator"] == source["authored_path"]
                assert edge["quote"] is None
                actual.add(
                    (
                        source["authored_revision"],
                        source["authored_path"],
                        source["sha256"],
                    )
                )
            assert actual == {
                (own, path, digest(repository.read(own, path))) for path in paths
            }


def test_historical_translation_sources_forward_mode_to_identity_replay(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    import sve_carddb.catalog.adoption_sources as pinned_module  # ruff: ignore[import-outside-top-level] -- relocate only the synthetic runtime used by the real identity verifier
    from sve_carddb.catalog.adoption_models import Batch, ReviewContext  # ruff: ignore[import-outside-top-level] -- actual reviewed identity closure
    from sve_carddb.translations.sources import Sources  # ruff: ignore[import-outside-top-level] -- constructor forwarding is the reviewed behavior

    config = object_value(parse(case.frozen.build.configuration.encode()))
    path = "carddb/src/sve_carddb/extract/official_jp.py"
    parser = "official-jp-exact-v1"
    config["catalog_source_recipes"] = {
        parser: {
            "version": parser,
            "program_revision": case.frozen.program,
            "code_path": path,
            "code_hash": digest((case.frozen.root / path).read_bytes()),
            "config": {},
            "config_hash": digest(canonical({})),
        }
    }
    build = case.frozen.changed(config)
    review = ReviewContext(
        context=build,
        source_batches=(
            Batch(store_id=case.frozen.jp.store_id, batch_id=case.frozen.jp.batch_id),
        ),
    )
    (case.frozen.root / path).write_bytes(
        (case.frozen.root / path).read_bytes() + b"\n# Synthetic runtime change.\n"
    )
    monkeypatch.setattr(
        pinned_module,
        "__file__",
        str(case.frozen.root / "carddb/src/sve_carddb/catalog/adoption_sources.py"),
    )
    sources = Sources(
        {"test-store": case.frozen.store}, case.frozen.root, build, historical=True
    )
    sources.identities.verify_printing(
        case.frozen.printing, review, case.frozen.jp.source_version_id
    )
    assert any(
        use.usage == "catalog_reviewed_identity" for use in sources.identities.uses
    )
    current = Sources({"test-store": case.frozen.store}, case.frozen.root, build)
    with pytest.raises(
        ValueError, match=r"^Historical recipe implementation cannot be replayed$"
    ):
        current.identities.verify_printing(
            case.frozen.printing, review, case.frozen.jp.source_version_id
        )


def test_multiple_printing_variants_cannot_make_a_face_mapping_unique(
    case: Case,
) -> None:
    from sve_carddb.registry.storage import (  # ruff: ignore[import-outside-top-level] -- a valid registry permits distinct variants of the same regional number
        Entry,
        _area,
        load,
        read_yaml,
        relayout,
        write_files,
    )

    root = case.frozen.root / "authored"
    index, records = load(root)
    entries = list(records.values())
    original = next(entry for entry in entries if entry.kind == "printing")
    identifier = "p:" + "1" * 32
    entries.extend(
        [
            Entry(
                record_key="printing:" + identifier,
                kind="printing",
                owner=original.owner,
                data=original.data
                | {"id": identifier, "variant_key": "synthetic-alternate"},
            ),
            Entry(
                record_key="card_int_id:" + identifier,
                kind="card_int_id",
                owner=original.owner,
                data={
                    "int_id": index.next_int_id["jp"],
                    "printing_id": identifier,
                    "allocated_at": "2026-10-03",
                },
            ),
        ]
    )
    reviews = {
        (_area(entry), entry.owner): ("gbaian10", "2026-10-03") for entry in entries
    }
    write_files(relayout(root, entries, reviews))
    record = case.concept()
    basis = object_value(object_value(record["data"])["identity_basis"])
    basis.update(
        authored_revision=commit(case.frozen.root),
        registry_index_hash=digest(canonical(read_yaml(root / "ids/index.yaml"))),
    )
    with pytest.raises(
        ValueError,
        match=r"^Name override frozen source has no unique physical face mapping$",
    ):
        case.replay(shards(record))

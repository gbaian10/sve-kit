"""Per-record immutable bases, explicit adoption bases and current-owner isolation."""

import re
from typing import TYPE_CHECKING

import pytest

from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.translations.models import AssignmentData, IdentityBasis
from sve_carddb.translations.name_build import NameOwner
from sve_carddb.translations.name_replay import (
    IdentityEvidence,
    replay_names,
    verify_name_adoption_base,
)
from sve_carddb.translations.sources import Sources

from .adoption_fixtures import commit, git
from .name_replay_fixtures import Case, Mixed, copied, human, make_case, make_mixed_case
from .test_glossary_adoption import checked
from .test_name_replay import database as database  # ruff: ignore[useless-import-alias] -- register the shared synthetic DB fixture
from .test_name_replay import shards

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue

    from sve_carddb.build_db import Value
    from sve_carddb.registry.records import Region
    from sve_carddb.text_observations.archive import FrozenTexts
    from sve_carddb.text_observations.models import TextCard

    from .database_fixtures import DatabaseTemplate


@pytest.fixture(scope="module")
def baseline(tmp_path_factory: pytest.TempPathFactory) -> Case:
    return make_case(tmp_path_factory.mktemp("name-history-template"))


@pytest.fixture
def case(baseline: Case, tmp_path: Path) -> Case:
    return copied(baseline, tmp_path / "case")


@pytest.fixture(scope="module")
def mixed(tmp_path_factory: pytest.TempPathFactory) -> Mixed:
    return make_mixed_case(tmp_path_factory.mktemp("name-history-mixed-template"))


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("missing", "Field required"),
        ("null", "Input should be an object"),
        ("extra", "Extra inputs are not permitted"),
    ],
)
def test_assignment_requires_its_own_closed_basis(
    baseline: Case, change: str, message: str
) -> None:
    data = object_value(baseline.assignment()["data"])
    if change == "missing":
        del data["identity_basis"]
    elif change == "null":
        data["identity_basis"] = None
    else:
        object_value(data["identity_basis"])["unknown"] = True
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        checked(AssignmentData, data)


@pytest.mark.parametrize("kind", ["assignment", "concept"])
def test_readable_sibling_basis_is_not_a_consumer_ancestor(
    case: Case, kind: str
) -> None:
    root = case.frozen.root
    branch = git(root, "branch", "--show-current")
    git(root, "switch", "-c", "synthetic-sibling")
    (root / "synthetic-history.txt").write_text("Sibling history.\n")
    sibling = commit(root)
    git(root, "switch", branch)
    record = case.assignment() if kind == "assignment" else case.concept()
    object_value(object_value(record["data"])["identity_basis"])[
        "authored_revision"
    ] = sibling
    with pytest.raises(
        ValueError, match=r"^Name identity basis is not a consumer ancestor$"
    ):
        case.replay(shards(record))


@pytest.mark.parametrize("kind", ["assignment", "concept"])
def test_feature_consumer_can_replay_but_cannot_adopt_a_feature_basis(
    case: Case, kind: str
) -> None:
    base = git(case.frozen.root, "rev-parse", "HEAD")
    # The reader needs no main ref, while adoption checks take an explicit base.
    git(case.frozen.root, "switch", "-c", "synthetic-feature")
    (case.frozen.root / "synthetic-history.txt").write_text("Feature history.\n")
    feature = commit(case.frozen.root)
    record = case.assignment() if kind == "assignment" else case.concept()
    object_value(object_value(record["data"])["identity_basis"])[
        "authored_revision"
    ] = feature
    replay, inputs, _ = case.replay(shards(record))
    assert replay.uses
    with pytest.raises(
        ValueError, match=r"^Name adoption basis is outside explicit base history$"
    ):
        verify_name_adoption_base(inputs, base)
    verify_name_adoption_base(inputs, feature)


def test_equal_basis_is_valid_for_the_reader(case: Case) -> None:
    evidence = IdentityEvidence(case.frozen.sources(), case.frozen.authored)
    registry = evidence.registry(
        IdentityBasis.model_validate_json(canonical(case.basis))
    )
    assert registry.records


@pytest.mark.parametrize("revision", ["main", "short", "--help"])
def test_consumer_revision_cannot_be_a_ref_or_git_option(
    baseline: Case, revision: str
) -> None:
    with pytest.raises(
        ValueError, match=r"^Name identity consumer revision must be a full Git SHA$"
    ):
        IdentityEvidence(baseline.frozen.sources(), revision)


def test_feature_consumer_with_a_base_background_passes_adoption(case: Case) -> None:
    base = git(case.frozen.root, "rev-parse", "HEAD")
    git(case.frozen.root, "switch", "-c", "synthetic-feature")
    replay, inputs, _ = case.replay(shards(case.assignment(), case.concept()))
    assert replay.uses
    assert inputs.authored_revision != base
    verify_name_adoption_base(inputs, base)


@pytest.mark.parametrize("base", ["main", "short", "0" * 40])
def test_adoption_base_must_be_an_explicit_available_commit(
    case: Case, base: str
) -> None:
    _, inputs, _ = case.replay(shards(case.assignment()))
    message = (
        "Name identity Git ancestry is unavailable"
        if base == "0" * 40
        else "Name adoption base revision must be a full Git SHA"
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        verify_name_adoption_base(inputs, base)


def _printing_assignment(case: Case) -> dict[str, JsonValue]:
    record = case.assignment()
    data = object_value(record["data"])
    data["owner"] = {
        "kind": "printing_face",
        "printing_id": case.frozen.printing.id,
        "face_id": case.frozen.face.id,
    }
    record["record_key"] = canonical(
        ["context_assignment", data["owner"], "name", None, 1]
    ).decode()
    return record


def _published_printing(db: DatabaseTemplate, case: Case) -> DatabaseTemplate:
    with db.copy() as connection:
        case.frozen.publish(connection)
        with connection.transaction():
            printing = dict(connection.rows("printing")[0].values)
            printing.update(
                id=case.frozen.printing.id,
                card_id=case.frozen.card.id,
                card_no=case.frozen.printing.card_no,
            )
            connection.insert("printing", printing)
            printed = dict(connection.rows("printing_face")[0].values)
            unit = next(
                row.values["name_unit_id"]
                for row in connection.rows("face_revision")
                if row.values["id"] == "link-revision"
            )
            printed.update(
                printing_id=case.frozen.printing.id,
                card_id=case.frozen.card.id,
                face_id=case.frozen.face.id,
                printed_name_unit_id=unit,
            )
            connection.insert("printing_face", printed)
        return type(db)(db.schema, connection._connection.serialize())


@pytest.mark.parametrize("state", ["verified", "unknown", "omitted"])
def test_historical_printing_proof_does_not_grant_current_printed_eligibility(
    case: Case, database: DatabaseTemplate, state: str
) -> None:
    record = _printing_assignment(case)
    replay, _, _ = case.replay(shards(record))
    assert dict(replay.assignment_languages)[str(record["record_key"])] == "ja"
    published = _published_printing(database, case)
    owner = NameOwner("printing_face", case.frozen.printing.id, case.frozen.face.id)
    with published.copy() as db:
        with db.transaction():
            values: dict[str, Value] = {"printed_text_state": state}
            if state != "verified":
                values["printed_name_unit_id"] = None
            db.update(
                "printing_face",
                {"printing_id": owner.identifier, "face_id": owner.face_id},
                values,
            )
        result = replay.resolve(db, owner)
        if state != "verified":
            assert result is None
        else:
            assert result is not None
            assert result.variant == "synthetic"
            assert result.record_hashes == (digest(canonical(record)),)


def test_assignment_never_follows_a_reparented_printing(
    case: Case, database: DatabaseTemplate
) -> None:
    record = _printing_assignment(case)
    replay, _, _ = case.replay(shards(record))
    published = _published_printing(database, case)
    owner = NameOwner("printing_face", case.frozen.printing.id, case.frozen.face.id)
    with published.copy() as db:
        with db.transaction():
            # The synthetic graph changes parent and its compound foreign keys together.
            db._connection.execute("PRAGMA defer_foreign_keys = ON")
            db.insert(
                "card",
                case.frozen.card.model_dump(mode="json") | {"id": "synthetic-parent"},
            )
            db.update(
                "face", {"id": case.frozen.face.id}, {"card_id": "synthetic-parent"}
            )
            db.update(
                "printing", {"id": owner.identifier}, {"card_id": "synthetic-parent"}
            )
            db.update(
                "printing_face",
                {"printing_id": owner.identifier, "face_id": owner.face_id},
                {"card_id": "synthetic-parent"},
            )
        result = replay.resolve(db, owner)
        assert result is not None
        assert (result.term_id, result.variant) == ("term:name.synthetic", "default")
        assert not result.record_hashes
        assert not result.decision_ids


def test_complete_reuses_observations_but_preserves_each_historical_basis_use(
    case: Case, monkeypatch: pytest.MonkeyPatch
) -> None:
    first = case.assignment()
    previous: dict[str, JsonValue] = {
        "record_key": first["record_key"],
        "record_hash": digest(canonical(first)),
        "decision_id": human([first])["default_decision_id"],
    }
    (case.frozen.root / "synthetic-history.txt").write_text("Later basis.\n")
    later = commit(case.frozen.root)
    second = case.assignment(number=2, previous=previous)
    object_value(object_value(second["data"])["identity_basis"])[
        "authored_revision"
    ] = later
    inputs, build = case.stage(shards(first, second))
    sources = Sources({"test-store": case.frozen.store}, inputs.repository, build)
    evidence = IdentityEvidence(sources, inputs.authored_revision)
    from sve_carddb.text_observations.archive import FrozenTexts  # ruff: ignore[import-outside-top-level] -- count actual physical extraction without replacing evidence validation

    calls: list[tuple[str, str, str]] = []
    original = FrozenTexts.version

    def count(
        provider: FrozenTexts, region: Region, number: str, version: str
    ) -> TextCard:
        calls.append((region, number, version))
        return original(provider, region, number, version)

    monkeypatch.setattr(FrozenTexts, "version", count)
    bases = [
        IdentityBasis.model_validate_json(
            canonical(object_value(record["data"])["identity_basis"])
        )
        for record in (first, second)
    ]
    batches = ((case.frozen.jp.store_id, case.frozen.jp.batch_id),)
    evidence.complete(bases[0], batches)
    first_calls = len(calls)
    assert first_calls > 0
    evidence.complete(bases[1], batches)
    assert len(calls) == first_calls
    assert {array(parse(use.locator.encode()))[0] for use in evidence.uses} == {
        case.basis["authored_revision"],
        later,
    }
    assert len(evidence.observations) == 1
    assert len(evidence.uses) == 2
    replay, _ = replay_names(
        inputs.load(), {"term:name.synthetic": "Synthetic card"}, inputs, sources
    )
    assert (
        len([use for use in replay.uses if use.usage == "name_identity_observation"])
        == 2
    )


@pytest.mark.parametrize("kind", ["assignment", "concept"])
def test_current_observation_update_does_not_replace_an_adoptions_own_basis(
    case: Case, kind: str
) -> None:
    from sve_carddb.extract import official_jp  # ruff: ignore[import-outside-top-level] -- compute the changed observation from synthetic raw, not a guessed hash
    from sve_carddb.extract.compare_jp import legacy_projection  # ruff: ignore[import-outside-top-level] -- historical identity uses the existing exact projection
    from sve_carddb.registry.review import observation  # ruff: ignore[import-outside-top-level] -- reproduce the registry's full observation recipe
    from sve_carddb.registry.storage import (  # ruff: ignore[import-outside-top-level] -- preserve old blobs while changing the current registry
        _area,
        load,
        read_yaml,
        relayout,
        write_files,
    )

    from .test_registry_preview_archive import RAW  # ruff: ignore[import-outside-top-level] -- exclusively synthetic physical input

    record = _printing_assignment(case) if kind == "assignment" else case.concept()
    raw = RAW.replace(b"Synthetic rule.", b"A different synthetic rule.")
    assert raw != RAW
    changed = observation(
        legacy_projection(official_jp.extract_card(raw, number="SYN-001")), "jp"
    )
    root = case.frozen.root / "authored"
    _, entries = load(root)
    for entry in entries.values():
        if entry.kind in {"printing", "art"}:
            assert entry.data["observation"] != changed
            entry.data["observation"] = changed
    reviews = {
        (_area(entry), entry.owner): ("gbaian10", "2026-10-03")
        for entry in entries.values()
    }
    write_files(relayout(root, list(entries.values()), reviews))
    consumer = commit(case.frozen.root)
    replay, inputs, build = case.replay(shards(record))
    assert inputs.authored_revision != case.basis["authored_revision"]
    assert consumer != case.basis["authored_revision"]
    assert replay.uses
    sources = Sources({"test-store": case.frozen.store}, inputs.repository, build)
    _, evidence = replay_names(
        inputs.load(), {"term:name.synthetic": "Synthetic card"}, inputs, sources
    )
    assert (
        evidence.record_revisions[str(record["record_key"])]
        == case.basis["authored_revision"]
    )
    current = IdentityBasis(
        authored_revision=consumer,
        registry_index_hash=digest(canonical(read_yaml(root / "ids/index.yaml"))),
        transition_index_hash=None,
    )
    with pytest.raises(
        ValueError,
        match=r"^Name identity complete frozen observation closure is absent$",
    ):
        evidence.complete(
            current, ((case.frozen.jp.store_id, case.frozen.jp.batch_id),)
        )


def test_superseded_assignment_keeps_its_own_bad_background_fatal(case: Case) -> None:
    first = case.assignment()
    object_value(object_value(first["data"])["identity_basis"])[
        "registry_index_hash"
    ] = "sha256:" + "0" * 64
    previous: dict[str, JsonValue] = {
        "record_key": first["record_key"],
        "record_hash": digest(canonical(first)),
        "decision_id": human([first])["default_decision_id"],
    }
    second = case.assignment(variant="default", number=2, previous=previous)
    with pytest.raises(
        ValueError, match=r"^Name identity registry index hash mismatch$"
    ):
        case.replay(shards(first, second))


def test_empty_entry_still_checks_that_the_adoption_base_exists(case: Case) -> None:
    _, inputs, _ = case.replay(shards())
    with pytest.raises(
        ValueError, match=r"^Name identity Git ancestry is unavailable$"
    ):
        verify_name_adoption_base(inputs, "0" * 40)


def test_observation_cache_keeps_the_exact_batch_membership(mixed: Mixed) -> None:
    from sve_carddb.registry.inputs import JSON_VALUE  # ruff: ignore[import-outside-top-level] -- pin the old single-region registry from its own immutable bytes
    from sve_carddb.registry.yaml_reader import parse_yaml  # ruff: ignore[import-outside-top-level] -- no current disk registry substitution

    case = mixed.case
    sources = case.frozen.sources()
    old_revision = case.frozen.authored
    old_index = sources.repository.read(old_revision, "authored/ids/index.yaml")
    old_basis = IdentityBasis(
        authored_revision=old_revision,
        registry_index_hash=digest(
            canonical(JSON_VALUE.validate_python(parse_yaml(old_index), strict=True))
        ),
        transition_index_hash=None,
    )
    evidence = IdentityEvidence(sources, str(case.basis["authored_revision"]))
    from sve_carddb.digital_links.models import Record  # ruff: ignore[import-outside-top-level] -- the fixture's unchanged adoption retains the old batch pin

    record = Record.model_validate_json(case.frozen.record)
    assert record.data.value is not None
    old_batch = record.data.value.sve_names[0].name_ref.batch_id
    evidence.complete(old_basis, ((case.frozen.jp.store_id, old_batch),))
    before = tuple(evidence.uses)
    evidence.complete(old_basis, ((case.frozen.jp.store_id, case.frozen.jp.batch_id),))
    later = tuple(evidence.uses[len(before) :])
    assert len(evidence.observations) == 2
    assert before
    assert later
    assert {use.source.archive.batch_id for use in before} == {old_batch}
    assert {use.source.archive.batch_id for use in later} == {case.frozen.jp.batch_id}

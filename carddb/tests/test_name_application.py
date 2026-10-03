"""Policy applications bind their own verified owner, raw target and complete F1 uses."""

from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.build_db import Json
from sve_carddb.digital_links.evidence import Evidence
from sve_carddb.digital_links.importer import review_context
from sve_carddb.digital_name_policies.application import (
    _policy_candidate,
    populate,
    prepare,
)
from sve_carddb.digital_name_policies.evaluate import catalogue, historical_sources
from sve_carddb.digital_name_policies.owners import publication_owners
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.translations.importer import Inputs, import_glossary
from sve_carddb.translations.name_build import NameOwner, name_context, name_source
from sve_carddb.translations.name_materialization import materialize_name
from sve_carddb.translations.name_selection import NameCandidate
from sve_carddb.translations.sources import Sources

from .name_replay_fixtures import human, name_term
from .test_glossary_adoption import delegated, receipt
from .translation_fixtures import choice, envelope

if TYPE_CHECKING:
    from pathlib import Path

from .name_application_fixtures import (
    ApplicationCase,
    application_case,
    printed_owner,
    staged,
)


@pytest.fixture(scope="module")
def case(tmp_path_factory: pytest.TempPathFactory) -> ApplicationCase:
    return application_case(tmp_path_factory.mktemp("name-application"))


@pytest.fixture(scope="module")
def missing_translation_case(
    tmp_path_factory: pytest.TempPathFactory,
) -> ApplicationCase:
    return application_case(
        tmp_path_factory.mktemp("name-application-missing"), translated_name=""
    )


@pytest.fixture(scope="module")
def equal_language_case(tmp_path_factory: pytest.TempPathFactory) -> ApplicationCase:
    return application_case(
        tmp_path_factory.mktemp("name-application-equal"),
        translated_name="Synthetic card",
    )


@pytest.mark.parametrize("eligible", [False, True])
def test_ambiguous_concept_keeps_independent_owner_policy(
    case: ApplicationCase,
    missing_translation_case: ApplicationCase,
    tmp_path: Path,
    eligible: bool,
) -> None:
    changed = staged(
        case if eligible else missing_translation_case,
        tmp_path / "ambiguous",
        {
            "translations/glossary/concepts/001.yaml": envelope(
                [name_term(), name_term("name.other")]
            ),
        },
    )
    with changed.database.copy() as db:
        import_glossary(
            db,
            Inputs(
                changed.inputs.root,
                changed.inputs.repository,
                changed.inputs.authored_revision,
            ),
            build=changed.context,
            stores={"test-store": changed.fixture.digital.store},
        )
        with db.transaction():
            result = populate(
                db,
                changed.inputs,
                changed.texts,
                sources=changed.sources(),
                replay=changed.replay,
            )
            owner = object_value(array(result.report["owners"])[0])
            assert owner["semantic_reason"] == "ambiguous_name_concept"
            assert result.report["warning_owners"] == 1
            assert result.report["policy_covered_owners"] == int(eligible)
            if eligible:
                assert db.rows("translation")[0].values["text"] == "合成測試名"
                assert db.rows("translation")[0].values["origin"] == "official_sv1"
                assert len(result.bindings) == 1
            else:
                assert owner["selection"] == "untranslated"
                assert not db.rows("translation")
                assert not result.bindings


def test_equal_language_policy_name_uses_frozen_target_proof(
    equal_language_case: ApplicationCase,
) -> None:
    changed = equal_language_case
    sources = changed.sources()
    with changed.database.copy() as db:
        plan = prepare(
            db, changed.inputs, changed.texts, sources=sources, replay=changed.replay
        )
        candidate = plan.owners[0].selection.candidate
        assert candidate is not None
        proofs = [
            sources.text(ref)
            for ref in plan.owners[0].result.refs
            if ref.parser == "translation-sv1-v1"
            and ref.text_hash == digest(b"Synthetic card")
        ]
        by_language = {proof[0]: proof[2] for proof in proofs}
        assert set(by_language) == {"ja", "zh-Hant"}
        assert candidate.source == by_language["zh-Hant"]
        assert candidate.source != by_language["ja"]
        loaded = changed.inputs.load().effective("names")
        historical = historical_sources(
            loaded, sources.stores, changed.inputs.repository
        )
        frozen = catalogue(loaded, historical)
        result = plan.owners[0].result
        refs = tuple(
            sorted(result.refs, key=lambda ref: historical.text(ref)[0] != "ja")
        )
        assert historical.text(refs[0])[0] == "ja"
        reordered = _policy_candidate(replace(result, refs=refs), frozen, historical)
        assert reordered is not None
        assert reordered.source == by_language["zh-Hant"]


def test_policy_application_is_not_human_confirmation_or_a_link(
    case: ApplicationCase,
) -> None:
    with case.database.copy() as db, db.transaction():
        original_links = tuple(db.rows("digital_link"))
        expected = prepare(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        result = populate(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        result.record.verify(db, case.context, expected.uses)
        assert len(result.bindings) == 1
        assert result.bindings[0].basis == "own_source"
        assert any(
            use.usage == "translation_evidence"
            and use.source.parser_version == "translation-jp-v1"
            for use in expected.uses
        )
        assert result.report["policy_covered_owners"] == 1
        assert result.report["human_checked_policy_members"] == 0
        decision = next(
            r.values
            for r in db.rows("decision")
            if r.values["category"] == "digital_name_policy"
        )
        assert decision["sample_ids"] == Json([])
        assert decision["reviewed_by"] == "gbaian10"
        assert decision["authored_by"] == "owner-name-v1"
        assert decision["authored_at"] != decision["reviewed_at"]
        assert decision["membership_hash"] == digest(
            canonical([expected.owners[0].source.owner.payload()])
        )
        assert db.rows("translation")[0].values["origin"] == "official_sv1"
        assert not db.rows("translation_selection")
        assert tuple(db.rows("digital_link")) == original_links
        assert all(
            "text" not in object_value(row) for row in array(result.report["owners"])
        )


def test_record_requires_every_owner_and_catalogue_use(case: ApplicationCase) -> None:
    with case.database.copy() as db, db.transaction():
        expected = prepare(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        result = populate(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        uses = tuple(
            use
            for use in result.record.uses
            if use.usage != "digital_policy_owner_name"
        )
        changed = result.record.model_copy(update={"uses": uses})
        with pytest.raises(
            ValueError, match=r"^Build input use closure or context mismatch$"
        ):
            changed.verify(db, case.context, expected.uses)


@pytest.mark.parametrize("fault", ["owner", "name", "parent"])
def test_database_owner_must_match_verified_publication(
    case: ApplicationCase, fault: str
) -> None:
    with case.database.copy() as db, pytest.raises(RollbackError), db.transaction():  # ruff: ignore[pytest-raises-with-multiple-statements] -- intentionally corrupt rows must roll back before graph checks
        row = db.rows("face_revision")[0].values
        if fault == "owner":
            db.update("face_revision", {"id": row["id"]}, {"id": "invented"})
            message = "Name owner is absent from verified publication candidates"
        elif fault == "name":
            db.update(
                "face_revision", {"id": row["id"]}, {"name_unit_id": "family-name"}
            )
            message = "Name build source exact hash mismatch"
        else:
            db.update("face_revision", {"id": row["id"]}, {"face_id": "front"})
            message = "Name owner differs from its verified publication source"
        with pytest.raises(ValueError, match="^" + message + "$"):
            publication_owners(db, case.texts)
        raise RollbackError


class RollbackError(Exception):
    """A deliberately corrupt graph must not run commit-time validators."""


def test_no_application_with_unpinned_or_missing_runtime(case: ApplicationCase) -> None:
    config = object_value(parse(case.context.configuration.encode()))
    object_value(config["digital_name_application"])["baseline"] = (
        "pretend previous run"
    )
    changed = type(case.context).from_inputs(
        case.context.program_revision,
        {
            p.name: (case.fixture.root / p.name).read_bytes()
            for p in case.context.dependencies
        },
        config,
    )
    with case.database.copy() as db:
        with pytest.raises(
            ValueError,
            match=r"^Build configuration does not pin complete name policy inputs$",
        ):
            prepare(
                db,
                case.inputs,
                case.texts,
                sources=Sources(
                    {"test-store": case.fixture.digital.store},
                    case.fixture.root,
                    changed,
                ),
                replay=case.replay,
            )


def test_direct_name_ids_ignore_receipt_time_owner_and_later_concept(
    case: ApplicationCase,
) -> None:
    with case.database.copy() as db, db.transaction():
        result = populate(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        owner = NameOwner(
            "face_revision", str(db.rows("face_revision")[0].values["id"])
        )
        source = name_source(db, owner)
        assert source is not None
        translated = db.rows("translation")[0].values
        raw = next(
            use.source
            for use in result.record.uses
            if use.source.id == translated["source_id"]
        )
        candidate = NameCandidate(
            str(translated["text"]),
            str(translated["origin"]),
            "digital_official",
            "decision",
            "2026-10-03T00:00:01Z",
            raw,
            choice_hash=digest(b"later unrelated concept"),
        )
        context = name_context(db, source)
        assert (
            materialize_name(
                db,
                replace(
                    source, owner=NameOwner("face_revision", "another verified owner")
                ),
                context,
                candidate,
            )
            == translated["id"]
        )
        assert len(db.rows("translation")) == 1


@pytest.mark.parametrize("at", ["", "2026-10-03", "2026-10-03T00:00:00+08:00"])
def test_application_time_is_rejected_before_loading_any_input(
    case: ApplicationCase, at: str
) -> None:
    with pytest.raises(
        ValueError, match=r"^Name application requires an explicit UTC instant$"
    ):
        replace(case.inputs, application_at=at)


@pytest.mark.parametrize("state", ["verified", "unknown", "omitted"])
def test_printing_state_is_rechecked_without_borrowing_current(
    case: ApplicationCase, state: str
) -> None:
    with case.database.copy() as db, db.transaction():
        owner = printed_owner(db, case)
        if state != "verified":
            db.update(
                "printing_face",
                {"printing_id": owner.identifier, "face_id": owner.face_id},
                {"printed_text_state": state, "printed_name_unit_id": None},
            )
        result = populate(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        expected = prepare(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        result.record.verify(db, case.context, expected.uses)
        assert len(result.bindings) == (2 if state == "verified" else 1)
        assert len(db.rows("translation")) == 1
        assert result.report["unknown_owners"] == (
            [] if state == "verified" else [owner.payload()]
        )
        if state == "verified":
            assert len({b.translation_id for b in result.bindings}) == 1
            assert (
                len(
                    [
                        use
                        for use in result.record.uses
                        if use.usage == "digital_policy_owner_name"
                    ]
                )
                == 2
            )


def test_printed_name_must_match_its_own_frozen_source(case: ApplicationCase) -> None:
    with case.database.copy() as db, pytest.raises(RollbackError), db.transaction():  # ruff: ignore[pytest-raises-with-multiple-statements] -- deliberately corrupt owner rows roll back before graph checks
        owner = printed_owner(db, case)
        db.insert(
            "text_unit",
            {
                "id": "older-name",
                "lang": "ja",
                "text": "Synthetic old name",
                "content_hash": digest(b"Synthetic old name"),
            },
        )
        db.update(
            "printing_face",
            {"printing_id": owner.identifier, "face_id": owner.face_id},
            {"printed_name_unit_id": "older-name"},
        )
        with pytest.raises(
            ValueError,
            match=r"^Name owner differs from its verified publication source$",
        ):
            populate(
                db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
            )
        raise RollbackError


@pytest.mark.parametrize("fault", ["decision", "date", "source", "collision"])
def test_materialization_guards_one_exact_message(
    case: ApplicationCase, fault: str
) -> None:
    with case.database.copy() as db, db.transaction():
        plan = prepare(
            db, case.inputs, case.texts, sources=case.sources(), replay=case.replay
        )
        candidate = plan.owners[0].selection.candidate
        assert candidate is not None
        candidate = replace(candidate, decision_id="decision")
        source = plan.owners[0].source
        context = name_context(db, source)
        if fault == "collision":
            identifier = materialize_name(db, source, context, candidate)
            db.update(
                "translation",
                {"id": identifier},
                {"text": "Synthetic corrupted translation"},
            )
            message = "Stable name translation ID collision"
        elif fault == "source":
            candidate = replace(candidate, source=None)
            message = "Selected official name lacks frozen source evidence"
        else:
            candidate = replace(
                candidate,
                **(
                    {"decision_id": None}
                    if fault == "decision"
                    else {"reviewed_at": ""}
                ),
            )
            message = "Selected name lacks an adopted decision and review date"
        with pytest.raises(ValueError, match="^" + message + "$"):
            materialize_name(db, source, context, candidate)


def test_unofficial_id_does_not_depend_on_later_adoption_metadata(
    case: ApplicationCase,
) -> None:
    with case.database.copy() as db, db.transaction():
        source = name_source(
            db,
            NameOwner("face_revision", str(db.rows("face_revision")[0].values["id"])),
        )
        assert source is not None
        context = name_context(db, source)
        candidate = NameCandidate(
            "合成自譯",
            "machine",
            "unofficial",
            "decision",
            "2026-10-03T00:00:00Z",
            None,
            choice_hash=digest(b"first choice"),
        )
        identifier = materialize_name(db, source, context, candidate)
        assert (
            materialize_name(
                db,
                source,
                context,
                replace(
                    candidate,
                    choice_hash=digest(b"later concept"),
                    reviewed_at="2026-10-04T00:00:00Z",
                ),
            )
            == identifier
        )


@pytest.mark.parametrize("sample_member", [False, True])
def test_priority_uses_actual_human_sample_membership(
    case: ApplicationCase, tmp_path: Path, sample_member: bool
) -> None:

    selected = choice("name.synthetic", value="人工親選名")
    spare = choice("name.unrelated", value="其他合成名")
    choices = human([selected, spare])
    decision = object_value(array(choices["decisions"])[0])
    decision["state"] = "sampled"
    decision["sample_ids"] = [
        selected["record_key"] if sample_member else spare["record_key"]
    ]
    changed = staged(
        case,
        tmp_path / "choice",
        {
            "translations/glossary/concepts/001.yaml": envelope(
                [name_term(), name_term("name.unrelated", "Another synthetic name")]
            ),
            "translations/glossary/choices/001.yaml": choices,
        },
    )
    with changed.database.copy() as db:
        import_glossary(
            db,
            Inputs(
                changed.inputs.root,
                changed.inputs.repository,
                changed.inputs.authored_revision,
            ),
            build=changed.context,
            stores={"test-store": changed.fixture.digital.store},
        )
        with db.transaction():
            result = populate(
                db,
                changed.inputs,
                changed.texts,
                sources=changed.sources(),
                replay=changed.replay,
            )
            assert result.report["policy_covered_owners"] == (0 if sample_member else 1)
            translation = db.rows("translation")[0].values
            assert translation["origin"] == (
                "project" if sample_member else "official_sv1"
            )
            assert translation["text"] == (
                "人工親選名" if sample_member else "合成測試名"
            )
            expected = prepare(
                db,
                changed.inputs,
                changed.texts,
                sources=changed.sources(),
                replay=changed.replay,
            )
            result.record.verify(db, changed.context, expected.uses)


@pytest.mark.parametrize("fault", ["reviewer", "mode"])
def test_choice_cannot_claim_human_priority_with_only_one_qualification(
    case: ApplicationCase, tmp_path: Path, fault: str
) -> None:
    selected = choice("name.synthetic", value="其他合法選詞")
    if fault == "reviewer":
        choices = human([selected])
        object_value(array(choices["decisions"])[0])["reviewed_by"] = "Other reviewer"
    else:
        choices = delegated([selected])
        receipt(selected)["decided_by"] = "gbaian10"
        note = object_value(array(choices["decisions"])[0])["note"]
        choices = envelope([selected])
        object_value(array(choices["decisions"])[0])["note"] = note
        object_value(array(choices["decisions"])[0])["reviewed_by"] = "gbaian10"
    decision = object_value(array(choices["decisions"])[0])
    assert selected["record_key"] in array(decision["sample_ids"])
    changed = staged(
        case,
        tmp_path / "choice-qualification",
        {
            "translations/glossary/concepts/001.yaml": envelope([name_term()]),
            "translations/glossary/choices/001.yaml": choices,
        },
    )
    with changed.database.copy() as db:
        import_glossary(
            db,
            Inputs(
                changed.inputs.root,
                changed.inputs.repository,
                changed.inputs.authored_revision,
            ),
            build=changed.context,
            stores={"test-store": changed.fixture.digital.store},
        )
        with db.transaction():
            result = populate(
                db,
                changed.inputs,
                changed.texts,
                sources=changed.sources(),
                replay=changed.replay,
            )
            assert result.report["policy_covered_owners"] == 1
            assert db.rows("translation")[0].values["text"] == "合成測試名"
            assert db.rows("translation")[0].values["origin"] == "official_sv1"


def test_provisional_publication_owner_cannot_acquire_name(
    case: ApplicationCase,
) -> None:
    with case.database.copy() as db, db.transaction():
        db.update(
            "card",
            {"id": case.fixture.digital.card.id},
            {"identity_state": "provisional"},
        )
        with pytest.raises(
            ValueError, match=r"^Name publication owner is not confirmed$"
        ):
            publication_owners(db, case.texts)


@pytest.fixture(scope="module")
def adopted_choice_case(
    case: ApplicationCase, tmp_path_factory: pytest.TempPathFactory
) -> ApplicationCase:
    return staged(
        case,
        tmp_path_factory.mktemp("name-adopted-choice") / "case",
        {
            "translations/glossary/concepts/001.yaml": envelope([name_term()]),
            "translations/glossary/choices/001.yaml": human(
                [choice("name.synthetic", value="人工親選名")]
            ),
        },
    )


@pytest.mark.parametrize("fault", ["replay", "imported"])
def test_imported_choice_cannot_substitute_authored_evidence(
    adopted_choice_case: ApplicationCase, fault: str
) -> None:
    changed = adopted_choice_case
    with changed.database.copy() as db:
        import_glossary(
            db,
            Inputs(
                changed.inputs.root,
                changed.inputs.repository,
                changed.inputs.authored_revision,
            ),
            build=changed.context,
            stores={"test-store": changed.fixture.digital.store},
        )
        with pytest.raises(RollbackError), db.transaction():  # ruff: ignore[pytest-raises-with-multiple-statements] -- intentionally corrupt evidence must roll back before validators
            term = "term:name.synthetic"
            db.update(
                "glossary_translation",
                {"term_id": term, "lang": "zh-Hant"},
                {"text": "變造選詞"},
            )
            replay = changed.replay
            if fault == "replay":
                replay = replace(replay, snapshot=replace(replay.snapshot, shards=()))
                message = "Adopted name choice differs from complete glossary replay"
            else:
                message = "Imported name choice differs from checked authored evidence"
            with pytest.raises(ValueError, match="^" + message + "$"):
                prepare(
                    db,
                    changed.inputs,
                    changed.texts,
                    sources=changed.sources(),
                    replay=replay,
                )
            raise RollbackError


def test_registry_cache_uses_the_complete_immutable_context(
    case: ApplicationCase,
) -> None:
    sources = case.sources()
    review = review_context(sources)
    evidence = Evidence(sources)
    first = evidence.index(review)
    key = sources.context_key(review.context)
    assert key == canonical(review.context.model_dump(mode="json"))
    assert sources.context_key(review.context) is key
    assert evidence.index(review) is first
    changed = object_value(parse(review.context.configuration.encode()))
    object_value(changed["catalog_registry"])["index_hash"] = digest(b"wrong registry")
    context = review.context.model_copy(
        update={"configuration": canonical(changed).decode()}
    )
    assert sources.context_key(context) != key
    with pytest.raises(ValueError, match=r"^Historical registry index hash mismatch$"):
        evidence.index(review.model_copy(update={"context": context}))

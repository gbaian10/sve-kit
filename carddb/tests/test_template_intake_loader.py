"""Exact batch, source and final-byte guards use only synthetic closed histories."""

import copy
import re
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_translations.loader import _frequency, load_templates
from sve_carddb.template_translations.models import DefinitionRecord

from .template_intake_fixtures import (
    DEFINITIONS,
    INVENTORY,
    TRANSLATIONS,
    definition,
    intake_case,
    policy_git,
    recognition_term_source,
    shard,
    translation,
    write,
)

if TYPE_CHECKING:
    from pathlib import Path

    from pydantic import JsonValue
    from pytest_mock import MockerFixture

    from .template_intake_fixtures import Case

__all__ = ("intake_case", "policy_git", "recognition_term_source")


def exact(message: str) -> str:
    """A broader alternate message can conceal a different earlier refusal."""
    return "^" + re.escape(message) + "$"


def test_adopted_definition_translation_and_terminal_revisions(
    intake_case: Case, tmp_path: Path
) -> None:
    case = intake_case
    snapshot = case.verified
    assert len(snapshot.records()) == 4
    assert len(snapshot.effective_translations()) == 2
    assert len(snapshot.frequencies) == 2
    assert all(n == 1 for _, n in snapshot.frequencies)
    assert snapshot.pins()["index_hash"] == digest(canonical(parse(snapshot.index)))
    assert snapshot.pins()["index_exact_hash"] == digest(snapshot.index)
    root = case.fork(tmp_path / "later", published=True)
    second = translation(
        case.definitions[0], revision=2, text="Revised {{slot_0}} translation"
    )
    # The chosen first definition is a numeric one; do not use a different schema's text.
    fields = array(
        object_value(object_value(case.definitions[0]["data"])["parameter_schema"])[
            "slots"
        ]
    )
    object_value(second["data"])["text"] = "Revised " + " ".join(
        "{{" + str(object_value(s)["name"]) + "}}" for s in fields
    )
    data = object_value(second["data"])
    object_value(data["model_review"])["text_hash"] = digest(str(data["text"]).encode())
    revision = write(
        root, {"translations/templates/translations/002.yaml": shard([second])}
    )
    result = load_templates(PinnedRepository(root), revision, case.sources())
    assert len(result.records()) == 5
    assert any(r.data.revision == 2 for r in result.effective_translations())
    detached = result.envelopes()[0]
    if isinstance(detached.records[0], DefinitionRecord):
        detached.records[0].data.parameter_schema.model_copy(update={"slots": ()})
    assert len(result.records()) == 5


@pytest.mark.parametrize(
    ("field", "value", "message"),
    [
        (
            "content_hash",
            "sha256:" + "a" * 64,
            "Template definition content hash differs from its six-field payload",
        ),
        (
            "inventory_id",
            "inv:missing",
            "Template definition references an absent inventory entry",
        ),
        (
            "normalizer_version",
            "unregistered-v1",
            "Template definition language normalizer or semantic variant is unsupported",
        ),
        (
            "source_lang",
            "en",
            "Template definition language normalizer or semantic variant is unsupported",
        ),
        (
            "semantic_variant",
            "unadopted-variant",
            "Template definition language normalizer or semantic variant is unsupported",
        ),
        (
            "supersedes_id",
            "T" + "a" * 16,
            "Template supersedes requires an adopted parent or its verified legacy family",
        ),
    ],
)
def test_definition_payload_refusals(
    intake_case: Case, tmp_path: Path, field: str, value: JsonValue, message: str
) -> None:
    root = intake_case.fork(tmp_path / "bad")
    files = copy.deepcopy(intake_case.files)
    records = [
        object_value(r) for r in array(object_value(files[DEFINITIONS])["records"])
    ]
    object_value(records[0]["data"])[field] = value
    files[DEFINITIONS] = shard(records)
    revision = write(root, files)
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("missing", "Formal template inventory must cover every frozen batch entry"),
        ("extra", "Formal template inventory must cover every frozen batch entry"),
        ("duplicate", "Invalid formal template inventory"),
        ("normalizer", "Formal template entry differs from its exact frozen replay"),
        ("span_hash", "Formal template entry differs from its exact frozen replay"),
        ("empty", "Formal template inventory shards cannot be empty"),
        ("boolean_format", "Invalid formal template inventory"),
    ],
)
def test_inventory_is_full_exact_and_closed(
    intake_case: Case, tmp_path: Path, change: str, message: str
) -> None:
    root = intake_case.fork(tmp_path / "bad")
    files = copy.deepcopy(intake_case.files)
    inv = object_value(files[INVENTORY])
    entries = array(inv["entries"])
    if change == "missing":
        entries.pop()
    elif change == "extra":
        added = copy.deepcopy(object_value(entries[-1]))
        added["id"] = "inv:" + "f" * 64
        entries.append(added)
        entries.sort(key=lambda e: str(object_value(e)["id"]))
    elif change == "duplicate":
        entries.insert(0, copy.deepcopy(entries[0]))
    elif change == "normalizer":
        object_value(entries[0])["normalizer_id"] = "wrong-v1"
    elif change == "span_hash":
        object_value(entries[0])["normalized_hash"] = "sha256:" + "c" * 64
    elif change == "empty":
        entries.clear()
    else:
        inv["template_source_format"] = True
    revision = write(root, files)
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("membership", "Template decision exact membership mismatch"),
        (
            "empty_samples",
            "Template decision requires actual sampled or checked members",
        ),
        (
            "wrong_samples",
            "Template decision requires actual sampled or checked members",
        ),
        ("partial_confirmed", "Confirmed template decision must check every member"),
        ("category", "Template record kind key or filing differs from its batch"),
        ("wrong_key", "Template record kind key or filing differs from its batch"),
        ("wrong_filing", "Template record kind key or filing differs from its batch"),
        ("unknown_field", "Invalid adopted template shard"),
    ],
)
def test_human_batch_guards(
    intake_case: Case, tmp_path: Path, change: str, message: str
) -> None:
    root = intake_case.fork(tmp_path / "bad")
    files = copy.deepcopy(intake_case.files)
    batch = object_value(files[DEFINITIONS])
    decision = object_value(array(batch["decisions"])[0])
    records = [object_value(r) for r in array(batch["records"])]
    if change == "membership":
        decision["membership_hash"] = "sha256:" + "d" * 64
    elif change == "empty_samples":
        decision["sample_ids"] = []
    elif change == "wrong_samples":
        decision["sample_ids"] = ["absent"]
    elif change == "partial_confirmed":
        decision.update(state="confirmed", sample_ids=[records[0]["record_key"]])
    elif change == "category":
        decision["category"] = "template_translation"
    elif change in {"wrong_key", "wrong_filing"}:
        records[0]["record_key" if change == "wrong_key" else "filing_key"] = "wrong"
        files[DEFINITIONS] = shard(records)
    else:
        object_value(records[0]["data"])["normalized_text"] = (
            "Synthetic literal forbidden in authored"
        )
        files[DEFINITIONS] = shard(records)
    revision = write(root, files)
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("hash", "Template model review must pin the final exact text hash"),
        ("same_model", "Invalid adopted template shard"),
        ("missing_review", "Invalid adopted template shard"),
        ("fake_project", "Invalid adopted template shard"),
        (
            "unresolved_dispute",
            "Disputed template translation requires human resolution and an actual sample",
        ),
        (
            "different_resolution",
            "Template dispute resolution differs from its human sample event",
        ),
        ("unknown_slot", "Template parameter is not declared by its schema"),
        ("missing_slot", "Template text must use every declared parameter"),
        ("missing_definition", "Template translation requires an adopted definition"),
        ("revision_gap", "Template translation revision chain has a gap or fork"),
    ],
)
def test_final_translation_rejections(
    intake_case: Case, tmp_path: Path, change: str, message: str
) -> None:
    root = intake_case.fork(tmp_path / "bad")
    files = copy.deepcopy(intake_case.files)
    records = [
        object_value(r) for r in array(object_value(files[TRANSLATIONS])["records"])
    ]
    data = object_value(records[0]["data"])
    model = object_value(data["model_review"])
    if change == "hash":
        data["text"] = str(data["text"]) + " change"
    elif change == "same_model":
        model["reviewed_by"] = model["translated_by"]
    elif change == "missing_review":
        data["model_review"] = None
    elif change == "fake_project":
        data["origin"] = "project"
    elif "dispute" in change or change == "different_resolution":
        model["result"] = "disputed"
        if change == "different_resolution":
            model["resolution"] = {
                "reviewed_by": "Other human",
                "reviewed_at": "2026-10-02T00:00:00Z",
                "note": "Synthetic different event",
            }
    elif change == "missing_definition":
        data["template_id"] = "T" + "a" * 10
        records[0]["record_key"] = canonical(
            [
                "template_translation",
                data["template_id"],
                data["lang"],
                data["revision"],
            ]
        ).decode()
    elif change == "revision_gap":
        data["revision"] = 2
        records[0]["record_key"] = canonical(
            ["template_translation", data["template_id"], data["lang"], 2]
        ).decode()
    else:
        data["text"] = (
            "{{absent}}" if change == "unknown_slot" else "Synthetic plain translation"
        )
        model["text_hash"] = digest(str(data["text"]).encode())
    files[TRANSLATIONS] = shard(records)
    revision = write(root, files)
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


def test_unknown_card_reference_stays_unadoptable(intake_case: Case) -> None:
    member = next(
        m
        for m in intake_case.replay.entries
        if "missing_card_name_concept" in m.pending
    )
    with pytest.raises(
        ValueError,
        match=exact("Template definition source has unresolved parameter roles"),
    ):
        definition(member)


def test_source_coverage_is_independent_of_a_valid_inventory(
    intake_case: Case, tmp_path: Path, mocker: MockerFixture
) -> None:
    mocker.patch(
        "sve_carddb.template_translations.sources.coverage",
        return_value={"complete": False, "synthetic_unknown_fields": 1},
    )
    root = intake_case.fork(tmp_path / "partial")
    revision = write(root, copy.deepcopy(intake_case.files))
    from sve_carddb.template_translations.sources import TemplateSources  # ruff: ignore[import-outside-top-level] -- request a fresh source replay for this independent coverage proof

    sources = TemplateSources(
        PinnedRepository(root),
        {"test-store": intake_case.source.store},
        main_revision=intake_case.source.main,
        legacy_bytes=intake_case.source.legacy,
        proposals=intake_case.source.proposals,
    )
    snapshot = load_templates(PinnedRepository(root), revision, sources)
    assert object_value(parse(snapshot.source_reports[0][1]))["complete"] is False
    assert len(snapshot.records()) == 4


def test_dispute_requires_its_own_sample_and_matching_event(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "disputed")
    files = copy.deepcopy(intake_case.files)
    records = [
        object_value(r) for r in array(object_value(files[TRANSLATIONS])["records"])
    ]
    data = object_value(records[0]["data"])
    model = object_value(data["model_review"])
    model.update(
        result="disputed",
        resolution={
            "reviewed_by": "Synthetic human",
            "reviewed_at": "2026-10-02T00:00:00Z",
            "note": "Synthetic resolved event",
        },
    )
    files[TRANSLATIONS] = shard(records)
    revision = write(root, files)
    snapshot = load_templates(PinnedRepository(root), revision, intake_case.sources())
    assert any(
        r.data.model_review and r.data.model_review.result == "disputed"
        for r in snapshot.effective_translations()
    )
    # A second prospective batch samples a different member only.
    other = intake_case.fork(tmp_path / "unsampled")
    batch = object_value(files[TRANSLATIONS])
    decision = object_value(array(batch["decisions"])[0])
    decision["sample_ids"] = [records[1]["record_key"]]
    revision = write(other, files)
    with pytest.raises(
        ValueError,
        match=exact(
            "Disputed template translation requires human resolution and an actual sample"
        ),
    ):
        load_templates(PinnedRepository(other), revision, intake_case.sources())


def test_duplicate_records_across_shards_are_rejected(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "duplicate")
    files = copy.deepcopy(intake_case.files)
    files["translations/templates/definitions/002.yaml"] = files[DEFINITIONS]
    revision = write(root, files)
    with pytest.raises(
        ValueError, match=exact("Duplicate immutable adopted template record")
    ):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


@pytest.mark.parametrize(
    ("change", "message"),
    [
        ("span", "Template definition span must match its exact inventory part"),
        (
            "id",
            "Template ID differs from its legacy fingerprint or allocated payload hash",
        ),
        ("cycle", "Template supersedes chain must not contain a cycle"),
    ],
)
def test_definition_span_id_and_parent_are_separate_guards(
    intake_case: Case, tmp_path: Path, change: str, message: str
) -> None:
    root = intake_case.fork(tmp_path / "bad-definition")
    files = copy.deepcopy(intake_case.files)
    records = [
        object_value(r) for r in array(object_value(files[DEFINITIONS])["records"])
    ]
    data = object_value(records[0]["data"])
    if change == "span":
        segment = object_value(array(object_value(data["source_span"])["segments"])[0])
        assert isinstance(segment["end"], int)
        segment["end"] += 1
    elif change == "id":
        data["id"] = "T" + "f" * 10
        records[0]["record_key"] = canonical(["sentence_template", data["id"]]).decode()
    else:
        data["supersedes_id"] = data["id"]
    files[DEFINITIONS] = shard(records)
    revision = write(root, files)
    with pytest.raises(ValueError, match=exact(message)):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


@pytest.mark.parametrize("change", ["pending", "role"])
def test_old_id_requires_every_family_member_to_agree(
    intake_case: Case, change: str
) -> None:
    member = next(
        m
        for m in intake_case.replay.entries
        if m.entry.role == "body" and not m.pending
    )
    record = DefinitionRecord.model_validate_json(canonical(definition(member)))
    if change == "pending":
        second = replace(member, pending=("Synthetic unresolved role",))
        message = "Legacy template members require one fully resolved schema"
    else:
        second = replace(member, roles=("numeric",))
        assert second.roles != member.roles
        message = "Legacy template members disagree on slot semantic roles"
    with pytest.raises(ValueError, match=exact(message)):
        _frequency(member, record, {"first": member, "second": second}, old=True)
    assert (
        _frequency(member, record, {"first": member, "second": second}, old=False) == 1
    )


def test_adoption_policy_format_does_not_enable_an_unimplemented_loader(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "policy")
    files = copy.deepcopy(intake_case.files)
    records = [
        object_value(r) for r in array(object_value(files[TRANSLATIONS])["records"])
    ]
    object_value(records[0]["data"])["adoption_review"] = {
        "mode": "approved_policy",
        "policy": {
            "policy_id": "synthetic-policy",
            "authored_revision": intake_case.prior,
            "path": "authored/template-policies/synthetic.yaml",
            "hash": "sha256:" + "a" * 64,
            "approval_receipt_hash": "sha256:" + "b" * 64,
        },
        "initial_sample_decisions": [
            {"decision_id": "d:" + "c" * 64, "membership_hash": "sha256:" + "c" * 64}
        ],
    }
    files[TRANSLATIONS] = shard(records)
    revision = write(root, files)
    with pytest.raises(
        ValueError,
        match=exact(
            "Template policy adoption requires the complete translation-policy loader"
        ),
    ):
        load_templates(PinnedRepository(root), revision, intake_case.sources())


def test_project_origin_and_verified_source_evidence(
    intake_case: Case, tmp_path: Path
) -> None:
    root = intake_case.fork(tmp_path / "project")
    files = copy.deepcopy(intake_case.files)
    records = [
        object_value(r) for r in array(object_value(files[TRANSLATIONS])["records"])
    ]
    object_value(records[0]["data"]).update(origin="project", model_review=None)
    records[0]["evidence"] = [
        {
            "role": "synthetic example",
            "source_ref": intake_case.replay.entries[0].entry.source_ref.model_dump(
                mode="json"
            ),
        }
    ]
    files[TRANSLATIONS] = shard(records)
    revision = write(root, files)
    result = load_templates(PinnedRepository(root), revision, intake_case.sources())
    assert any(r.data.origin == "project" for r in result.effective_translations())

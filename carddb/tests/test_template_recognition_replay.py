"""Whole-source resolution preserves raw spellings and independent pending causes."""

import copy
import re
import shutil
from dataclasses import replace
from typing import TYPE_CHECKING

import pytest
from pydantic import JsonValue

from sve_carddb.catalog.adoption_sources import PinnedRepository
from sve_carddb.snapshot.values import array, canonical, digest, object_value, parse
from sve_carddb.template_parameter_rules import loader as loader_module
from sve_carddb.template_parameter_rules import replay as replay_module
from sve_carddb.template_parameter_rules.replay import (
    ProposalInputs,
    Replay,
    ResolvedSlotsChangedError,
    _evidence,
    _resolve,
    compare_replays,
    replay,
    resolved_identity,
)
from sve_carddb.template_parameters.inventory import Candidates
from sve_carddb.template_parameters.references import adopted

from .adoption_fixtures import commit
from .recognition_policy_fixtures import pair, policy_git, publish
from .recognition_replay_fixtures import (
    SourceCase,
    changed_sign_source,
    recognition_source,
    recognition_term_source,
)
from .test_template_parameters import candidate
from .translation_fixtures import envelope, term, write

if TYPE_CHECKING:
    from pathlib import Path

    from pytest_mock import MockerFixture

    from sve_carddb.template_sources.models import Recipe

__all__ = (
    "changed_sign_source",
    "policy_git",
    "recognition_source",
    "recognition_term_source",
)


def changed(case: SourceCase, key: str, value: JsonValue) -> Recipe:
    config = copy.deepcopy(case.recipe.config)
    config[key] = value
    return case.recipe.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )


def run(case: SourceCase, recipe: Recipe | None = None) -> Replay:
    return replay(
        PinnedRepository(case.repository),
        recipe or case.recipe,
        {"test-store": case.store},
        main_revision=case.main,
        legacy_bytes=case.legacy,
        proposals=case.proposals,
    )


@pytest.fixture(scope="module")
def verified_replay(recognition_source: SourceCase) -> Replay:
    return run(recognition_source)


@pytest.fixture(scope="module")
def verified_term_replay(recognition_term_source: SourceCase) -> Replay:
    return run(recognition_term_source)


def test_glossary_upgrade_ambiguity_lists_the_lost_resolution(
    recognition_term_source: SourceCase, verified_term_replay: Replay, tmp_path: Path
) -> None:
    case = recognition_term_source
    root = tmp_path / "glossary-upgrade"
    shutil.copytree(case.repository, root)
    path = root / "authored/translations/glossary/concepts/001.yaml"
    original = object_value(array(object_value(parse(path.read_bytes()))["records"])[0])
    other = term("stat.other")
    object_value(other["data"]).update(
        source_ref=None,
        authored_source_ja="攻撃力",
        missing_source_reason="Synthetic fixture unavailable source",
    )
    write(
        root / "authored",
        {"translations/glossary/concepts/001.yaml": envelope([original, other])},
    )
    revision = commit(root)
    refs = adopted(
        root / "authored",
        _evidence(PinnedRepository(root), case.recipe, {"test-store": case.store}),
    )
    config = copy.deepcopy(case.recipe.config)
    references = object_value(config["references"])
    references.update(refs.pins)
    object_value(references["glossary"])["authored_revision"] = revision
    recipe = case.recipe.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )
    current = run(replace(case, repository=root, main=revision, recipe=recipe))
    old = [
        raw
        for raw in verified_term_replay.resolved_slots
        if object_value(parse(raw))["rule_id"] == "braced_stat_reference"
    ]
    assert len(old) == 1
    assert not any(
        object_value(parse(raw))["rule_id"] == "braced_stat_reference"
        for raw in current.resolved_slots
    )
    assert any(
        "ambiguous_or_missing_term_concept" in array(object_value(parse(raw))["issues"])
        for raw in current.remaining_slots
    )
    assert current.numeric_positions == verified_term_replay.numeric_positions
    assert current.fingerprints == verified_term_replay.fingerprints
    affected = resolved_identity(old[0])
    message = (
        "Recognition replay lost or changed previously resolved slots: "
        + canonical([parse(affected)]).decode()
    )
    with pytest.raises(
        ResolvedSlotsChangedError, match="^" + re.escape(message) + "$"
    ) as caught:
        compare_replays(verified_term_replay, current)
    assert caught.value.affected_slots == (affected,)
    assert caught.value.comparison.added_resolved_slots == ()
    assert caught.value.comparison.remaining_slots == current.remaining_slots


@pytest.mark.parametrize(
    "mutation",
    [
        "missing",
        "slot",
        "rule_id",
        "source_segments",
        "raw_hash",
        "role",
        "value",
        "concept_id",
        "concept_hash",
    ],
)
def test_resolved_identity_cannot_disappear_or_be_rebound(
    verified_term_replay: Replay, mutation: str
) -> None:
    previous = verified_term_replay
    before = next(
        raw
        for raw in previous.resolved_slots
        if object_value(parse(raw))["rule_id"] == "braced_stat_reference"
    )
    row = object_value(parse(before))
    if mutation in {"slot", "rule_id"}:
        row[mutation] = "other"
    elif mutation in {"raw_hash", "value"}:
        row[mutation] = digest(b"different") if mutation == "raw_hash" else 1
    elif mutation == "source_segments":
        row[mutation] = [{"start": 0, "end": 1}]
    elif mutation == "role":
        row["recognized_role"] = "other"
    elif mutation in {"concept_id", "concept_hash"}:
        object_value(row["match_evidence"])[
            "target_id" if mutation == "concept_id" else "target_hash"
        ] = "term:stat.other" if mutation == "concept_id" else digest(b"different")
    entries = tuple(raw for raw in previous.resolved_slots if raw != before)
    if mutation != "missing":
        entries = (*entries, canonical(row))
    current = replace(previous, resolved_slots=tuple(sorted(entries)))
    affected = resolved_identity(before)
    message = (
        "Recognition replay lost or changed previously resolved slots: "
        + canonical([parse(affected)]).decode()
    )
    with pytest.raises(
        ResolvedSlotsChangedError, match="^" + re.escape(message) + "$"
    ) as caught:
        compare_replays(previous, current)
    assert caught.value.affected_slots == (affected,)


def test_added_resolution_and_pending_causes_are_reported_separately(
    verified_term_replay: Replay,
) -> None:
    previous = verified_term_replay
    added = object_value(parse(previous.resolved_slots[0]))
    added["inventory_id"] = "synthetic-added"
    updated = []
    for raw in previous.resolved_slots:
        row = object_value(parse(raw))
        row["remaining_issues"] = []
        updated.append(canonical(row))
    current = replace(
        previous,
        resolved_slots=tuple(sorted((*updated, canonical(added)))),
        remaining_slots=(),
    )
    comparison = compare_replays(previous, current)
    assert comparison.added_resolved_slots == (resolved_identity(canonical(added)),)
    assert comparison.remaining_slots == ()


def test_body_only_policy_does_not_resolve_a_reminder(
    recognition_source: SourceCase, tmp_path: Path
) -> None:
    case = recognition_source
    root = tmp_path / "body-only"
    shutil.copytree(case.repository, root)
    policy, receipt = pair(
        case.main, policy_id="synthetic-body-v1", store="test-store", batch=case.batch
    )
    object_value(policy["scope"])["roles"] = ["body"]
    event = object_value(object_value(receipt["events"])["event_20261002_2"])
    object_value(event["authorization_basis"])["event_locator"] = (
        "00000000-0000-0000-0000-000000000003"
    )
    revision = publish(root, policy, receipt)
    pin: dict[str, JsonValue] = {
        "policy_id": policy["policy_id"],
        "authored_revision": revision,
        "path": "authored/template-parameter-rules/synthetic-body-v1.policy.yaml",
        "hash": receipt["policy_hash"],
        "approval_receipt_hash": digest(canonical(receipt)),
    }
    result = run(
        replace(case, repository=root), changed(case, "recognition_policy", pin)
    )
    assert len(result.resolved_slots) == 1
    assert (
        object_value(parse(result.resolved_slots[0]))["rule_id"]
        == "suffix_damage_amount"
    )
    assert not object_value(parse(result.resolved_slots[0]))["remaining_issues"]
    assert any(
        "numeric_role_requires_review" in array(object_value(parse(raw))["issues"])
        for raw in result.remaining_slots
    )


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        ("old", "Recognition new rules cannot claim old numeric ownership"),
        ("duplicate", "Recognition matches must have unique slot ownership"),
    ],
)
def test_resolution_defenses_reject_synthetic_conflicting_ownership(
    recognition_source: SourceCase, mutation: str, message: str
) -> None:
    old = candidate("コスト2ダメージ")
    match: dict[str, JsonValue] = {
        "inventory_id": old.inventory_id,
        "slot": old.slots[0].name,
        "rule_id": "suffix_damage_amount",
    }
    values = Candidates(
        entries=[old],
        rule_matches=[match] if mutation == "old" else [match, dict(match)],
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        _resolve(recognition_source.loaded, values)


def test_resolution_never_clears_invalid_unsigned_value(
    recognition_source: SourceCase,
) -> None:
    old = candidate("試験2枚")
    hint = old.slots[0].model_copy(
        update={
            "issues": tuple(
                sorted((*old.slots[0].issues, "invalid_safe_unsigned_decimal"))
            )
        }
    )
    candidate_with_invalid_value = old.model_copy(update={"slots": (hint,)})
    resolved, pending = _resolve(
        recognition_source.loaded, Candidates(entries=[candidate_with_invalid_value])
    )
    assert not resolved
    assert any(
        "numeric_rule_pending_approval" in array(object_value(parse(raw))["issues"])
        for raw in pending
    )


def test_policy_decode_cost_does_not_grow_with_resolved_positions(
    recognition_source: SourceCase, mocker: MockerFixture
) -> None:
    item = candidate("甲2枚" * 16)
    decode = mocker.spy(loader_module, "parse_policy")
    resolved, pending = _resolve(recognition_source.loaded, Candidates(entries=[item]))
    assert len(resolved) == len(item.slots) == 16
    assert not pending
    assert decode.call_count <= 2


def test_first_batch_count_guard_cannot_be_skipped(
    recognition_source: SourceCase, monkeypatch: pytest.MonkeyPatch
) -> None:
    monkeypatch.setattr(replay_module, "JP_BATCH", recognition_source.batch)
    with pytest.raises(
        ValueError,
        match=r"^Recognition first JP batch differs from its fixed baseline counts$",
    ):
        run(recognition_source)


def test_full_source_replay_resolves_only_authorized_causes(
    verified_replay: Replay,
) -> None:
    result = verified_replay
    assert len(result.numeric_positions) == 1
    assert object_value(parse(result.numeric_positions[0]))["slot"] == "slot_1"
    matched = [object_value(parse(raw)) for raw in result.resolved_slots]
    assert {r["rule_id"] for r in matched} == {
        "suffix_unit_cards",
        "suffix_damage_amount",
        "prefix_cost_delta",
    }
    assert len(matched) == 4
    pending = [object_value(parse(raw)) for raw in result.remaining_slots]
    assert any("missing_card_name_concept" in array(r["issues"]) for r in pending)
    assert any(
        "legacy_parenthesis_classification_requires_review" in array(r["issues"])
        for r in pending
    )
    assert any("vocabulary_not_adopted" in array(r["issues"]) for r in pending)
    assert result.complete is False
    assert object_value(parse(result.source_coverage))["complete"] is True
    assert (
        object_value(object_value(parse(result.checkpoint))["fingerprints"])["expected"]
        == 4
    )
    compare_replays(result, result)


def test_null_policy_does_not_enable_candidate_rules(
    recognition_source: SourceCase,
) -> None:
    recipe = changed(recognition_source, "recognition_policy", None)
    result = run(recognition_source, recipe)
    assert result.policy_pin is None
    assert not result.resolved_slots
    assert result.remaining_slots


@pytest.mark.parametrize(
    ("mutation", "message"),
    [
        (
            "config_hash",
            "Recognition parameter recipe ID, program or config hash is invalid",
        ),
        (
            "code_hash",
            "Recognition replay implementation differs from its immutable recipe",
        ),
        (
            "source_recipes",
            "Recognition replay requires its complete source recipe pins",
        ),
        ("batch_scope", "Recognition frozen batch differs from the policy scope"),
        (
            "batch_fields",
            "Recognition replay requires one explicit frozen source batch",
        ),
        ("legacy_hash", "Recognition legacy baseline differs from its recipe hash"),
        (
            "glossary_index",
            "Recognition glossary exact bytes differ from their revision pin",
        ),
        (
            "glossary_shard",
            "Recognition glossary exact bytes differ from their revision pin",
        ),
        (
            "glossary_revision",
            "Recognition glossary exact bytes differ from their revision pin",
        ),
        (
            "glossary_shards_missing",
            "Recognition glossary pin must cover its complete Git file closure",
        ),
        (
            "glossary_canonical",
            "Recognition glossary canonical index or shard pins differ",
        ),
        (
            "glossary_concepts",
            "Recognition glossary exact concept bindings differ from the recipe",
        ),
    ],
)
def test_pinned_source_closure_refusals(  # ruff: ignore[complex-structure,too-many-branches] -- independent malformed pin variants target separate domain refusals
    recognition_source: SourceCase, mutation: str, message: str
) -> None:
    case = recognition_source
    config = copy.deepcopy(case.recipe.config)
    if mutation == "source_recipes":
        config["source_recipes"] = []
    elif mutation == "batch_scope":
        object_value(config["source_batch"])["batch_id"] = digest(b"wrong")
    elif mutation == "batch_fields":
        object_value(config["source_batch"])["extra"] = True
    elif mutation == "legacy_hash":
        config["legacy_file_hash"] = digest(b"wrong")
    elif mutation.startswith("glossary_"):
        glossary = object_value(object_value(config["references"])["glossary"])
        if mutation == "glossary_index":
            glossary["index_hash"] = digest(b"wrong")
        elif mutation == "glossary_shard":
            object_value(array(glossary["shards"])[0])["exact_hash"] = digest(b"wrong")
        elif mutation == "glossary_revision":
            glossary["authored_revision"] = case.loaded.historical_revision
            message = (
                "Recognition glossary pin must cover its complete Git file closure"
            )
        elif mutation == "glossary_shards_missing":
            glossary["shards"] = []
        elif mutation == "glossary_canonical":
            object_value(array(glossary["shards"])[0])["canonical_hash"] = digest(
                b"wrong"
            )
        else:
            object_value(config["references"])["exact_concepts_hash"] = digest(b"wrong")
    recipe = case.recipe.model_copy(
        update={"config": config, "config_hash": digest(canonical(config))}
    )
    if mutation == "config_hash":
        recipe = recipe.model_copy(update={"config_hash": digest(b"wrong")})
    elif mutation == "code_hash":
        recipe = recipe.model_copy(update={"code_hash": digest(b"wrong")})
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        run(case, recipe)


def test_missing_store_is_a_refusal(recognition_source: SourceCase) -> None:
    with pytest.raises(
        ValueError, match=r"^Recognition frozen source store is not configured$"
    ):
        replay(
            PinnedRepository(recognition_source.repository),
            recognition_source.recipe,
            {},
            main_revision=recognition_source.main,
            legacy_bytes=recognition_source.legacy,
        )


@pytest.mark.parametrize("mutation", ["slot", "value", "raw_hash", "source_segments"])
def test_identity_not_total_count(verified_replay: Replay, mutation: str) -> None:
    previous = verified_replay
    identity = object_value(parse(previous.numeric_positions[0]))
    if mutation == "slot":
        identity["slot"] = "slot_other"
    elif mutation == "value":
        identity["value"] = 3
    elif mutation == "raw_hash":
        identity["raw_hash"] = digest(b"wrong")
    else:
        identity["source_segments"] = [{"start": 0, "end": 1}]
    current = replace(previous, numeric_positions=(canonical(identity),))
    assert len(current.numeric_positions) == len(previous.numeric_positions)
    with pytest.raises(
        ValueError, match=r"^Recognition replay changed an existing numeric position$"
    ):
        compare_replays(previous, current)


def test_fingerprint_use_cannot_change_at_equal_count(
    verified_replay: Replay,
) -> None:
    previous = verified_replay
    current = replace(
        previous,
        fingerprints=(digest(b"different").encode(), *previous.fingerprints[1:]),
    )
    with pytest.raises(
        ValueError,
        match=r"^Recognition replay changed a legacy fingerprint or source use$",
    ):
        compare_replays(previous, current)


def test_sign_bridge_requires_zero_original_position_impact(
    changed_sign_source: SourceCase,
) -> None:
    with pytest.raises(
        ValueError,
        match=r"^Recognition sign restriction changes an original numeric position$",
    ):
        run(changed_sign_source)


def test_unknown_coverage_cannot_be_claimed_complete(verified_replay: Replay) -> None:
    unknown = replace(
        verified_replay,
        source_coverage=canonical({"complete": False, "field_states": {"unknown": 1}}),
        remaining_slots=(),
    )
    assert unknown.complete is False
    complete = replace(unknown, source_coverage=canonical({"complete": True}))
    assert complete.complete is True


def test_different_batch_cannot_borrow_an_old_baseline(verified_replay: Replay) -> None:
    recipe = object_value(parse(verified_replay.recipe))
    object_value(object_value(recipe["config"])["source_batch"])["batch_id"] = digest(
        b"other batch"
    )
    different = replace(verified_replay, recipe=canonical(recipe))
    with pytest.raises(
        ValueError,
        match=r"^Recognition replay comparison requires the same frozen batch$",
    ):
        compare_replays(verified_replay, different)


@pytest.mark.parametrize("mutation", ["missing", "vocabulary", "basis"])
def test_proposed_vocabulary_is_pinned_and_never_adopted(
    recognition_source: SourceCase, mutation: str
) -> None:
    case = recognition_source
    proposals = (
        None
        if mutation == "missing"
        else ProposalInputs(
            b"{}" if mutation == "vocabulary" else case.proposals.vocabulary,
            b"different" if mutation == "basis" else case.proposals.basis,
        )
    )
    message = (
        "Recognition replay cannot omit pinned vocabulary proposal inputs"
        if mutation == "missing"
        else "Recognition vocabulary proposal or basis hash differs from its recipe"
    )
    with pytest.raises(ValueError, match="^" + re.escape(message) + "$"):
        replay(
            PinnedRepository(case.repository),
            case.recipe,
            {"test-store": case.store},
            main_revision=case.main,
            legacy_bytes=case.legacy,
            proposals=proposals,
        )
